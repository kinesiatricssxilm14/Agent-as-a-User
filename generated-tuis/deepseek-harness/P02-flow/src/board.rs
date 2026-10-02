//! Board data model and all real-filesystem operations.
//!
//! On-disk layout (relative to the board root directory, e.g. `/bench/data/board`):
//!
//! ```text
//! board.txt                       # column definitions: `col <column_id> "<display_name>"`
//! cols/<column_id>/order.txt      # card ids, one per line (no extension)
//! cols/<column_id>/<card_id>.md   # line 1 `# <title>`, then body text
//! ```

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Card {
    pub id: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct Column {
    pub id: String,
    pub name: String,
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone)]
pub struct Board {
    pub columns: Vec<Column>,
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

pub fn load_board(root: &Path) -> Result<Board, String> {
    fs::create_dir_all(root).map_err(io)?;
    let board_file = root.join("board.txt");
    if !board_file.exists() {
        fs::write(&board_file, "").map_err(io)?;
    }
    let content = fs::read_to_string(&board_file).map_err(io)?;
    let mut columns = Vec::new();
    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((id, name)) = parse_col_line(t) {
            let cards = load_cards(root, &id)?;
            columns.push(Column { id, name, cards });
        }
    }
    Ok(Board { columns })
}

fn load_cards(root: &Path, col_id: &str) -> Result<Vec<Card>, String> {
    let dir = root.join("cols").join(col_id);
    let order_file = dir.join("order.txt");
    let mut ids = read_order(&order_file)?;

    // Defensively include any `.md` files present in the directory that are not
    // listed in order.txt, so cards are never silently hidden.
    if dir.exists() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    if let Some(stem) = path.file_stem() {
                        let id = stem.to_string_lossy().to_string();
                        if !ids.iter().any(|x| x == &id) {
                            ids.push(id);
                        }
                    }
                }
            }
        }
    }

    let mut cards = Vec::new();
    for id in ids {
        let path = dir.join(format!("{}.md", id));
        if !path.exists() {
            continue;
        }
        if let Ok((title, body)) = read_card(&path) {
            cards.push(Card { id, title, body });
        }
    }
    Ok(cards)
}

fn read_card(path: &Path) -> Result<(String, String), String> {
    let content = fs::read_to_string(path).map_err(io)?;
    let (first, body) = match content.split_once('\n') {
        Some((f, b)) => (f, b.to_string()),
        None => (content.as_str(), String::new()),
    };
    let title = first
        .strip_prefix("# ")
        .or_else(|| first.strip_prefix('#'))
        .unwrap_or(first)
        .to_string();
    Ok((title, body))
}

// ---------------------------------------------------------------------------
// Card operations
// ---------------------------------------------------------------------------

/// Create a new card and return its generated id.
pub fn create_card(root: &Path, col_id: &str, title: &str, body: &str) -> Result<String, String> {
    let id = unique_card_id(root, col_id, title);
    let dir = root.join("cols").join(col_id);
    fs::create_dir_all(&dir).map_err(io)?;
    let path = card_path(root, col_id, &id);
    write_card(&path, title, body)?;
    append_order(root, col_id, &id)?;
    Ok(id)
}

/// Replace the title (line 1) of a card, keeping the `# ` prefix.
pub fn edit_title(root: &Path, col_id: &str, card_id: &str, title: &str) -> Result<(), String> {
    let path = card_path(root, col_id, card_id);
    let (_, body) = read_card(&path)?;
    write_card(&path, title, &body)
}

/// Append one line to the end of a card's body.
pub fn append_body(root: &Path, col_id: &str, card_id: &str, text: &str) -> Result<(), String> {
    let path = card_path(root, col_id, card_id);
    let (title, body) = read_card(&path)?;
    let mut b = body;
    if !b.is_empty() && !b.ends_with('\n') {
        b.push('\n');
    }
    b.push_str(text);
    b.push('\n');
    write_card(&path, &title, &b)
}

/// Delete a card file and remove it from the column's order.
pub fn delete_card(root: &Path, col_id: &str, card_id: &str) -> Result<(), String> {
    let path = card_path(root, col_id, card_id);
    if path.exists() {
        fs::remove_file(&path).map_err(io)?;
    }
    remove_order(root, col_id, card_id)
}

