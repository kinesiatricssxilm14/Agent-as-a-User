//! All filesystem access lives here.
//!
//! Board layout, exactly as specified:
//!
//! ```text
//! <root>/board.txt                    col <column_id> "<display name>"
//! <root>/cols/<column_id>/order.txt   one card id per line, no extension
//! <root>/cols/<column_id>/<id>.md     line 1 is "# <title>", the rest is the body
//! ```
//!
//! Three invariants drive the design:
//!
//! 1. **Loading never writes.** A board can be inspected without perturbing it, so a
//!    reconciled-but-unsaved order is a display concern only.
//! 2. **Rewrites preserve everything untouched.** `board.txt` keeps its comments, blank lines
//!    and original quoting; only the line being changed is re-rendered.
//! 3. **Every write is atomic and self-healing.** Writes go to a temp file in the same
//!    directory and are renamed into place, and multi-step operations are ordered so that an
//!    interruption leaves a state that load-time reconciliation repairs.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::model::{Board, Card, Column};
use crate::slug;

/// Result alias: errors are already-formatted, user-facing strings.
pub type Result<T> = std::result::Result<T, String>;

/// Prefix for atomic-write temp files. Skipped when scanning a column directory so an
/// interrupted write can never appear as a card.
const TEMP_PREFIX: &str = ".toolb-tmp-";

/// Counter making temp file names unique within a process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Which line terminator a file uses, so rewrites do not convert it.
///
/// Mixing LF into a CRLF file is a visible corruption in tools like `git diff`, and a
/// byte-comparing test would catch it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    /// Detect the dominant terminator. CRLF only if it is the majority style.
    pub fn detect(text: &str) -> Self {
        let crlf = text.matches("\r\n").count();
        let lf = text.matches('\n').count();
        // `lf` counts the newline in every CRLF too, so bare LFs are the difference.
        if crlf > 0 && crlf * 2 >= lf {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    }

    /// Join lines, terminating every line (including the last).
    /// An empty list yields an empty file rather than a lone newline.
    pub fn join(self, lines: &[String]) -> String {
        let mut out = String::new();
        for line in lines {
            out.push_str(line);
            out.push_str(self.as_str());
        }
        out
    }
}

/// Write `bytes` to `path` atomically: temp file in the same directory, flushed and synced,
/// then renamed over the target.
///
/// Same-directory placement keeps the rename on one filesystem, where it is atomic. The temp
/// file is removed on every error path so a failure cannot leave debris behind.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = dir.join(format!("{TEMP_PREFIX}{}-{counter}", std::process::id()));

    let write_result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.flush()?;
        // Durability before the rename: otherwise a crash can expose an empty file.
        file.sync_all()?;
        Ok(())
    })();

    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("cannot write {}: {e}", path.display()));
    }

    if let Err(e) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("cannot replace {}: {e}", path.display()));
    }
    Ok(())
}

/// Read a file as text, tolerating invalid UTF-8 rather than refusing to show the board.
fn read_text(path: &Path) -> Result<Option<String>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Split into lines without terminators, dropping the single trailing empty element produced by
/// a final newline.
fn split_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> =
        text.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

// ---------------------------------------------------------------------------------------------
// board.txt
// ---------------------------------------------------------------------------------------------

/// A parsed `col` declaration together with the index of the raw line it came from.
#[derive(Debug, Clone)]
struct ColumnDecl {
    id: String,
    display_name: String,
    line_index: usize,
}

/// `board.txt` with enough context to rewrite it losslessly.
#[derive(Debug, Clone)]
pub struct BoardFile {
    pub path: PathBuf,
    pub exists: bool,
    /// Every physical line, verbatim, without terminators.
    lines: Vec<String>,
    decls: Vec<ColumnDecl>,
    line_ending: LineEnding,
}

impl BoardFile {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("board.txt");
        let Some(text) = read_text(&path)? else {
            return Ok(Self {
                path,
                exists: false,
                lines: Vec::new(),
                decls: Vec::new(),
                line_ending: LineEnding::Lf,
            });
        };
        let line_ending = LineEnding::detect(&text);
        let lines = split_lines(&text);
        let decls = lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| parse_col_line(line).map(|(id, name)| ColumnDecl {
                id,
                display_name: name,
                line_index: i,
            }))
            .collect();
        Ok(Self { path, exists: true, lines, decls, line_ending })
    }

    /// Declared columns in file order, with duplicate ids removed (first wins).
    fn columns(&self, warnings: &mut Vec<String>) -> Vec<(String, String)> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for decl in &self.decls {
            if !seen.insert(decl.id.clone()) {
                warnings.push(format!(
                    "board.txt declares column id '{}' more than once; using the first",
                    decl.id
                ));
                continue;
            }
            out.push((decl.id.clone(), decl.display_name.clone()));
        }
        let mut names: Vec<&String> = out.iter().map(|(_, n)| n).collect();
        names.sort();
        for pair in names.windows(2) {
            if pair[0] == pair[1] {
                warnings.push(format!(
                    "two columns share the display name '{}'; showing ids to tell them apart",
                    pair[0]
                ));
            }
        }
        out
    }

    fn save(&self) -> Result<()> {
        atomic_write(&self.path, self.line_ending.join(&self.lines).as_bytes())
    }

    /// Append a new `col` declaration.
    fn append_column(&mut self, id: &str, display_name: &str) -> Result<()> {
        let rendered = render_col_line(id, display_name);
        // Keep the file tidy if it ended in blank lines: reuse one instead of stacking more.
        if let Some(pos) = self.lines.iter().rposition(|l| !l.trim().is_empty()) {
            self.lines.truncate(pos + 1);
        } else if !self.lines.is_empty() {
            self.lines.clear();
        }
        self.lines.push(rendered);
        self.decls.push(ColumnDecl {
            id: id.to_string(),
            display_name: display_name.to_string(),
            line_index: self.lines.len() - 1,
        });
        self.exists = true;
        self.save()
    }

    /// Change a column's display name, keeping its id and its position in the file.
    fn rename_column(&mut self, id: &str, display_name: &str) -> Result<()> {
        let decl = self
            .decls
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| format!("column '{id}' is not declared in board.txt"))?;
        self.lines[decl.line_index] = render_col_line(id, display_name);
        decl.display_name = display_name.to_string();
        self.save()
    }
}

/// Parse a `col <id> "<display name>"` line.
///
/// Tolerant by design: blank lines and `#` comments are skipped, the display name may be
/// unquoted (taken as the rest of the line), and `\"` / `\\` are unescaped inside quotes. A
/// missing display name falls back to the id so a half-written line still yields a usable column.
fn parse_col_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let rest = trimmed.strip_prefix("col")?;
    // Require whitespace after the keyword so `column_x ...` is not mistaken for `col`.
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let (id, rest) = match rest.find(char::is_whitespace) {
        Some(pos) => (&rest[..pos], rest[pos..].trim_start()),
        None => (rest, ""),
    };
    if id.is_empty() || slug::validate_id(id).is_err() {
        return None;
    }
    let display_name = if rest.is_empty() {
        id.to_string()
    } else if rest.starts_with('"') {
        unquote_display_name(rest)
    } else {
        rest.trim_end().to_string()
    };
    Some((id.to_string(), display_name))
}

