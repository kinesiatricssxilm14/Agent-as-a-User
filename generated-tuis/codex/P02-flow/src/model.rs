use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub id: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub id: String,
    pub name: String,
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone)]
pub struct Board {
    pub root: PathBuf,
    pub columns: Vec<Column>,
}

impl Board {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("cols"))
            .with_context(|| format!("create board directory {}", root.display()))?;
        let board_path = root.join("board.txt");
        if !board_path.exists() {
            atomic_write(&board_path, b"")?;
        }
        Self::load(root)
    }

    pub fn load(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let contents = fs::read_to_string(root.join("board.txt"))
            .with_context(|| format!("read {}", root.join("board.txt").display()))?;
        let mut columns = Vec::new();
        let mut ids = HashSet::new();
        for (index, line) in contents.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (id, name) =
                parse_column_line(line).with_context(|| format!("board.txt line {}", index + 1))?;
            if !ids.insert(id.clone()) {
                bail!("duplicate column id '{id}' in board.txt");
            }
            columns.push(load_column(&root, id, name)?);
        }
        Ok(Self { root, columns })
    }

    pub fn reload(&mut self) -> Result<()> {
        *self = Self::load(self.root.clone())?;
        Ok(())
    }

    pub fn create_column(&mut self, id: &str, name: &str) -> Result<()> {
        validate_id(id, "column")?;
        validate_text(name, "column display name")?;
        if self.columns.iter().any(|column| column.id == id) {
            bail!("column id '{id}' already exists");
        }
        if self.columns.iter().any(|column| column.name == name) {
            bail!("column display name '{name}' already exists");
        }
        let dir = self.root.join("cols").join(id);
        fs::create_dir(&dir).with_context(|| format!("create {}", dir.display()))?;
        if let Err(error) = atomic_write(&dir.join("order.txt"), b"") {
            let _ = fs::remove_dir(&dir);
            return Err(error);
        }
        let mut definitions: Vec<(String, String)> = self
            .columns
            .iter()
            .map(|column| (column.id.clone(), column.name.clone()))
            .collect();
        definitions.push((id.to_owned(), name.to_owned()));
        if let Err(error) = self.write_definitions(&definitions) {
            let _ = fs::remove_file(dir.join("order.txt"));
            let _ = fs::remove_dir(&dir);
            return Err(error);
        }
        self.columns.push(Column {
            id: id.to_owned(),
            name: name.to_owned(),
            cards: Vec::new(),
        });
        Ok(())
    }

    pub fn create_card(&mut self, column: usize, id: &str, title: &str) -> Result<()> {
        validate_id(id, "card")?;
        validate_text(title, "card title")?;
        let col = self.column(column)?;
        let path = self.card_path(column, id)?;
        if path.exists() || col.cards.iter().any(|card| card.id == id) {
            bail!("card id '{id}' already exists in {}", col.name);
        }
        let contents = format!("# {title}\n");
        atomic_write(&path, contents.as_bytes())?;
        let mut order: Vec<String> = col.cards.iter().map(|card| card.id.clone()).collect();
        order.push(id.to_owned());
        if let Err(error) = self.write_order(column, &order) {
            let _ = fs::remove_file(&path);
            return Err(error);
        }
        self.columns[column].cards.push(Card {
            id: id.to_owned(),
            title: title.to_owned(),
            body: String::new(),
        });
        Ok(())
    }

    pub fn edit_title(&mut self, column: usize, card: usize, title: &str) -> Result<()> {
        validate_text(title, "card title")?;
        let selected = self.card(column, card)?.clone();
        let contents = render_card(title, &selected.body);
        atomic_write(&self.card_path(column, &selected.id)?, contents.as_bytes())?;
        self.columns[column].cards[card].title = title.to_owned();
        Ok(())
    }

    pub fn append_body_line(&mut self, column: usize, card: usize, line: &str) -> Result<()> {
        if line.contains(['\n', '\r']) {
            bail!("body line cannot contain a newline");
        }
        let selected = self.card(column, card)?.clone();
        let body = if selected.body.is_empty() {
            line.to_owned()
        } else {
            format!("{}\n{line}", selected.body)
        };
        let contents = render_card(&selected.title, &body);
        atomic_write(&self.card_path(column, &selected.id)?, contents.as_bytes())?;
        self.columns[column].cards[card].body = body;
        Ok(())
    }

    pub fn delete_card(&mut self, column: usize, card: usize) -> Result<()> {
        let selected = self.card(column, card)?.clone();
        let current = self.column(column)?;
        let order: Vec<String> = current
            .cards
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != card)
            .map(|(_, item)| item.id.clone())
            .collect();
        let path = self.card_path(column, &selected.id)?;
        fs::remove_file(&path).with_context(|| format!("delete {}", path.display()))?;
        if let Err(error) = self.write_order(column, &order) {
            // Preserve discoverability if updating order fails: restore the card.
            let _ = atomic_write(
                &path,
                render_card(&selected.title, &selected.body).as_bytes(),
            );
            return Err(error);
        }
        self.columns[column].cards.remove(card);
        Ok(())
    }

    pub fn move_card(&mut self, from: usize, card: usize, to: usize) -> Result<()> {
        if from == to {
            bail!("card is already in that column");
        }
        let selected = self.card(from, card)?.clone();
        let target = self.column(to)?;
        if target.cards.iter().any(|item| item.id == selected.id)
            || self.card_path(to, &selected.id)?.exists()
        {
            bail!("target column already has card id '{}'", selected.id);
        }
        let source_path = self.card_path(from, &selected.id)?;
        let target_path = self.card_path(to, &selected.id)?;
        fs::rename(&source_path, &target_path).with_context(|| {
            format!(
                "move {} to {}",
                source_path.display(),
                target_path.display()
            )
        })?;

        let source_order: Vec<String> = self.columns[from]
            .cards
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != card)
            .map(|(_, item)| item.id.clone())
            .collect();
        let mut target_order: Vec<String> = self.columns[to]
            .cards
            .iter()
            .map(|item| item.id.clone())
            .collect();
        target_order.push(selected.id.clone());

        if let Err(error) = self
            .write_order(from, &source_order)
            .and_then(|_| self.write_order(to, &target_order))
        {
            let _ = fs::rename(&target_path, &source_path);
            let old_source: Vec<String> = self.columns[from]
                .cards
                .iter()
                .map(|item| item.id.clone())
                .collect();
            let old_target: Vec<String> = self.columns[to]
                .cards
                .iter()
                .map(|item| item.id.clone())
                .collect();
            let _ = self.write_order(from, &old_source);
            let _ = self.write_order(to, &old_target);
            return Err(error);
        }

        let moved = self.columns[from].cards.remove(card);
        self.columns[to].cards.push(moved);
        Ok(())
    }

    fn column(&self, index: usize) -> Result<&Column> {
        self.columns
            .get(index)
            .ok_or_else(|| anyhow!("no column selected"))
    }

    fn card(&self, column: usize, card: usize) -> Result<&Card> {
        self.column(column)?
            .cards
            .get(card)
            .ok_or_else(|| anyhow!("no card selected"))
    }

    fn card_path(&self, column: usize, card_id: &str) -> Result<PathBuf> {
        Ok(self
            .root
            .join("cols")
            .join(&self.column(column)?.id)
            .join(format!("{card_id}.md")))
    }

    fn write_order(&self, column: usize, ids: &[String]) -> Result<()> {
        let mut text = ids.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        let path = self
            .root
            .join("cols")
            .join(&self.column(column)?.id)
            .join("order.txt");
        atomic_write(&path, text.as_bytes())
    }

    fn write_definitions(&self, definitions: &[(String, String)]) -> Result<()> {
        let mut text = String::new();
        for (id, name) in definitions {
            text.push_str("col ");
            text.push_str(id);
            text.push_str(" \"");
            text.push_str(&escape_name(name));
            text.push_str("\"\n");
        }
        atomic_write(&self.root.join("board.txt"), text.as_bytes())
    }
}

