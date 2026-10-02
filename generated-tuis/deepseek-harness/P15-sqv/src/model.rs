//! Core data model shared by the database layer, application state and UI.

use std::cmp::Ordering;

use regex::Regex;
use rusqlite::types::Value as SqlValue;
use unicode_width::UnicodeWidthStr;

/// A single database value, normalized across the SQLite type system.
#[derive(Clone, Debug, PartialEq)]
pub enum CellValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

const MAX_TEXT_DISPLAY: usize = 4096;
const MAX_BLOB_BYTES: usize = 128;

fn fmt_real(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Inf".into() } else { "-Inf".into() };
    }
    // Match SQLite's habit of showing a trailing ".0" for integral reals so
    // that what the user sees is stable and self-consistent with filtering.
    let s = format!("{}", f);
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        format!("{}.0", s)
    } else {
        s
    }
}

fn truncate_text(s: &str) -> String {
    if s.chars().count() > MAX_TEXT_DISPLAY {
        let t: String = s.chars().take(MAX_TEXT_DISPLAY).collect();
        format!("{}…", t)
    } else {
        s.to_string()
    }
}

fn blob_hex(b: &[u8]) -> String {
    let mut hex = String::with_capacity(b.len().min(MAX_BLOB_BYTES) * 2);
    let mut i = 0;
    while i < b.len() && i < MAX_BLOB_BYTES {
        hex.push_str(&format!("{:02X}", b[i]));
        i += 1;
    }
    if b.len() > MAX_BLOB_BYTES {
        format!("0x{}…", hex)
    } else {
        format!("0x{}", hex)
    }
}

impl CellValue {
    pub fn from_sql(v: SqlValue) -> Self {
        match v {
            SqlValue::Null => CellValue::Null,
            SqlValue::Integer(i) => CellValue::Integer(i),
            SqlValue::Real(f) => CellValue::Real(f),
            SqlValue::Text(s) => CellValue::Text(s),
            SqlValue::Blob(b) => CellValue::Blob(b),
        }
    }

    pub fn to_sql(&self) -> SqlValue {
        match self {
            CellValue::Null => SqlValue::Null,
            CellValue::Integer(i) => SqlValue::Integer(*i),
            CellValue::Real(f) => SqlValue::Real(*f),
            CellValue::Text(s) => SqlValue::Text(s.clone()),
            CellValue::Blob(b) => SqlValue::Blob(b.clone()),
        }
    }

    /// The text representation used for display and for client-side filtering.
    pub fn display(&self) -> String {
        match self {
            CellValue::Null => "NULL".to_string(),
            CellValue::Integer(i) => i.to_string(),
            CellValue::Real(f) => fmt_real(*f),
            CellValue::Text(s) => truncate_text(s),
            CellValue::Blob(b) => blob_hex(b),
        }
    }
}