/// Read a `"`-delimited display name, honouring `\"` and `\\`.
fn unquote_display_name(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().skip(1);
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.next() {
                Some(next @ ('"' | '\\')) => out.push(next),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            },
            '"' => return out,
            _ => out.push(ch),
        }
    }
    // Unterminated quote: take what we have rather than dropping the column.
    out
}

/// Render a `col` line in the canonical quoted form.
fn render_col_line(id: &str, display_name: &str) -> String {
    let escaped = display_name.replace('\\', "\\\\").replace('"', "\\\"");
    format!("col {id} \"{escaped}\"")
}

// ---------------------------------------------------------------------------------------------
// order.txt
// ---------------------------------------------------------------------------------------------

/// Parse `order.txt` into card ids.
///
/// Tolerant: trims whitespace, skips blanks and `#` comments, strips a trailing `.md` (the file
/// is specified to hold bare ids, but hand-edited files often include the extension), drops
/// entries containing path separators, and de-duplicates with first-occurrence-wins.
fn parse_order(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for line in text.split('\n') {
        let entry = line.trim_end_matches('\r').trim();
        if entry.is_empty() || entry.starts_with('#') {
            continue;
        }
        let id = slug::normalize_id(entry);
        if slug::validate_id(&id).is_err() {
            continue;
        }
        if seen.insert(id.clone()) {
            out.push(id);
        }
    }
    out
}

/// Reconcile a declared order against what is actually on disk.
///
/// Ids listed in `order.txt` keep their relative order; ids with no file are dropped ("phantoms");
/// files not listed are appended in byte-wise name order ("orphans") so the result is
/// deterministic regardless of directory iteration order.
///
/// This is what makes the interrupted-write states below self-healing, and it is a pure function
/// so it can be tested without touching a filesystem.
fn reconcile_order(declared: &[String], on_disk: &[String]) -> Vec<String> {
    let present: HashSet<&String> = on_disk.iter().collect();
    let mut out: Vec<String> = declared.iter().filter(|id| present.contains(id)).cloned().collect();

    let listed: HashSet<&String> = declared.iter().collect();
    let mut orphans: Vec<String> =
        on_disk.iter().filter(|id| !listed.contains(id)).cloned().collect();
    orphans.sort();
    out.extend(orphans);
    out
}

/// Card ids present in a column directory, from the `.md` files themselves.
///
/// Skips subdirectories, non-`.md` files, dotfiles (including our own temp files) and anything
/// whose stem would not be a valid id.
fn scan_card_files(dir: &Path) -> Result<Vec<String>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", dir.display())),
    };
    let mut ids = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Some(stem) = name.strip_suffix(".md") else { continue };
        if slug::validate_id(stem).is_ok() {
            ids.push(stem.to_string());
        }
    }
    Ok(ids)
}

/// Write `order.txt` for a column, creating the directory if needed.
fn write_order(root: &Path, column_id: &str, ids: &[String], ending: LineEnding) -> Result<()> {
    let path = root.join("cols").join(column_id).join("order.txt");
    atomic_write(&path, ending.join(ids).as_bytes())
}

/// Line ending currently used by a column's `order.txt`, so rewrites preserve it.
fn order_line_ending(root: &Path, column_id: &str) -> LineEnding {
    let path = root.join("cols").join(column_id).join("order.txt");
    read_text(&path)
        .ok()
        .flatten()
        .map(|t| LineEnding::detect(&t))
        .unwrap_or(LineEnding::Lf)
}

// ---------------------------------------------------------------------------------------------
// card files
// ---------------------------------------------------------------------------------------------

/// Split card file text into a title and a body.
///
/// Line 1 is the title when it starts with `#`: all leading `#` are stripped, then at most one
/// space, so `# T`, `## T` and `#T` all parse. A file with no header keeps its full text as the
/// body and is flagged, so writing a title later prepends a header instead of destroying content.
fn parse_card_text(text: &str) -> (String, String, bool) {
    let normalized = text.replace("\r\n", "\n");
    let (first, rest) = match normalized.split_once('\n') {
        Some((f, r)) => (f, Some(r)),
        None => (normalized.as_str(), None),
    };

    if first.starts_with('#') {
        let title = first.trim_start_matches('#');
        let title = title.strip_prefix(' ').unwrap_or(title);
        let body = rest.unwrap_or("");
        // A single trailing newline is the terminator of the last body line, not an extra
        // blank line; anything beyond that is real content and is kept.
        let body = body.strip_suffix('\n').unwrap_or(body);
        (title.to_string(), body.to_string(), false)
    } else {
        let body = normalized.strip_suffix('\n').unwrap_or(&normalized);
        (String::new(), body.to_string(), true)
    }
}

/// Load one card.
fn load_card(root: &Path, column_id: &str, card_id: &str) -> Result<Card> {
    let path = root.join("cols").join(column_id).join(format!("{card_id}.md"));
    let text = read_text(&path)?.unwrap_or_default();
    let (title, body, missing_header) = parse_card_text(&text);
    Ok(Card { id: card_id.to_string(), title, body, missing_header })
}