fn load_column(root: &Path, id: String, name: String) -> Result<Column> {
    let dir = root.join("cols").join(&id);
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let order_path = dir.join("order.txt");
    if !order_path.exists() {
        atomic_write(&order_path, b"")?;
    }
    let order_text = fs::read_to_string(&order_path)
        .with_context(|| format!("read {}", order_path.display()))?;
    let mut ordered_ids = Vec::new();
    let mut seen = HashSet::new();
    for value in order_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        validate_id(value, "card")
            .with_context(|| format!("invalid entry in {}", order_path.display()))?;
        if seen.insert(value.to_owned()) {
            ordered_ids.push(value.to_owned());
        }
    }

    // Include markdown files omitted from order.txt so filesystem data is never hidden.
    let mut extras = Vec::new();
    for entry in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("md") {
            if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
                if seen.insert(stem.to_owned()) {
                    extras.push(stem.to_owned());
                }
            }
        }
    }
    extras.sort();
    ordered_ids.extend(extras);

    let mut cards = Vec::new();
    for card_id in ordered_ids {
        let path = dir.join(format!("{card_id}.md"));
        if !path.is_file() {
            continue;
        }
        cards.push(load_card(&path, card_id)?);
    }
    Ok(Column { id, name, cards })
}

fn load_card(path: &Path, id: String) -> Result<Card> {
    let contents = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let normalized = contents.replace("\r\n", "\n");
    let (first, body) = normalized
        .split_once('\n')
        .map_or((normalized.as_str(), ""), |(first, rest)| (first, rest));
    let title = first
        .strip_prefix("# ")
        .ok_or_else(|| anyhow!("{} line 1 must start with '# '", path.display()))?
        .to_owned();
    Ok(Card {
        id,
        title,
        body: body.strip_suffix('\n').unwrap_or(body).to_owned(),
    })
}