/// Move a card between two columns on disk, updating both order files.
pub fn move_card(root: &Path, from_id: &str, to_id: &str, card_id: &str) -> Result<(), String> {
    let from_dir = root.join("cols").join(from_id);
    let to_dir = root.join("cols").join(to_id);
    fs::create_dir_all(&to_dir).map_err(io)?;
    let src = from_dir.join(format!("{}.md", card_id));
    let dst = to_dir.join(format!("{}.md", card_id));
    if !src.exists() {
        return Err(format!("card file not found: {}", src.display()));
    }
    if dst.exists() {
        return Err(format!("card already exists in target column: {}", dst.display()));
    }
    fs::rename(&src, &dst).map_err(io)?;
    remove_order(root, from_id, card_id)?;
    append_order(root, to_id, card_id)
}

// ---------------------------------------------------------------------------
// Column operations
// ---------------------------------------------------------------------------

/// Append a new column definition to board.txt, create its directory, and
/// return the generated column id.
pub fn create_column(root: &Path, name: &str) -> Result<String, String> {
    let name = sanitize_line(name);
    if name.trim().is_empty() {
        return Err("column name must not be empty".to_string());
    }
    let id = unique_column_id(root, &name);
    let line = format!("col {} {}\n", id, quote_name(&name));
    let board_file = root.join("board.txt");
    let mut content = if board_file.exists() {
        fs::read_to_string(&board_file).map_err(io)?
    } else {
        String::new()
    };
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&line);
    fs::write(&board_file, content).map_err(io)?;

    let dir = root.join("cols").join(&id);
    fs::create_dir_all(&dir).map_err(io)?;
    fs::write(dir.join("order.txt"), "").map_err(io)?;
    Ok(id)
}