/// Render card text from a title and body, always terminating the final line.
fn render_card(title: &str, body: &str) -> String {
    let mut out = String::with_capacity(title.len() + body.len() + 4);
    out.push_str("# ");
    out.push_str(title);
    out.push('\n');
    if !body.is_empty() {
        out.push_str(body);
        if !body.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// public operations
// ---------------------------------------------------------------------------------------------

/// Default columns created by `--init` and by the in-application initialise action.
///
/// A starting point only: nothing downstream assumes these ids, these names, or that there are
/// three of them.
pub const DEFAULT_COLUMNS: [(&str, &str); 3] =
    [("todo", "TO DO"), ("doing", "DOING"), ("done", "DONE")];

/// The store: every board mutation goes through here.
#[derive(Debug, Clone)]
pub struct Store {
    pub root: PathBuf,
}

impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn board_file_path(&self) -> PathBuf {
        self.root.join("board.txt")
    }

    /// True when the board root holds a readable `board.txt`.
    pub fn is_initialised(&self) -> bool {
        self.board_file_path().is_file()
    }

    /// Read the whole board.
    ///
    /// Never writes: a reconciled order stays in memory until some mutation persists it.
    /// Problems that do not prevent display (duplicate ids, unreadable cards, a missing column
    /// directory) are collected as warnings.
    pub fn load(&self) -> Result<Board> {
        if self.root.exists() && !self.root.is_dir() {
            return Err(format!("{} exists but is not a directory", self.root.display()));
        }

        let mut warnings = Vec::new();
        let board_file = BoardFile::load(&self.root)?;
        if !board_file.exists {
            return Ok(Board { root: self.root.clone(), columns: Vec::new(), warnings });
        }

        let declared_columns = board_file.columns(&mut warnings);
        let mut columns = Vec::with_capacity(declared_columns.len());

        for (id, display_name) in declared_columns {
            let dir = self.root.join("cols").join(&id);
            if !dir.exists() {
                warnings.push(format!(
                    "column '{display_name}' has no directory yet (cols/{id}/ is missing)"
                ));
            }
            let declared = match read_text(&dir.join("order.txt"))? {
                Some(text) => parse_order(&text),
                None => Vec::new(),
            };
            let on_disk = scan_card_files(&dir)?;
            let ordered = reconcile_order(&declared, &on_disk);

            let mut cards = Vec::with_capacity(ordered.len());
            for card_id in ordered {
                match load_card(&self.root, &id, &card_id) {
                    Ok(card) => cards.push(card),
                    Err(e) => warnings.push(e),
                }
            }
            columns.push(Column { id, display_name, cards });
        }

        Ok(Board { root: self.root.clone(), columns, warnings })
    }

    /// Create the board root, `cols/`, and `board.txt` if absent.
    ///
    /// Called at the start of every mutation so operations succeed against an empty or missing
    /// root instead of failing. Existing files are left untouched.
    fn ensure_board(&self) -> Result<()> {
        std::fs::create_dir_all(self.root.join("cols"))
            .map_err(|e| format!("cannot create {}: {e}", self.root.join("cols").display()))?;
        let board_path = self.board_file_path();
        if !board_path.exists() {
            atomic_write(&board_path, b"")?;
        }
        Ok(())
    }

    /// Create the board with the default column set. Safe to re-run: existing columns are kept.
    pub fn init_board(&self) -> Result<()> {
        self.ensure_board()?;
        let existing = BoardFile::load(&self.root)?;
        let mut have: HashSet<String> =
            existing.decls.iter().map(|d| d.id.clone()).collect();
        for (id, name) in DEFAULT_COLUMNS {
            if have.contains(id) {
                continue;
            }
            self.create_column(id, name)?;
            have.insert(id.to_string());
        }
        Ok(())
    }

    /// Create a column: validate, declare it in `board.txt`, and make its directory.
    ///
    /// Rejects an id that already exists and a display name already in use, since the UI
    /// addresses columns by display name.
    pub fn create_column(&self, id: &str, display_name: &str) -> Result<()> {
        let id = slug::normalize_id(id);
        slug::validate_id(&id).map_err(|e| format!("invalid column id: {e}"))?;
        let display_name = display_name.trim();
        if display_name.is_empty() {
            return Err("column display name must not be empty".to_string());
        }
        if display_name.contains(['\n', '\r']) {
            return Err("column display name must be a single line".to_string());
        }

        self.ensure_board()?;
        let mut board_file = BoardFile::load(&self.root)?;
        if board_file.decls.iter().any(|d| d.id == id) {
            return Err(format!("column id '{id}' already exists"));
        }
        if board_file.decls.iter().any(|d| d.display_name == display_name) {
            return Err(format!("a column named '{display_name}' already exists"));
        }

        let dir = self.root.join("cols").join(&id);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        // Declare last: a crash before this leaves an undeclared directory, which is inert,
        // rather than a declaration pointing at nothing.
        board_file.append_column(&id, display_name)
    }

    /// Rename a column's display name. The id, and therefore every path, is unchanged.
    pub fn rename_column(&self, id: &str, display_name: &str) -> Result<()> {
        let display_name = display_name.trim();
        if display_name.is_empty() {
            return Err("column display name must not be empty".to_string());
        }
        if display_name.contains(['\n', '\r']) {
            return Err("column display name must be a single line".to_string());
        }
        let mut board_file = BoardFile::load(&self.root)?;
        if board_file
            .decls
            .iter()
            .any(|d| d.display_name == display_name && d.id != id)
        {
            return Err(format!("a column named '{display_name}' already exists"));
        }
        board_file.rename_column(id, display_name)
    }

    /// Ids already used in a column, from both `order.txt` and the directory listing, so a new
    /// card cannot collide with a file that the order file forgot to mention.
    fn used_card_ids(&self, column_id: &str) -> HashSet<String> {
        let dir = self.root.join("cols").join(column_id);
        let mut ids: HashSet<String> = scan_card_files(&dir).unwrap_or_default().into_iter().collect();
        if let Ok(Some(text)) = read_text(&dir.join("order.txt")) {
            ids.extend(parse_order(&text));
        }
        ids
    }

    /// Suggest an unused card id for a title. Used to prefill the id prompt.
    pub fn suggest_card_id(&self, column_id: &str, title: &str) -> String {
        let used = self.used_card_ids(column_id);
        slug::suggest_card_id(title, |id| used.contains(id))
    }

    /// Create a card with `# <title>` as its first line and register it in `order.txt`.
    ///
    /// Order: write the card file, then the order file. An interruption between the two leaves an
    /// orphan file, which reconciliation appends on the next load — no data is lost.
    pub fn create_card(&self, column_id: &str, card_id: &str, title: &str) -> Result<String> {
        let card_id = slug::normalize_id(card_id);
        slug::validate_id(&card_id).map_err(|e| format!("invalid card id: {e}"))?;
        if title.contains(['\n', '\r']) {
            return Err("card title must be a single line".to_string());
        }
        self.ensure_column_exists(column_id)?;

        let dir = self.root.join("cols").join(column_id);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let path = dir.join(format!("{card_id}.md"));
        if path.exists() {
            return Err(format!("card '{card_id}' already exists in this column"));
        }

        atomic_write(&path, render_card(title, "").as_bytes())?;

        let mut ids = self.current_order(column_id)?;
        if !ids.iter().any(|i| i == &card_id) {
            ids.push(card_id.clone());
        }
        write_order(&self.root, column_id, &ids, order_line_ending(&self.root, column_id))?;
        Ok(card_id)
    }

    /// Replace line 1, keeping the `# ` prefix and the body untouched.
    ///
    /// A file with no header gains one; its existing first line stays part of the body.
    pub fn set_card_title(&self, column_id: &str, card_id: &str, title: &str) -> Result<()> {
        if title.contains(['\n', '\r']) {
            return Err("card title must be a single line".to_string());
        }
        let path = self.card_path(column_id, card_id);
        let text = read_text(&path)?
            .ok_or_else(|| format!("card file {} no longer exists", path.display()))?;
        let ending = LineEnding::detect(&text);
        let (_, body, _) = parse_card_text(&text);

        let mut rendered = render_card(title, &body);
        if ending == LineEnding::CrLf {
            rendered = rendered.replace('\n', "\r\n");
        }
        atomic_write(&path, rendered.as_bytes())
    }

    /// Append a line to the end of a card file.
    ///
    /// Adds a terminator first if the file lacks one, then the new line. Existing blank lines are
    /// preserved exactly: normalising trailing whitespace here would silently delete content the
    /// user (or a fixture) put there deliberately.
    pub fn append_card_line(&self, column_id: &str, card_id: &str, line: &str) -> Result<()> {
        let path = self.card_path(column_id, card_id);
        let existing = read_text(&path)?
            .ok_or_else(|| format!("card file {} no longer exists", path.display()))?;
        let ending = LineEnding::detect(&existing);

        let mut out = existing;
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(ending.as_str());
        }
        // Multi-line input is appended as multiple lines, each properly terminated.
        for (i, part) in line.split('\n').enumerate() {
            if i > 0 {
                out.push_str(ending.as_str());
            }
            out.push_str(part.trim_end_matches('\r'));
        }
        out.push_str(ending.as_str());
        atomic_write(&path, out.as_bytes())
    }

    /// Replace a card's entire body, keeping its title.
    pub fn set_card_body(&self, column_id: &str, card_id: &str, body: &str) -> Result<()> {
        let path = self.card_path(column_id, card_id);
        let text = read_text(&path)?
            .ok_or_else(|| format!("card file {} no longer exists", path.display()))?;
        let ending = LineEnding::detect(&text);
        let (title, _, _) = parse_card_text(&text);

        let normalized_body = body.replace("\r\n", "\n");
        let mut rendered = render_card(&title, &normalized_body);
        if ending == LineEnding::CrLf {
            rendered = rendered.replace('\n', "\r\n");
        }
        atomic_write(&path, rendered.as_bytes())
    }

    /// Delete a card file and drop it from `order.txt`.
    ///
    /// Order: unlink, then rewrite. An interruption leaves a phantom id, which reconciliation
    /// drops on the next load.
    pub fn delete_card(&self, column_id: &str, card_id: &str) -> Result<()> {
        let path = self.card_path(column_id, card_id);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            // Already gone: fall through and still clean up the order file.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("cannot delete {}: {e}", path.display())),
        }
        let ids: Vec<String> =
            self.current_order(column_id)?.into_iter().filter(|i| i != card_id).collect();
        write_order(&self.root, column_id, &ids, order_line_ending(&self.root, column_id))
    }

    /// Move a card to another column, inserting at `target_index` (clamped; `None` appends).
    ///
    /// Order: rename the file, append to the target order, then rewrite the source order. An
    /// interruption leaves an orphan in the target (appended on load) and a phantom in the source
    /// (dropped on load) — the card is never lost or duplicated.
    ///
    /// A colliding id in the target is refused rather than silently overwritten; the caller can
    /// retry with `new_card_id` set.
    pub fn move_card(
        &self,
        from_column: &str,
        card_id: &str,
        to_column: &str,
        target_index: Option<usize>,
        new_card_id: Option<&str>,
    ) -> Result<String> {
        if from_column == to_column && new_card_id.is_none() {
            return Err("card is already in that column".to_string());
        }
        self.ensure_column_exists(to_column)?;

        let source = self.card_path(from_column, card_id);
        if !source.exists() {
            return Err(format!("card file {} no longer exists", source.display()));
        }

        let dest_id = match new_card_id {
            Some(id) => {
                let id = slug::normalize_id(id);
                slug::validate_id(&id).map_err(|e| format!("invalid card id: {e}"))?;
                id
            }
            None => card_id.to_string(),
        };

        let dest_dir = self.root.join("cols").join(to_column);
        std::fs::create_dir_all(&dest_dir)
            .map_err(|e| format!("cannot create {}: {e}", dest_dir.display()))?;
        let dest = dest_dir.join(format!("{dest_id}.md"));
        if dest.exists() && dest != source {
            let suggestion = self.suggest_card_id(to_column, &dest_id);
            return Err(format!(
                "'{dest_id}' already exists in the target column - retry with a different id (e.g. '{suggestion}')"
            ));
        }

        rename_or_copy(&source, &dest)?;

        // Target order first, so an interruption yields a recoverable orphan.
        let mut target_ids: Vec<String> =
            self.current_order(to_column)?.into_iter().filter(|i| i != &dest_id).collect();
        let index = target_index.unwrap_or(target_ids.len()).min(target_ids.len());
        target_ids.insert(index, dest_id.clone());
        write_order(&self.root, to_column, &target_ids, order_line_ending(&self.root, to_column))?;

        let source_ids: Vec<String> =
            self.current_order(from_column)?.into_iter().filter(|i| i != card_id).collect();
        write_order(
            &self.root,
            from_column,
            &source_ids,
            order_line_ending(&self.root, from_column),
        )?;

        Ok(dest_id)
    }

    /// Move a card up or down inside its column by rewriting `order.txt`.
    pub fn reorder_card(&self, column_id: &str, card_id: &str, delta: isize) -> Result<()> {
        let mut ids = self.current_order(column_id)?;
        let Some(pos) = ids.iter().position(|i| i == card_id) else {
            return Err(format!("card '{card_id}' is not in this column"));
        };
        let target = pos as isize + delta;
        if target < 0 || target >= ids.len() as isize {
            return Err("card is already at the end of the column".to_string());
        }
        ids.swap(pos, target as usize);
        write_order(&self.root, column_id, &ids, order_line_ending(&self.root, column_id))
    }

    /// Persist the reconciled order for a column, so `order.txt` matches what was displayed.
    pub fn normalise_order(&self, column_id: &str) -> Result<()> {
        let ids = self.current_order(column_id)?;
        write_order(&self.root, column_id, &ids, order_line_ending(&self.root, column_id))
    }

    fn card_path(&self, column_id: &str, card_id: &str) -> PathBuf {
        self.root.join("cols").join(column_id).join(format!("{card_id}.md"))
    }

    /// The reconciled order for a column: what the UI shows and what a rewrite should persist.
    fn current_order(&self, column_id: &str) -> Result<Vec<String>> {
        let dir = self.root.join("cols").join(column_id);
        let declared = match read_text(&dir.join("order.txt"))? {
            Some(text) => parse_order(&text),
            None => Vec::new(),
        };
        let on_disk = scan_card_files(&dir)?;
        Ok(reconcile_order(&declared, &on_disk))
    }

    fn ensure_column_exists(&self, column_id: &str) -> Result<()> {
        let board_file = BoardFile::load(&self.root)?;
        if !board_file.decls.iter().any(|d| d.id == column_id) {
            return Err(format!("column '{column_id}' is not declared in board.txt"));
        }
        Ok(())
    }
}