fn render_card(title: &str, body: &str) -> String {
    if body.is_empty() {
        format!("# {title}\n")
    } else {
        format!("# {title}\n{body}\n")
    }
}

fn parse_column_line(line: &str) -> Result<(String, String)> {
    let rest = line
        .strip_prefix("col ")
        .ok_or_else(|| anyhow!("expected: col <column_id> \"<display_name>\""))?;
    let (id, quoted) = rest
        .split_once(char::is_whitespace)
        .ok_or_else(|| anyhow!("missing quoted display name"))?;
    validate_id(id, "column")?;
    let quoted = quoted.trim();
    if !quoted.starts_with('"') || !quoted.ends_with('"') || quoted.len() < 2 {
        bail!("display name must be enclosed in double quotes");
    }
    let inner = &quoted[1..quoted.len() - 1];
    let name = unescape_name(inner)?;
    validate_text(&name, "column display name")?;
    Ok((id.to_owned(), name))
}

fn escape_name(name: &str) -> String {
    name.replace('\\', "\\\\").replace('"', "\\\"")
}

fn unescape_name(value: &str) -> Result<String> {
    let mut output = String::new();
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            match ch {
                '\\' | '"' => output.push(ch),
                _ => bail!("unsupported escape sequence \\{ch}"),
            }
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            output.push(ch);
        }
    }
    if escaped {
        bail!("unfinished escape sequence");
    }
    Ok(output)
}

fn validate_id(id: &str, kind: &str) -> Result<()> {
    if id.is_empty() {
        bail!("{kind} id cannot be empty");
    }
    if id == "."
        || id == ".."
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        bail!("{kind} id may contain only ASCII letters, digits, '.', '_' and '-'");
    }
    Ok(())
}

fn validate_text(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{label} cannot be empty");
    }
    if value.contains(['\n', '\r']) {
        bail!("{label} cannot contain a newline");
    }
    Ok(())
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".{}.toolb-tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file"),
        std::process::id()
    ));
    let result = (|| -> Result<()> {
        let mut file = fs::File::create(&temp)
            .with_context(|| format!("create temporary file {}", temp.display()))?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temp, path).with_context(|| format!("replace {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn complete_card_lifecycle_updates_real_files() {
        let temp = tempdir().unwrap();
        let mut board = Board::open(temp.path()).unwrap();
        board.create_column("todo", "TO DO").unwrap();
        board.create_column("done", "DONE").unwrap();
        board.create_card(0, "item-1", "First title").unwrap();
        board.append_body_line(0, 0, "line one").unwrap();
        board.append_body_line(0, 0, "line two").unwrap();
        board.edit_title(0, 0, "Changed").unwrap();
        board.move_card(0, 0, 1).unwrap();

        assert!(!temp.path().join("cols/todo/item-1.md").exists());
        assert_eq!(
            fs::read_to_string(temp.path().join("cols/done/item-1.md")).unwrap(),
            "# Changed\nline one\nline two\n"
        );
        assert_eq!(
            fs::read_to_string(temp.path().join("cols/done/order.txt")).unwrap(),
            "item-1\n"
        );

        board.delete_card(1, 0).unwrap();
        assert!(!temp.path().join("cols/done/item-1.md").exists());
        assert_eq!(
            fs::read_to_string(temp.path().join("cols/done/order.txt")).unwrap(),
            ""
        );
    }

    #[test]
    fn loads_orphan_markdown_cards_after_ordered_cards() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("cols/todo")).unwrap();
        fs::write(temp.path().join("board.txt"), "col todo \"TO DO\"\n").unwrap();
        fs::write(temp.path().join("cols/todo/order.txt"), "b\n").unwrap();
        fs::write(temp.path().join("cols/todo/a.md"), "# A\nbody\n").unwrap();
        fs::write(temp.path().join("cols/todo/b.md"), "# B\n").unwrap();
        let board = Board::load(temp.path()).unwrap();
        assert_eq!(board.columns[0].cards[0].id, "b");
        assert_eq!(board.columns[0].cards[1].id, "a");
    }

    #[test]
    fn quoted_column_names_round_trip() {
        let temp = tempdir().unwrap();
        let mut board = Board::open(temp.path()).unwrap();
        board.create_column("review", "Review \\\"QA\\\"").unwrap();
        let loaded = Board::load(temp.path()).unwrap();
        assert_eq!(loaded.columns[0].name, "Review \\\"QA\\\"");
    }
}