/// Remove a column from board.txt and delete its directory tree.
pub fn delete_column(root: &Path, col_id: &str) -> Result<(), String> {
    let board_file = root.join("board.txt");
    let content = if board_file.exists() {
        fs::read_to_string(&board_file).map_err(io)?
    } else {
        String::new()
    };
    let mut out = String::new();
    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some((id, _)) = parse_col_line(t) {
            if id == col_id {
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    fs::write(&board_file, out).map_err(io)?;

    let dir = root.join("cols").join(col_id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(io)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn card_path(root: &Path, col_id: &str, card_id: &str) -> PathBuf {
    root.join("cols").join(col_id).join(format!("{}.md", card_id))
}

fn write_card(path: &Path, title: &str, body: &str) -> Result<(), String> {
    let title = sanitize_line(title);
    let mut content = format!("# {}\n", title);
    content.push_str(body);
    fs::write(path, content).map_err(io)
}

fn read_order(path: &Path) -> Result<Vec<String>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(io)?;
    Ok(content
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

fn write_order(path: &Path, ids: &[String]) -> Result<(), String> {
    let mut s = ids.join("\n");
    if !s.is_empty() {
        s.push('\n');
    }
    fs::write(path, s).map_err(io)
}

fn append_order(root: &Path, col_id: &str, card_id: &str) -> Result<(), String> {
    let dir = root.join("cols").join(col_id);
    fs::create_dir_all(&dir).map_err(io)?;
    let order_file = dir.join("order.txt");
    let mut ids = read_order(&order_file)?;
    if !ids.iter().any(|x| x == card_id) {
        ids.push(card_id.to_string());
    }
    write_order(&order_file, &ids)
}

fn remove_order(root: &Path, col_id: &str, card_id: &str) -> Result<(), String> {
    let dir = root.join("cols").join(col_id);
    let order_file = dir.join("order.txt");
    let mut ids = read_order(&order_file)?;
    ids.retain(|x| x != card_id);
    write_order(&order_file, &ids)
}

fn unique_card_id(root: &Path, col_id: &str, title: &str) -> String {
    let dir = root.join("cols").join(col_id);
    let base = slugify(title);
    let base = if base.is_empty() {
        "card".to_string()
    } else {
        base
    };
    let mut id = base.clone();
    let mut n = 2usize;
    while dir.join(format!("{}.md", id)).exists() {
        id = format!("{}-{}", base, n);
        n += 1;
    }
    id
}

fn unique_column_id(root: &Path, name: &str) -> String {
    let base = slugify(name);
    let base = if base.is_empty() {
        "column".to_string()
    } else {
        base
    };
    let mut id = base.clone();
    let mut n = 2usize;
    while column_id_exists(root, &id) {
        id = format!("{}-{}", base, n);
        n += 1;
    }
    id
}

fn column_id_exists(root: &Path, id: &str) -> bool {
    let board_file = root.join("board.txt");
    if let Ok(content) = fs::read_to_string(&board_file) {
        for line in content.lines() {
            if let Some((i, _)) = parse_col_line(line) {
                if i == id {
                    return true;
                }
            }
        }
    }
    root.join("cols").join(id).exists()
}

// ---------------------------------------------------------------------------
// Text utilities
// ---------------------------------------------------------------------------

/// Turn arbitrary text into a filesystem-safe id.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in s.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !out.is_empty() && !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    let mut truncated: String = out.chars().take(48).collect();
    while truncated.ends_with('-') {
        truncated.pop();
    }
    truncated
}

/// Strip newlines so the value can never corrupt the line-based formats.
fn sanitize_line(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
}

/// Quote a display name for the `col <id> "<name>"` line, escaping backslashes
/// and double quotes so arbitrary names round-trip.
fn quote_name(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

fn parse_col_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    let after = line.strip_prefix("col")?;
    let first = after.chars().next()?;
    if !first.is_whitespace() {
        return None;
    }
    let rest = after.trim_start();
    let idx = rest.find(|c: char| c.is_whitespace())?;
    let (id, rest) = rest.split_at(idx);
    let id = id.trim();
    if id.is_empty() {
        return None;
    }
    let name = parse_quoted(rest)?;
    Some((id.to_string(), name))
}

fn parse_quoted(s: &str) -> Option<String> {
    let s = s.trim();
    if !s.starts_with('"') {
        return Some(s.to_string());
    }
    let mut out = String::new();
    let mut chars = s[1..].chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let next = chars.next()?;
                out.push(next);
            }
            '"' => break,
            _ => out.push(c),
        }
    }
    Some(out)
}

fn io(e: std::io::Error) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "toolb-test-{}-{}",
            std::process::id(),
            n
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trip_board_and_cards() {
        let root = tmp_root();

        // create two columns
        let todo = create_column(&root, "TO DO").unwrap();
        let doing = create_column(&root, "DOING").unwrap();
        assert_eq!(todo, "to-do");
        assert_eq!(doing, "doing");

        // create a card
        let card = create_card(&root, &todo, "Fix login bug", "Investigate timeout.").unwrap();
        assert_eq!(card, "fix-login-bug");

        // board.txt and files on disk
        let board_txt = fs::read_to_string(root.join("board.txt")).unwrap();
        assert!(board_txt.contains("col to-do \"TO DO\""));
        let card_file = fs::read_to_string(root.join("cols").join(&todo).join(format!("{}.md", card))).unwrap();
        assert_eq!(card_file, "# Fix login bug\nInvestigate timeout.");
        let order = fs::read_to_string(root.join("cols").join(&todo).join("order.txt")).unwrap();
        assert!(order.lines().any(|l| l == card));

        // load and verify
        let board = load_board(&root).unwrap();
        assert_eq!(board.columns.len(), 2);
        assert_eq!(board.columns[0].name, "TO DO");
        assert_eq!(board.columns[0].cards[0].title, "Fix login bug");
        assert_eq!(board.columns[0].cards[0].body, "Investigate timeout.");

        // edit title
        edit_title(&root, &todo, &card, "Fix login bug (urgent)").unwrap();
        let board = load_board(&root).unwrap();
        assert_eq!(board.columns[0].cards[0].title, "Fix login bug (urgent)");
        assert_eq!(board.columns[0].cards[0].body, "Investigate timeout.");

        // append body
        append_body(&root, &todo, &card, "Also check mobile.").unwrap();
        let board = load_board(&root).unwrap();
        assert_eq!(board.columns[0].cards[0].body, "Investigate timeout.\nAlso check mobile.\n");

        // move card
        move_card(&root, &todo, &doing, &card).unwrap();
        let board = load_board(&root).unwrap();
        assert!(board.columns[0].cards.is_empty());
        assert_eq!(board.columns[1].cards[0].id, card);
        assert!(root.join("cols").join(&doing).join(format!("{}.md", card)).exists());

        // delete card
        delete_card(&root, &doing, &card).unwrap();
        let board = load_board(&root).unwrap();
        assert!(board.columns[1].cards.is_empty());

        // delete column
        delete_column(&root, &doing).unwrap();
        let board = load_board(&root).unwrap();
        assert_eq!(board.columns.len(), 1);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn slug_and_quoting() {
        assert_eq!(slugify("  Fix Login  Bug! "), "fix-login-bug");
        assert_eq!(slugify("$$$"), "");
        assert_eq!(quote_name("A \"quoted\" name"), "\"A \\\"quoted\\\" name\"");
        let parsed = parse_col_line("col my-id \"A \\\"quoted\\\" name\"").unwrap();
        assert_eq!(parsed, ("my-id".to_string(), "A \"quoted\" name".to_string()));
    }

    #[test]
    fn missing_board_is_created_empty() {
        let root = tmp_root().join("fresh");
        let board = load_board(&root).unwrap();
        assert!(board.columns.is_empty());
        assert!(root.join("board.txt").exists());
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }
}