/// Rename, falling back to copy+remove when the two paths are on different filesystems.
///
/// A board root spanning a mount point (a bind-mounted column directory, say) would otherwise
/// make every move fail with `EXDEV`.
fn rename_or_copy(source: &Path, dest: &Path) -> Result<()> {
    match std::fs::rename(source, dest) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(source, dest)
                .map_err(|e| format!("cannot copy {} to {}: {e}", source.display(), dest.display()))?;
            std::fs::remove_file(source)
                .map_err(|e| format!("copied, but cannot remove {}: {e}", source.display()))?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        (dir, store)
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    // --- line endings -------------------------------------------------------------------

    #[test]
    fn detects_line_endings() {
        assert_eq!(LineEnding::detect("a\nb\n"), LineEnding::Lf);
        assert_eq!(LineEnding::detect("a\r\nb\r\n"), LineEnding::CrLf);
        assert_eq!(LineEnding::detect(""), LineEnding::Lf);
        assert_eq!(LineEnding::detect("no newline"), LineEnding::Lf);
        // Mostly CRLF with one stray LF still counts as CRLF.
        assert_eq!(LineEnding::detect("a\r\nb\r\nc\nd\r\n"), LineEnding::CrLf);
        assert_eq!(LineEnding::detect("a\nb\nc\r\n"), LineEnding::Lf);
    }

    #[test]
    fn join_terminates_every_line_and_keeps_empty_empty() {
        assert_eq!(LineEnding::Lf.join(&[]), "");
        assert_eq!(LineEnding::Lf.join(&["a".into(), "b".into()]), "a\nb\n");
        assert_eq!(LineEnding::CrLf.join(&["a".into()]), "a\r\n");
    }

    // --- board.txt parsing --------------------------------------------------------------

    #[test]
    fn parses_the_specified_col_syntax() {
        assert_eq!(parse_col_line(r#"col todo "TO DO""#), Some(("todo".into(), "TO DO".into())));
        assert_eq!(parse_col_line("  col  todo   \"TO DO\"  "), Some(("todo".into(), "TO DO".into())));
        // Unquoted display name: rest of line.
        assert_eq!(parse_col_line("col todo TO DO"), Some(("todo".into(), "TO DO".into())));
        // Missing display name falls back to the id.
        assert_eq!(parse_col_line("col todo"), Some(("todo".into(), "todo".into())));
    }

    #[test]
    fn parses_escapes_and_unicode_in_display_names() {
        assert_eq!(
            parse_col_line(r#"col a "say \"hi\"""#),
            Some(("a".into(), r#"say "hi""#.into()))
        );
        assert_eq!(parse_col_line(r#"col a "b\\c""#), Some(("a".into(), r"b\c".into())));
        assert_eq!(parse_col_line(r#"col wip "English-only text""#), Some(("wip".into(), "English-only text".into())));
        // Unterminated quote still yields a usable column.
        assert_eq!(parse_col_line(r#"col a "oops"#), Some(("a".into(), "oops".into())));
    }

    #[test]
    fn ignores_non_column_lines() {
        assert_eq!(parse_col_line(""), None);
        assert_eq!(parse_col_line("   "), None);
        assert_eq!(parse_col_line("# a comment"), None);
        assert_eq!(parse_col_line("# col todo \"X\""), None);
        assert_eq!(parse_col_line("column todo \"X\""), None, "'col' must be its own token");
        assert_eq!(parse_col_line("colly todo"), None);
        assert_eq!(parse_col_line("col"), None, "no id");
        assert_eq!(parse_col_line("col ../escape \"X\""), None, "unsafe id rejected");
        assert_eq!(parse_col_line("col a/b \"X\""), None);
    }

    #[test]
    fn renders_canonical_quoted_form() {
        assert_eq!(render_col_line("todo", "TO DO"), r#"col todo "TO DO""#);
        assert_eq!(render_col_line("a", r#"q"x"#), r#"col a "q\"x""#);
        assert_eq!(render_col_line("a", r"b\c"), r#"col a "b\\c""#);
    }

    #[test]
    fn col_line_round_trips_through_parse_and_render() {
        for name in ["TO DO", r#"say "hi""#, r"back\slash", "English-only text", "with  spaces"] {
            let rendered = render_col_line("id", name);
            assert_eq!(parse_col_line(&rendered), Some(("id".into(), name.to_string())), "{name}");
        }
    }

    // --- order.txt ----------------------------------------------------------------------

    #[test]
    fn order_parsing_is_tolerant() {
        let ids = parse_order("item-1\n\n  item-2  \n# comment\nitem-3.md\nitem-1\n../evil\na/b\n");
        assert_eq!(ids, vec!["item-1", "item-2", "item-3"], "dedup, trim, strip .md, drop unsafe");
    }

    #[test]
    fn order_parsing_handles_crlf_and_no_trailing_newline() {
        assert_eq!(parse_order("a\r\nb\r\n"), vec!["a", "b"]);
        assert_eq!(parse_order("a\nb"), vec!["a", "b"]);
        assert_eq!(parse_order(""), Vec::<String>::new());
    }

    #[test]
    fn reconcile_keeps_declared_order_drops_phantoms_appends_orphans() {
        let declared = vec!["b".to_string(), "gone".to_string(), "a".to_string()];
        let on_disk = vec!["a".to_string(), "b".to_string(), "z".to_string(), "c".to_string()];
        // Declared order (b, a) preserved; "gone" dropped; orphans c,z appended sorted.
        assert_eq!(reconcile_order(&declared, &on_disk), vec!["b", "a", "c", "z"]);
    }

    #[test]
    fn reconcile_is_deterministic_regardless_of_scan_order() {
        let declared: Vec<String> = vec![];
        let mut a = vec!["c".to_string(), "a".to_string(), "b".to_string()];
        let expected = reconcile_order(&declared, &a);
        a.reverse();
        assert_eq!(reconcile_order(&declared, &a), expected);
        assert_eq!(expected, vec!["a", "b", "c"]);
    }

    // --- card text ----------------------------------------------------------------------

    #[test]
    fn parses_title_and_body_variants() {
        assert_eq!(parse_card_text("# T\nbody\n"), ("T".into(), "body".into(), false));
        assert_eq!(parse_card_text("## T\n"), ("T".into(), String::new(), false));
        assert_eq!(parse_card_text("#T\n"), ("T".into(), String::new(), false));
        assert_eq!(parse_card_text("# T"), ("T".into(), String::new(), false));
        assert_eq!(parse_card_text("#\n"), (String::new(), String::new(), false));
        // Multi-line body, including interior blank lines.
        assert_eq!(parse_card_text("# T\na\n\nb\n"), ("T".into(), "a\n\nb".into(), false));
        // CRLF normalised for display.
        assert_eq!(parse_card_text("# T\r\nbody\r\n"), ("T".into(), "body".into(), false));
    }

    #[test]
    fn headerless_files_keep_all_content_as_body() {
        let (title, body, missing) = parse_card_text("just text\nmore\n");
        assert_eq!(title, "");
        assert_eq!(body, "just text\nmore");
        assert!(missing, "flagged so a later title write prepends instead of overwriting");
        assert_eq!(parse_card_text(""), (String::new(), String::new(), true));
    }

    #[test]
    fn body_preserves_deliberate_trailing_blank_lines() {
        // Only the final terminator is consumed; the blank line before it is content.
        let (_, body, _) = parse_card_text("# T\nbody\n\n");
        assert_eq!(body, "body\n");
    }

    // --- create / init ------------------------------------------------------------------

    #[test]
    fn init_creates_the_specified_layout() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        assert_eq!(
            read(&s.board_file_path()),
            "col todo \"TO DO\"\ncol doing \"DOING\"\ncol done \"DONE\"\n"
        );
        for (id, _) in DEFAULT_COLUMNS {
            assert!(s.root.join("cols").join(id).is_dir(), "missing dir for {id}");
        }
        assert!(s.is_initialised());
        assert_eq!(s.load().unwrap().columns.len(), 3);
    }

    #[test]
    fn init_is_idempotent_and_keeps_existing_columns() {
        let (_dir, s) = store();
        s.create_column("custom", "CUSTOM").unwrap();
        s.init_board().unwrap();
        s.init_board().unwrap();
        let board = s.load().unwrap();
        let ids: Vec<&str> = board.columns.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["custom", "todo", "doing", "done"]);
    }

    #[test]
    fn mutations_work_against_a_missing_root() {
        let dir = tempfile::tempdir().unwrap();
        // Root does not exist yet, mirroring a task run against a fresh /bench/data/board.
        let s = Store::new(dir.path().join("fresh").join("board"));
        s.create_column("todo", "TO DO").unwrap();
        s.create_card("todo", "item-1", "First").unwrap();
        assert_eq!(read(&s.board_file_path()), "col todo \"TO DO\"\n");
        assert_eq!(read(&s.root.join("cols/todo/order.txt")), "item-1\n");
    }

    #[test]
    fn create_card_writes_specified_format_and_order() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        s.create_card("todo", "item-1", "Fix login bug").unwrap();
        assert_eq!(read(&s.root.join("cols/todo/item-1.md")), "# Fix login bug\n");
        assert_eq!(read(&s.root.join("cols/todo/order.txt")), "item-1\n");

        s.create_card("todo", "item-2", "Second").unwrap();
        assert_eq!(read(&s.root.join("cols/todo/order.txt")), "item-1\nitem-2\n");
    }

    #[test]
    fn create_card_rejects_duplicates_and_bad_ids() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        s.create_card("todo", "item-1", "T").unwrap();
        assert!(s.create_card("todo", "item-1", "T").unwrap_err().contains("already exists"));
        assert!(s.create_card("todo", "../evil", "T").is_err());
        assert!(s.create_card("todo", "", "T").is_err());
        assert!(s.create_card("nosuch", "x", "T").unwrap_err().contains("not declared"));
        assert!(s.create_card("todo", "x", "line1\nline2").is_err());
    }

    #[test]
    fn column_creation_rejects_duplicate_ids_and_names() {
        let (_dir, s) = store();
        s.create_column("todo", "TO DO").unwrap();
        assert!(s.create_column("todo", "Other").unwrap_err().contains("already exists"));
        assert!(s.create_column("other", "TO DO").unwrap_err().contains("already exists"));
        assert!(s.create_column("../evil", "X").is_err());
        assert!(s.create_column("ok", "").is_err());
    }

    #[test]
    fn arbitrary_display_names_and_ids_are_supported() {
        let (_dir, s) = store();
        // No hard-coded vocabulary: any valid id/name pair round-trips.
        s.create_column("blocked-2", "Blocked \"hard\"").unwrap();
        s.create_column("English-only text", "English-only text").unwrap();
        let board = s.load().unwrap();
        assert_eq!(board.columns[0].display_name, "Blocked \"hard\"");
        assert_eq!(board.columns[1].display_name, "English-only text");
        assert_eq!(board.column_by_display_name("English-only text"), Some(1));
    }

    // --- board.txt preservation ---------------------------------------------------------

    #[test]
    fn board_file_rewrite_preserves_comments_and_untouched_lines() {
        let (_dir, s) = store();
        let original = concat!(
            "# my board\n",
            "\n",
            "col todo \"TO DO\"\n",
            "col odd \"quoted \\\"name\\\"\"\n",
            "# trailing comment\n",
        );
        std::fs::create_dir_all(&s.root).unwrap();
        std::fs::write(s.board_file_path(), original).unwrap();

        s.rename_column("todo", "BACKLOG").unwrap();
        assert_eq!(
            read(&s.board_file_path()),
            concat!(
                "# my board\n",
                "\n",
                "col todo \"BACKLOG\"\n",
                "col odd \"quoted \\\"name\\\"\"\n",
                "# trailing comment\n",
            ),
            "only the renamed line changes"
        );
    }

    #[test]
    fn board_file_preserves_crlf() {
        let (_dir, s) = store();
        std::fs::create_dir_all(&s.root).unwrap();
        std::fs::write(s.board_file_path(), "# c\r\ncol todo \"TO DO\"\r\n").unwrap();
        s.rename_column("todo", "NEXT").unwrap();
        assert_eq!(read(&s.board_file_path()), "# c\r\ncol todo \"NEXT\"\r\n");
    }

    #[test]
    fn appending_a_column_does_not_stack_blank_lines() {
        let (_dir, s) = store();
        std::fs::create_dir_all(&s.root).unwrap();
        std::fs::write(s.board_file_path(), "col a \"A\"\n\n\n").unwrap();
        s.create_column("b", "B").unwrap();
        assert_eq!(read(&s.board_file_path()), "col a \"A\"\ncol b \"B\"\n");
    }

    #[test]
    fn rename_keeps_id_and_paths_stable() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        s.create_card("todo", "item-1", "T").unwrap();
        s.rename_column("todo", "BACKLOG").unwrap();

        let board = s.load().unwrap();
        assert_eq!(board.columns[0].id, "todo");
        assert_eq!(board.columns[0].display_name, "BACKLOG");
        assert!(s.root.join("cols/todo/item-1.md").exists(), "files did not move");
        assert!(s.rename_column("nosuch", "X").is_err());
        assert!(s.rename_column("todo", "DOING").unwrap_err().contains("already exists"));
    }

    // --- title / body edits -------------------------------------------------------------

    #[test]
    fn set_title_keeps_body_and_hash_prefix() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        s.create_card("todo", "c", "Old").unwrap();
        s.append_card_line("todo", "c", "body line").unwrap();
        s.set_card_title("todo", "c", "New title").unwrap();
        assert_eq!(read(&s.root.join("cols/todo/c.md")), "# New title\nbody line\n");
    }

    #[test]
    fn set_title_on_headerless_file_prepends_and_keeps_content() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        let path = s.root.join("cols/todo/raw.md");
        std::fs::write(&path, "important text\nsecond line\n").unwrap();
        s.set_card_title("todo", "raw", "Recovered").unwrap();
        assert_eq!(read(&path), "# Recovered\nimportant text\nsecond line\n");
    }

    #[test]
    fn set_title_preserves_crlf_files() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        let path = s.root.join("cols/todo/c.md");
        std::fs::write(&path, "# Old\r\nbody\r\n").unwrap();
        s.set_card_title("todo", "c", "New").unwrap();
        assert_eq!(read(&path), "# New\r\nbody\r\n");
    }

    #[test]
    fn append_adds_terminator_when_missing_without_touching_blanks() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        let path = s.root.join("cols/todo/c.md");

        // No trailing newline.
        std::fs::write(&path, "# T\nfirst").unwrap();
        s.append_card_line("todo", "c", "second").unwrap();
        assert_eq!(read(&path), "# T\nfirst\nsecond\n");

        // Deliberate blank line must survive.
        std::fs::write(&path, "# T\nfirst\n\n").unwrap();
        s.append_card_line("todo", "c", "second").unwrap();
        assert_eq!(read(&path), "# T\nfirst\n\nsecond\n");
    }

    #[test]
    fn append_to_empty_file_adds_no_leading_blank() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        let path = s.root.join("cols/todo/c.md");
        std::fs::write(&path, "").unwrap();
        s.append_card_line("todo", "c", "only").unwrap();
        assert_eq!(read(&path), "only\n");
    }

    #[test]
    fn append_preserves_crlf_and_splits_multiline_input() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        let path = s.root.join("cols/todo/c.md");
        std::fs::write(&path, "# T\r\nbody\r\n").unwrap();
        s.append_card_line("todo", "c", "x\ny").unwrap();
        assert_eq!(read(&path), "# T\r\nbody\r\nx\r\ny\r\n");
    }

    #[test]
    fn append_to_missing_file_is_an_error() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        assert!(s.append_card_line("todo", "ghost", "x").is_err());
        assert!(s.set_card_title("todo", "ghost", "x").is_err());
    }

    #[test]
    fn set_body_replaces_content_and_keeps_title() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        s.create_card("todo", "c", "Keep me").unwrap();
        s.set_card_body("todo", "c", "line 1\nline 2").unwrap();
        assert_eq!(read(&s.root.join("cols/todo/c.md")), "# Keep me\nline 1\nline 2\n");
        s.set_card_body("todo", "c", "").unwrap();
        assert_eq!(read(&s.root.join("cols/todo/c.md")), "# Keep me\n");
    }

    // --- delete / move / reorder --------------------------------------------------------

    #[test]
    fn delete_removes_file_and_order_entry() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        s.create_card("todo", "a", "A").unwrap();
        s.create_card("todo", "b", "B").unwrap();
        s.delete_card("todo", "a").unwrap();
        assert!(!s.root.join("cols/todo/a.md").exists());
        assert_eq!(read(&s.root.join("cols/todo/order.txt")), "b\n");
    }

    #[test]
    fn delete_of_already_missing_file_still_cleans_order() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        s.create_card("todo", "a", "A").unwrap();
        std::fs::remove_file(s.root.join("cols/todo/a.md")).unwrap();
        s.delete_card("todo", "a").unwrap();
        assert_eq!(read(&s.root.join("cols/todo/order.txt")), "");
    }

    #[test]
    fn move_relocates_file_and_updates_both_orders() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        s.create_card("todo", "a", "A").unwrap();
        s.create_card("todo", "b", "B").unwrap();
        s.create_card("doing", "x", "X").unwrap();

        s.move_card("todo", "a", "doing", None, None).unwrap();
        assert!(!s.root.join("cols/todo/a.md").exists());
        assert_eq!(read(&s.root.join("cols/doing/a.md")), "# A\n");
        assert_eq!(read(&s.root.join("cols/todo/order.txt")), "b\n");
        assert_eq!(read(&s.root.join("cols/doing/order.txt")), "x\na\n");
    }

    #[test]
    fn move_can_target_a_specific_index() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        s.create_card("todo", "a", "A").unwrap();
        s.create_card("doing", "x", "X").unwrap();
        s.create_card("doing", "y", "Y").unwrap();
        s.move_card("todo", "a", "doing", Some(1), None).unwrap();
        assert_eq!(read(&s.root.join("cols/doing/order.txt")), "x\na\ny\n");
        // Out-of-range index clamps rather than failing.
        s.create_card("todo", "b", "B").unwrap();
        s.move_card("todo", "b", "doing", Some(99), None).unwrap();
        assert_eq!(read(&s.root.join("cols/doing/order.txt")), "x\na\ny\nb\n");
    }

    #[test]
    fn move_refuses_collision_and_suggests_an_alternative() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        s.create_card("todo", "dup", "A").unwrap();
        s.create_card("doing", "dup", "B").unwrap();

        let err = s.move_card("todo", "dup", "doing", None, None).unwrap_err();
        assert!(err.contains("already exists"), "got: {err}");
        assert!(err.contains("dup-2"), "should suggest a free id: {err}");
        // Nothing moved.
        assert_eq!(read(&s.root.join("cols/todo/dup.md")), "# A\n");
        assert_eq!(read(&s.root.join("cols/doing/dup.md")), "# B\n");

        // Retrying with the suggested id succeeds.
        let new_id = s.move_card("todo", "dup", "doing", None, Some("dup-2")).unwrap();
        assert_eq!(new_id, "dup-2");
        assert_eq!(read(&s.root.join("cols/doing/dup-2.md")), "# A\n");
        assert_eq!(read(&s.root.join("cols/doing/order.txt")), "dup\ndup-2\n");
    }

    #[test]
    fn move_rejects_same_column_and_undeclared_target() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        s.create_card("todo", "a", "A").unwrap();
        assert!(s.move_card("todo", "a", "todo", None, None).is_err());
        assert!(s.move_card("todo", "a", "nosuch", None, None).is_err());
        assert!(s.move_card("todo", "ghost", "doing", None, None).is_err());
    }

    #[test]
    fn move_preserves_full_card_content() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        s.create_card("todo", "a", "Title").unwrap();
        s.append_card_line("todo", "a", "body 1").unwrap();
        s.append_card_line("todo", "a", "body 2").unwrap();
        let before = read(&s.root.join("cols/todo/a.md"));
        s.move_card("todo", "a", "done", None, None).unwrap();
        assert_eq!(read(&s.root.join("cols/done/a.md")), before);
    }

    #[test]
    fn reorder_swaps_neighbours_and_clamps_at_edges() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        for id in ["a", "b", "c"] {
            s.create_card("todo", id, id).unwrap();
        }
        s.reorder_card("todo", "c", -1).unwrap();
        assert_eq!(read(&s.root.join("cols/todo/order.txt")), "a\nc\nb\n");
        s.reorder_card("todo", "a", 1).unwrap();
        assert_eq!(read(&s.root.join("cols/todo/order.txt")), "c\na\nb\n");

        assert!(s.reorder_card("todo", "c", -1).is_err(), "already first");
        assert!(s.reorder_card("todo", "b", 1).is_err(), "already last");
        assert!(s.reorder_card("todo", "ghost", 1).is_err());
    }

    // --- loading, reconciliation, warnings ----------------------------------------------

    #[test]
    fn load_reconciles_without_writing_anything() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        let dir = s.root.join("cols/todo");
        // Orphan file (not in order.txt) and phantom entry (no file).
        std::fs::write(dir.join("orphan.md"), "# Orphan\n").unwrap();
        std::fs::write(dir.join("order.txt"), "phantom\n").unwrap();

        let order_before = std::fs::metadata(dir.join("order.txt")).unwrap();
        let bytes_before = read(&dir.join("order.txt"));

        let board = s.load().unwrap();
        let ids: Vec<&str> = board.columns[0].cards.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["orphan"], "phantom hidden, orphan surfaced");

        let order_after = std::fs::metadata(dir.join("order.txt")).unwrap();
        assert_eq!(read(&dir.join("order.txt")), bytes_before, "load must not rewrite order.txt");
        assert_eq!(
            order_before.modified().unwrap(),
            order_after.modified().unwrap(),
            "load must not touch order.txt"
        );
    }

    #[test]
    fn first_mutation_persists_the_whole_reconciled_order() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        let dir = s.root.join("cols/todo");
        std::fs::write(dir.join("a.md"), "# A\n").unwrap();
        std::fs::write(dir.join("b.md"), "# B\n").unwrap();
        std::fs::write(dir.join("order.txt"), "b\nphantom\n").unwrap();

        // Displayed order is b, a (declared first, then orphans).
        let board = s.load().unwrap();
        let ids: Vec<&str> = board.columns[0].cards.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "a"]);

        s.create_card("todo", "c", "C").unwrap();
        // The persisted file matches what was on screen, with the phantom gone.
        assert_eq!(read(&dir.join("order.txt")), "b\na\nc\n");
    }

    #[test]
    fn normalise_order_persists_reconciled_state() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        let dir = s.root.join("cols/todo");
        std::fs::write(dir.join("a.md"), "# A\n").unwrap();
        std::fs::write(dir.join("order.txt"), "ghost\na\n").unwrap();
        s.normalise_order("todo").unwrap();
        assert_eq!(read(&dir.join("order.txt")), "a\n");
    }

    #[test]
    fn scan_ignores_non_cards_and_temp_files() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        let dir = s.root.join("cols/todo");
        std::fs::write(dir.join("real.md"), "# R\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        std::fs::write(dir.join(".hidden.md"), "x").unwrap();
        std::fs::write(dir.join(format!("{TEMP_PREFIX}123-0")), "x").unwrap();
        std::fs::create_dir(dir.join("subdir.md")).unwrap();

        let board = s.load().unwrap();
        let ids: Vec<&str> = board.columns[0].cards.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["real"]);
    }

    #[test]
    fn load_on_empty_or_missing_root_is_not_an_error() {
        let (_dir, s) = store();
        let board = s.load().unwrap();
        assert!(board.columns.is_empty());
        assert!(!s.is_initialised());

        let missing = Store::new(_dir.path().join("nope"));
        assert!(missing.load().unwrap().columns.is_empty());
    }

    #[test]
    fn load_reports_a_file_where_the_root_should_be() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("afile");
        std::fs::write(&path, "x").unwrap();
        let err = Store::new(&path).load().unwrap_err();
        assert!(err.contains("not a directory"), "got: {err}");
    }

    #[test]
    fn load_warns_about_duplicate_ids_and_missing_directories() {
        let (_dir, s) = store();
        std::fs::create_dir_all(&s.root).unwrap();
        std::fs::write(
            s.board_file_path(),
            "col todo \"TO DO\"\ncol todo \"DUPLICATE\"\ncol ghost \"GHOST\"\n",
        )
        .unwrap();
        let board = s.load().unwrap();
        assert_eq!(board.columns.len(), 2, "duplicate id collapsed, first wins");
        assert_eq!(board.columns[0].display_name, "TO DO");
        assert!(board.warnings.iter().any(|w| w.contains("more than once")));
        assert!(board.warnings.iter().any(|w| w.contains("no directory")));
    }

    #[test]
    fn load_warns_about_duplicate_display_names() {
        let (_dir, s) = store();
        std::fs::create_dir_all(&s.root).unwrap();
        std::fs::write(s.board_file_path(), "col a \"Same\"\ncol b \"Same\"\n").unwrap();
        let board = s.load().unwrap();
        assert_eq!(board.columns.len(), 2);
        assert!(board.warnings.iter().any(|w| w.contains("share the display name")));
        assert!(board.is_display_name_ambiguous("Same"));
    }

    #[test]
    fn load_reads_full_multiline_bodies() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        std::fs::write(
            s.root.join("cols/todo/c.md"),
            "# Fix login bug\nInvestigate timeout.\n\nAffects mobile only.\n",
        )
        .unwrap();
        let board = s.load().unwrap();
        let card = &board.columns[0].cards[0];
        assert_eq!(card.title, "Fix login bug");
        assert_eq!(card.body, "Investigate timeout.\n\nAffects mobile only.");
        assert_eq!(card.body_lines().len(), 3);
    }

    #[test]
    fn load_tolerates_invalid_utf8_in_a_card() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        std::fs::write(s.root.join("cols/todo/bad.md"), b"# T\n\xff\xfe invalid\n").unwrap();
        let board = s.load().unwrap();
        assert_eq!(board.columns[0].cards.len(), 1, "shows the card rather than failing the load");
    }

    // --- atomic write -------------------------------------------------------------------

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.txt");
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(read(&path), "second");

        let leftovers: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(TEMP_PREFIX))
            .collect();
        assert!(leftovers.is_empty(), "temp files left behind: {leftovers:?}");
    }

    #[test]
    fn atomic_write_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("f.txt");
        atomic_write(&path, b"x").unwrap();
        assert_eq!(read(&path), "x");
    }

    #[test]
    fn atomic_write_reports_unwritable_targets() {
        // A path whose parent is a file cannot be created.
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        assert!(atomic_write(&blocker.join("child.txt"), b"y").is_err());
    }

    #[test]
    fn suggest_card_id_avoids_ids_on_disk_and_in_order_file() {
        let (_dir, s) = store();
        s.init_board().unwrap();
        s.create_card("todo", "fix-login-bug", "Fix login bug").unwrap();
        assert_eq!(s.suggest_card_id("todo", "Fix login bug"), "fix-login-bug-2");
        // Non-ASCII titles fall back to a generated id.
        assert_eq!(s.suggest_card_id("todo", "English-only text"), "card-1");
    }
}