/// Total order used for ascending/descending sorts.
/// Ordering groups follow SQLite's conventional ordering:
/// NULL < numbers < text < blobs.
pub fn compare_values(a: &CellValue, b: &CellValue) -> Ordering {
    use CellValue::*;

    fn is_num(v: &CellValue) -> bool {
        matches!(v, Integer(_) | Real(_))
    }
    fn num(v: &CellValue) -> f64 {
        match v {
            Integer(i) => *i as f64,
            Real(f) => *f,
            _ => 0.0,
        }
    }

    match (a, b) {
        (Null, Null) => Ordering::Equal,
        (Null, _) => Ordering::Less,
        (_, Null) => Ordering::Greater,
        (Integer(x), Integer(y)) => x.cmp(y),
        (Real(x), Real(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        _ if is_num(a) && is_num(b) => num(a).partial_cmp(&num(b)).unwrap_or(Ordering::Equal),
        (Text(x), Text(y)) => x.cmp(y),
        (Blob(x), Blob(y)) => x.cmp(y),
        _ if is_num(a) => Ordering::Less,
        _ if is_num(b) => Ordering::Greater,
        (Text(_), Blob(_)) => Ordering::Less,
        (Blob(_), Text(_)) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

/// Filter match modes supported per column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FilterMode {
    Contains,
    Regex,
    Exact,
}

impl FilterMode {
    pub fn next(self) -> Self {
        match self {
            FilterMode::Contains => FilterMode::Regex,
            FilterMode::Regex => FilterMode::Exact,
            FilterMode::Exact => FilterMode::Contains,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            FilterMode::Contains => FilterMode::Exact,
            FilterMode::Regex => FilterMode::Contains,
            FilterMode::Exact => FilterMode::Regex,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FilterMode::Contains => "contains",
            FilterMode::Regex => "regex",
            FilterMode::Exact => "exact",
        }
    }
}

/// A filter applied to a single column. Matching is performed against the
/// displayed text of each cell so that results always agree with what is
/// rendered on screen.
#[derive(Clone, Debug)]
pub struct ColumnFilter {
    pub col: usize,
    pub mode: FilterMode,
    pub value: String,
    pub regex: Option<Regex>,
}

impl ColumnFilter {
    pub fn new(col: usize, mode: FilterMode, value: String) -> Self {
        let regex = if mode == FilterMode::Regex {
            Regex::new(&value).ok()
        } else {
            None
        };
        Self {
            col,
            mode,
            value,
            regex,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.mode != FilterMode::Regex || self.regex.is_some()
    }

    pub fn matches(&self, text: &str) -> bool {
        match self.mode {
            FilterMode::Exact => text == self.value,
            FilterMode::Contains => text.contains(&self.value),
            FilterMode::Regex => self.regex.as_ref().map(|r| r.is_match(text)).unwrap_or(false),
        }
    }
}

/// A table discovered in the database.
#[derive(Clone, Debug)]
pub struct TableInfo {
    pub name: String,
    pub sql: Option<String>,
    pub has_rowid: bool,
}

/// A column discovered via `PRAGMA table_info`.
#[derive(Clone, Debug)]
pub struct ColumnInfo {
    pub cid: usize,
    pub name: String,
    pub declared_type: String,
    pub notnull: bool,
    pub pk: i64,
    pub default: Option<String>,
}

/// One loaded row. `rowid` is `Some` for ordinary (non `WITHOUT ROWID`)
/// tables and is used as the primary key for updates.
#[derive(Clone, Debug)]
pub struct DataRow {
    pub rowid: Option<i64>,
    pub cells: Vec<CellValue>,
}

/// The three main views shown in the right-hand panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Data,
    Schema,
    Query,
}

/// Which region of the screen currently receives keyboard input.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Tables,
    Main,
    QueryInput,
    QueryResult,
}

/// What a modal prompt is for.
#[derive(Clone, Debug)]
pub enum PromptKind {
    /// Filter the given column. `col` indexes `columns`.
    Filter { col: usize },
    /// Edit the cell at (`row` in current display order, `col`).
    Edit { row: usize, col: usize },
    /// Open a database file at a path.
    OpenDb,
    /// Incremental filter over the table list.
    TableSearch,
}

/// A single-line modal input bar rendered at the bottom of the screen.
#[derive(Clone, Debug)]
pub struct Prompt {
    pub title: String,
    pub buffer: TextBuffer,
    pub kind: PromptKind,
    pub mode: FilterMode,
}

/// A small editable line buffer. The cursor position is measured in
/// characters (not bytes) and converted on access so multibyte input is safe.
#[derive(Clone, Debug)]
pub struct TextBuffer {
    pub text: String,
    pub cursor: usize,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
        }
    }

    pub fn from_str(s: &str) -> Self {
        let cursor = s.chars().count();
        Self {
            text: s.to_string(),
            cursor,
        }
    }

    pub fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    pub fn insert_char(&mut self, c: char) {
        let b = self.byte_index(self.cursor);
        self.text.insert(b, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let b = self.byte_index(self.cursor - 1);
            self.text.remove(b);
            self.cursor -= 1;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.char_count() {
            let b = self.byte_index(self.cursor);
            self.text.remove(b);
        }
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.char_count());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.char_count();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// Byte offset of the cursor, for slicing.
    pub fn cursor_byte(&self) -> usize {
        self.byte_index(self.cursor)
    }

    fn byte_index(&self, char_idx: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Display width of a string (number of terminal columns).
pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_nulls_first_then_numbers_text_blob() {
        assert_eq!(compare_values(&CellValue::Null, &CellValue::Integer(1)), Ordering::Less);
        assert_eq!(compare_values(&CellValue::Integer(1), &CellValue::Real(2.0)), Ordering::Less);
        assert_eq!(compare_values(&CellValue::Real(2.0), &CellValue::Text("a".into())), Ordering::Less);
        assert_eq!(compare_values(&CellValue::Text("a".into()), &CellValue::Blob(vec![1])), Ordering::Less);
        assert_eq!(compare_values(&CellValue::Integer(5), &CellValue::Integer(3)), Ordering::Greater);
    }

    #[test]
    fn real_display_has_decimal_point() {
        assert_eq!(CellValue::Real(1.0).display(), "1.0");
        assert_eq!(CellValue::Real(1.5).display(), "1.5");
        assert_eq!(CellValue::Integer(42).display(), "42");
        assert_eq!(CellValue::Null.display(), "NULL");
    }

    #[test]
    fn filter_modes() {
        let c = ColumnFilter::new(0, FilterMode::Contains, "ell".into());
        assert!(c.matches("hello"));
        assert!(!c.matches("world"));
        let e = ColumnFilter::new(0, FilterMode::Exact, "42".into());
        assert!(e.matches("42"));
        assert!(!e.matches("42.0"));
        let r = ColumnFilter::new(0, FilterMode::Regex, "^a.*c$".into());
        assert!(r.is_valid());
        assert!(r.matches("abc"));
        assert!(!r.matches("abd"));
        let bad = ColumnFilter::new(0, FilterMode::Regex, "([".into());
        assert!(!bad.is_valid());
        assert!(!bad.matches("anything"));
    }

    #[test]
    fn text_buffer_editing_is_multibyte_safe() {
        let mut b = TextBuffer::from_str("héllo");
        b.home();
        b.insert_char('>');
        assert_eq!(b.text, ">héllo");
        b.end();
        b.backspace();
        assert_eq!(b.text, ">héll");
        b.move_left();
        b.delete();
        assert_eq!(b.text, ">hél");
    }
}
