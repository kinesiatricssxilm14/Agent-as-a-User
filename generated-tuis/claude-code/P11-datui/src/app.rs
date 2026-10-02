//! Application state, key handling and derived (cached) analysis results.
//!
//! The UI layer only renders what it finds here, so all behaviour in this
//! module can be exercised in tests without a real terminal.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::data::{Correlations, Dataset, Distribution, SortDir, Stats};
use crate::editor::Editor;
use crate::fmtnum::{CellFormat, NumFormat};

/// Query mode. Each mode keeps its own input line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Fuzzy,
    SqlLike,
    Sql,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Fuzzy, Mode::SqlLike, Mode::Sql];

    pub fn title(self) -> &'static str {
        match self {
            Mode::Fuzzy => "Fuzzy",
            Mode::SqlLike => "SQL-Like",
            Mode::Sql => "SQL",
        }
    }

    /// Placeholder shown in the empty query line, which doubles as syntax help.
    pub fn hint(self) -> &'static str {
        match self {
            Mode::Fuzzy => "type a keyword to fuzzy-match across all columns (live)",
            Mode::SqlLike => "select where age > 40 and department = 'Engineering'",
            Mode::Sql => "select * from df where country = 'US' and score > 85",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Mode::Fuzzy => 0,
            Mode::SqlLike => 1,
            Mode::Sql => 2,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Mode::Fuzzy => Mode::SqlLike,
            Mode::SqlLike => Mode::Sql,
            Mode::Sql => Mode::Fuzzy,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Mode::Fuzzy => Mode::Sql,
            Mode::SqlLike => Mode::Fuzzy,
            Mode::Sql => Mode::SqlLike,
        }
    }
}

/// Which part of the screen receives key presses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Query,
    Table,
}

/// Top-level screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Main,
    Help,
}

/// Severity of the status message, which drives its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgKind {
    Info,
    Good,
    Error,
}

/// A one-line input request, shown under the query line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// Export the current result to a path.
    Export,
    /// Open a different CSV file.
    Open,
}

impl PromptKind {
    pub fn label(self) -> &'static str {
        match self {
            PromptKind::Export => "Export result to CSV path",
            PromptKind::Open => "Open CSV file",
        }
    }
}

/// Everything the analysis panel shows for the current column and result.
pub struct Analysis {
    pub stats: Result<Stats, String>,
    pub distribution: Result<Distribution, String>,
    pub correlations: Result<Correlations, String>,
    /// Cache key: (view generation, column, number format).
    key: (u64, String, NumFormat),
}

/// The application.
pub struct App {
    pub ds: Dataset,
    pub mode: Mode,
    pub focus: Focus,
    pub screen: Screen,
    /// One editor per mode so switching modes does not lose a typed query.
    editors: [Editor; 3],
    /// Active prompt, if any.
    pub prompt: Option<(PromptKind, Editor)>,
    /// Selected row within the current view.
    pub selected: usize,
    /// First visible row; kept in sync with the drawn viewport height.
    pub row_offset: usize,
    /// Index of the focused column within the view's columns.
    pub focused_col: usize,
    /// First visible column.
    pub col_offset: usize,
    /// Whether the analysis panel is open.
    pub analysis_open: bool,
    pub analysis_scroll: u16,
    pub detail_scroll: u16,
    pub help_scroll: u16,
    pub cell_fmt: CellFormat,
    pub num_fmt: NumFormat,
    pub message: String,
    pub msg_kind: MsgKind,
    /// Rows matched by the last executed query.
    pub match_count: Option<usize>,
    /// Description of the query that produced the current view.
    pub active_query: Option<String>,
    pub should_quit: bool,
    /// Bumped whenever the view changes, to invalidate the analysis cache.
    view_gen: u64,
    cached: Option<Analysis>,
    /// Height of the table viewport from the last draw, for page scrolling.
    pub page_rows: usize,
}

impl App {
    pub fn new(ds: Dataset) -> Self {
        let rows = ds.total_rows();
        let cols = ds.columns().len();
        let mut app = App {
            ds,
            mode: Mode::Fuzzy,
            focus: Focus::Table,
            screen: Screen::Main,
            editors: [Editor::new(), Editor::new(), Editor::new()],
            prompt: None,
            selected: 0,
            row_offset: 0,
            focused_col: 0,
            col_offset: 0,
            analysis_open: false,
            analysis_scroll: 0,
            detail_scroll: 0,
            help_scroll: 0,
            cell_fmt: CellFormat::Auto,
            num_fmt: NumFormat::TwoDp,
            message: String::new(),
            msg_kind: MsgKind::Info,
            match_count: None,
            active_query: None,
            should_quit: false,
            view_gen: 0,
            cached: None,
            page_rows: 10,
        };
        app.set_msg(
            format!(
                "Loaded {rows} rows x {cols} columns. Press ? for help, Tab to type a query.",
            ),
            MsgKind::Good,
        );
        app
    }

    // ----- messages -------------------------------------------------------

    pub fn set_msg(&mut self, msg: impl Into<String>, kind: MsgKind) {
        self.message = msg.into();
        self.msg_kind = kind;
    }

    // ----- query editors --------------------------------------------------

    pub fn editor(&self) -> &Editor {
        &self.editors[self.mode.index()]
    }

    pub fn editor_mut(&mut self) -> &mut Editor {
        let i = self.mode.index();
        &mut self.editors[i]
    }

    pub fn query_text(&self) -> String {
        self.editor().text()
    }

    // ----- view helpers ---------------------------------------------------

    pub fn view_columns(&self) -> Vec<String> {
        self.ds.view_columns()
    }

    /// Name of the currently focused column, if the view has any columns.
    pub fn focused_column(&self) -> Option<String> {
        let cols = self.view_columns();
        if cols.is_empty() {
            return None;
        }
        Some(cols[self.focused_col.min(cols.len() - 1)].clone())
    }

    pub fn row_count(&self) -> usize {
        self.ds.view_rows()
    }

    /// Called after the view changes: clamp selection and drop caches.
    fn view_changed(&mut self) {
        self.view_gen = self.view_gen.wrapping_add(1);
        self.cached = None;
        let rows = self.row_count();
        if rows == 0 {
            self.selected = 0;
            self.row_offset = 0;
        } else if self.selected >= rows {
            self.selected = rows - 1;
        }
        let cols = self.view_columns().len();
        if cols == 0 {
            self.focused_col = 0;
            self.col_offset = 0;
        } else {
            if self.focused_col >= cols {
                self.focused_col = cols - 1;
            }
            if self.col_offset >= cols {
                self.col_offset = cols.saturating_sub(1);
            }
        }
        self.detail_scroll = 0;
        self.analysis_scroll = 0;
    }

    // ----- query execution ------------------------------------------------

    /// Execute the current query line in the current mode.
    pub fn run_query(&mut self) {
        let q = self.query_text();
        let trimmed = q.trim().to_string();

        if trimmed.is_empty() {
            match self.ds.clear_query() {
                Ok(()) => {
                    self.view_changed();
                    self.match_count = None;
                    self.active_query = None;
                    let n = self.row_count();
                    self.set_msg(format!("Cleared query. Showing all {n} rows."), MsgKind::Info);
                }
                Err(e) => self.set_msg(e, MsgKind::Error),
            }
            return;
        }

        let result = match self.mode {
            Mode::Fuzzy => self.ds.fuzzy_ranked(&trimmed),
            Mode::SqlLike => self.ds.query_sql_like(&trimmed),
            Mode::Sql => self.ds.query_sql(&trimmed),
        };

        match result {
            Ok(n) => {
                self.view_changed();
                self.match_count = Some(n);
                self.active_query = Some(format!("{}: {}", self.mode.title(), trimmed));
                self.editor_mut().remember();
                let total = self.ds.total_rows();
                if n == 0 {
                    self.set_msg(
                        format!("No matches for `{trimmed}` ({total} rows searched)."),
                        MsgKind::Info,
                    );
                } else {
                    self.selected = 0;
                    self.row_offset = 0;
                    self.set_msg(
                        format!("{n} of {total} rows matched `{trimmed}`."),
                        MsgKind::Good,
                    );
                }
            }
            Err(e) => {
                // Leave the previous result in place so the error is recoverable.
                self.set_msg(format!("{} error: {e}", self.mode.title()), MsgKind::Error);
            }
        }
    }

    /// Re-run the fuzzy search as the user types, for real-time results.
    fn live_search(&mut self) {
        if self.mode != Mode::Fuzzy {
            return;
        }
        // The search index makes per-keystroke scanning cheap, but on very
        // large files each keystroke still rebuilds and re-sorts the result, so
        // past this size the query only runs on Enter.
        if self.ds.total_rows() > 500_000 {
            return;
        }
        let q = self.query_text();
        let trimmed = q.trim().to_string();
        let result = if trimmed.is_empty() {
            self.ds.clear_query().map(|()| self.ds.view_rows())
        } else {
            self.ds.fuzzy_ranked(&trimmed)
        };
        match result {
            Ok(n) => {
                self.view_changed();
                self.selected = 0;
                self.row_offset = 0;
                if trimmed.is_empty() {
                    self.match_count = None;
                    self.active_query = None;
                } else {
                    self.match_count = Some(n);
                    self.active_query = Some(format!("Fuzzy: {trimmed}"));
                }
            }
            Err(e) => self.set_msg(e, MsgKind::Error),
        }
    }

    pub fn clear_query(&mut self) {
        self.editor_mut().clear();
        match self.ds.clear_query() {
            Ok(()) => {
                self.view_changed();
                self.match_count = None;
                self.active_query = None;
                let n = self.row_count();
                self.set_msg(format!("Query cleared. Showing all {n} rows."), MsgKind::Info);
            }
            Err(e) => self.set_msg(e, MsgKind::Error),
        }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.set_msg(
            format!("{} mode. {}", mode.title(), mode.hint()),
            MsgKind::Info,
        );
        if mode == Mode::Fuzzy && self.focus == Focus::Query {
            self.live_search();
        }
    }

    // ----- navigation -----------------------------------------------------

    pub fn select_row(&mut self, row: usize) {
        let rows = self.row_count();
        if rows == 0 {
            self.selected = 0;
            return;
        }
        self.selected = row.min(rows - 1);
        self.detail_scroll = 0;
    }

    pub fn move_row(&mut self, delta: isize) {
        let rows = self.row_count();
        if rows == 0 {
            return;
        }
        let cur = self.selected as isize;
        let next = (cur + delta).clamp(0, rows as isize - 1);
        self.selected = next as usize;
        self.detail_scroll = 0;
    }

    pub fn move_col(&mut self, delta: isize) {
        let cols = self.view_columns().len();
        if cols == 0 {
            return;
        }
        let cur = self.focused_col as isize;
        let next = (cur + delta).clamp(0, cols as isize - 1);
        self.focused_col = next as usize;
        // Analysis follows the focused column, so its cache must be rechecked.
        if self.analysis_open {
            self.analysis_scroll = 0;
        }
    }

    // ----- data operations ------------------------------------------------

    pub fn toggle_sort(&mut self, replace: bool) {
        let Some(col) = self.focused_column() else {
            self.set_msg("No column to sort.", MsgKind::Error);
            return;
        };
        if replace {
            // A primary sort replaces any existing keys.
            let keep = self
                .ds
                .sort_keys
                .iter()
                .find(|k| k.column == col)
                .map(|k| k.dir);
            self.ds.sort_keys.clear();
            let dir = match keep {
                Some(d) => d.flip(),
                None => SortDir::Asc,
            };
            if let Err(e) = self.ds.add_sort_key(&col, dir) {
                self.set_msg(e, MsgKind::Error);
                return;
            }
            self.view_changed();
            self.set_msg(
                format!("Sorted by {col} ({}).", dir.label()),
                MsgKind::Good,
            );
        } else {
            match self.ds.toggle_sort(&col) {
                Ok(dir) => {
                    self.view_changed();
                    let keys = self.sort_summary();
                    self.set_msg(
                        format!("Sort key {col} ({}) added. Order: {keys}", dir.label()),
                        MsgKind::Good,
                    );
                }
                Err(e) => self.set_msg(e, MsgKind::Error),
            }
        }
    }

    pub fn clear_sort(&mut self) {
        if self.ds.sort_keys.is_empty() {
            self.set_msg("No sort keys to clear.", MsgKind::Info);
            return;
        }
        let _ = self.ds.clear_sort();
        // Re-running the query restores the natural (unsorted) order.
        let q = self.query_text();
        if q.trim().is_empty() {
            let _ = self.ds.clear_query();
        } else {
            let r = match self.mode {
                Mode::Fuzzy => self.ds.fuzzy_ranked(q.trim()),
                Mode::SqlLike => self.ds.query_sql_like(q.trim()),
                Mode::Sql => self.ds.query_sql(q.trim()),
            };
            if let Err(e) = r {
                self.set_msg(e, MsgKind::Error);
                return;
            }
        }
        self.view_changed();
        self.set_msg("Sort cleared.", MsgKind::Good);
    }

    pub fn sort_summary(&self) -> String {
        if self.ds.sort_keys.is_empty() {
            return "none".to_string();
        }
        self.ds
            .sort_keys
            .iter()
            .map(|k| format!("{}{}", k.column, k.dir.arrow()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn reload(&mut self) {
        let path = self.ds.path.clone();
        match Dataset::load(&path) {
            Ok(mut ds) => {
                // Keep the active sort across a reload.
                ds.sort_keys = std::mem::take(&mut self.ds.sort_keys);
                let _ = ds.apply_sort();
                self.ds = ds;
                self.view_changed();
                let q = self.query_text();
                if !q.trim().is_empty() {
                    self.run_query();
                }
                self.set_msg(
                    format!(
                        "Reloaded {} ({} rows).",
                        path.display(),
                        self.ds.total_rows()
                    ),
                    MsgKind::Good,
                );
            }
            Err(e) => self.set_msg(e, MsgKind::Error),
        }
    }

    /// Open the export prompt, pre-filled with a sensible default path.
    pub fn begin_export(&mut self) {
        let mut ed = Editor::new();
        let stem = self
            .ds
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("result");
        let dir = self
            .ds
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let default = dir.join(format!("{stem}-export.csv"));
        ed.set_text(&default.to_string_lossy());
        self.prompt = Some((PromptKind::Export, ed));
        self.set_msg(
            "Enter an export path, then press Enter (Esc cancels).",
            MsgKind::Info,
        );
    }

    pub fn begin_open(&mut self) {
        let mut ed = Editor::new();
        ed.set_text(&self.ds.path.to_string_lossy());
        self.prompt = Some((PromptKind::Open, ed));
        self.set_msg("Enter a CSV path to open (Esc cancels).", MsgKind::Info);
    }

    fn submit_prompt(&mut self) {
        let Some((kind, ed)) = self.prompt.take() else {
            return;
        };
        let raw = ed.text();
        let text = raw.trim();
        if text.is_empty() {
            self.set_msg("Cancelled: no path given.", MsgKind::Info);
            return;
        }
        let path = PathBuf::from(expand_home(text));
        match kind {
            PromptKind::Export => match self.ds.export_csv(&path) {
                Ok(n) => self.set_msg(
                    format!(
                        "Exported {n} rows x {} columns to {}",
                        self.view_columns().len(),
                        path.display()
                    ),
                    MsgKind::Good,
                ),
                Err(e) => self.set_msg(format!("Export failed: {e}"), MsgKind::Error),
            },
            PromptKind::Open => match Dataset::load(&path) {
                Ok(ds) => {
                    self.ds = ds;
                    self.editors = [Editor::new(), Editor::new(), Editor::new()];
                    self.selected = 0;
                    self.row_offset = 0;
                    self.focused_col = 0;
                    self.col_offset = 0;
                    self.match_count = None;
                    self.active_query = None;
                    self.view_changed();
                    self.set_msg(
                        format!(
                            "Opened {} ({} rows x {} columns).",
                            path.display(),
                            self.ds.total_rows(),
                            self.ds.columns().len()
                        ),
                        MsgKind::Good,
                    );
                }
                Err(e) => self.set_msg(format!("Open failed: {e}"), MsgKind::Error),
            },
        }
    }

    // ----- analysis -------------------------------------------------------

    /// Analysis for the focused column, computed on demand and cached.
    pub fn analysis(&mut self) -> Option<&Analysis> {
        let col = self.focused_column()?;
        let key = (self.view_gen, col.clone(), self.num_fmt);
        let fresh = self.cached.as_ref().map(|a| a.key == key).unwrap_or(false);
        if !fresh {
            let stats = self.ds.describe(&col, self.num_fmt);
            let distribution = self.ds.distribution(&col, 8);
            let correlations = self.ds.correlations(self.num_fmt);
            self.cached = Some(Analysis {
                stats,
                distribution,
                correlations,
                key,
            });
        }
        self.cached.as_ref()
    }

    pub fn toggle_analysis(&mut self) {
        self.analysis_open = !self.analysis_open;
        self.analysis_scroll = 0;
        if self.analysis_open {
            let col = self.focused_column().unwrap_or_default();
            self.set_msg(
                format!(
                    "Analysis of `{col}` ({} format). ←/→ picks the column, F toggles format, a closes.",
                    self.num_fmt.label()
                ),
                MsgKind::Info,
            );
        } else {
            self.set_msg("Analysis closed.", MsgKind::Info);
        }
    }

    pub fn cycle_num_fmt(&mut self) {
        self.num_fmt = self.num_fmt.next();
        self.cached = None;
        let desc = match self.num_fmt {
            NumFormat::TwoDp => "two decimals, zero padded (50.00)",
            NumFormat::Sig3 => "three significant figures (0.878)",
        };
        self.set_msg(format!("Analysis numbers: {desc}."), MsgKind::Info);
    }

    pub fn cycle_cell_fmt(&mut self) {
        self.cell_fmt = self.cell_fmt.next();
        let desc = match self.cell_fmt {
            CellFormat::Auto => "as stored in the file",
            CellFormat::TwoDp => "two decimals, zero padded",
            CellFormat::Sig3 => "three significant figures",
        };
        self.set_msg(format!("Table numbers: {desc}."), MsgKind::Info);
    }

    // ----- key handling ---------------------------------------------------

    /// Handle one key press. Returns immediately for key releases/repeats that
    /// would otherwise double-apply on some terminals.
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // Quit works from anywhere.
        if ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('q')) {
            self.should_quit = true;
            return;
        }

        // An active prompt takes all remaining keys.
        if self.prompt.is_some() {
            self.prompt_key(key, ctrl);
            return;
        }

        if self.screen == Screen::Help {
            self.help_key(key);
            return;
        }

        // Mode selection works regardless of focus.
        match key.code {
            KeyCode::F(1) => {
                self.screen = Screen::Help;
                self.help_scroll = 0;
                return;
            }
            KeyCode::F(2) => {
                self.set_mode(Mode::Fuzzy);
                return;
            }
            KeyCode::F(3) => {
                self.set_mode(Mode::SqlLike);
                return;
            }
            KeyCode::F(4) => {
                self.set_mode(Mode::Sql);
                return;
            }
            KeyCode::BackTab => {
                let m = self.mode.prev();
                self.set_mode(m);
                return;
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Query => Focus::Table,
                    Focus::Table => Focus::Query,
                };
                let msg = match self.focus {
                    Focus::Query => format!("Editing the {} query. Enter runs it, Esc returns to the table.", self.mode.title()),
                    Focus::Table => "Browsing rows. Tab returns to the query line.".to_string(),
                };
                self.set_msg(msg, MsgKind::Info);
                return;
            }
            _ => {}
        }

        if ctrl {
            match key.code {
                KeyCode::Right => {
                    let m = self.mode.next();
                    self.set_mode(m);
                    return;
                }
                KeyCode::Left => {
                    let m = self.mode.prev();
                    self.set_mode(m);
                    return;
                }
                _ => {}
            }
        }

        match self.focus {
            Focus::Query => self.query_key(key, ctrl),
            Focus::Table => self.table_key(key, ctrl),
        }
    }

    fn prompt_key(&mut self, key: KeyEvent, ctrl: bool) {
        let Some((_, ed)) = self.prompt.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Enter => self.submit_prompt(),
            KeyCode::Esc => {
                self.prompt = None;
                self.set_msg("Cancelled.", MsgKind::Info);
            }
            KeyCode::Backspace => ed.backspace(),
            KeyCode::Delete => ed.delete(),
            KeyCode::Left => ed.left(),
            KeyCode::Right => ed.right(),
            KeyCode::Home => ed.home(),
            KeyCode::End => ed.end(),
            KeyCode::Char(c) if ctrl => match c {
                'u' => ed.kill_to_start(),
                'k' => ed.kill_to_end(),
                'w' => ed.delete_word(),
                'a' => ed.home(),
                'e' => ed.end(),
                _ => {}
            },
            KeyCode::Char(c) => ed.insert(c),
            _ => {}
        }
    }

    fn help_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.help_scroll = self.help_scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => self.help_scroll = self.help_scroll.saturating_sub(1),
            KeyCode::PageDown => self.help_scroll = self.help_scroll.saturating_add(10),
            KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(10),
            KeyCode::Home => self.help_scroll = 0,
            _ => {
                // Any other key closes help, so it is never a trap.
                self.screen = Screen::Main;
                self.set_msg("Help closed.", MsgKind::Info);
            }
        }
    }

    fn query_key(&mut self, key: KeyEvent, ctrl: bool) {
        match key.code {
            KeyCode::Enter => {
                self.run_query();
                // Jump to the results so they can be browsed straight away.
                self.focus = Focus::Table;
            }
            KeyCode::Esc => {
                self.focus = Focus::Table;
                self.set_msg("Browsing rows. Tab returns to the query line.", MsgKind::Info);
            }
            KeyCode::Char(c) if ctrl => {
                match c {
                    'u' => self.editor_mut().kill_to_start(),
                    'k' => self.editor_mut().kill_to_end(),
                    'w' => self.editor_mut().delete_word(),
                    'a' => self.editor_mut().home(),
                    'e' => self.editor_mut().end(),
                    'l' => {
                        self.clear_query();
                        return;
                    }
                    _ => return,
                }
                self.live_search();
            }
            KeyCode::Backspace => {
                self.editor_mut().backspace();
                self.live_search();
            }
            KeyCode::Delete => {
                self.editor_mut().delete();
                self.live_search();
            }
            KeyCode::Left => self.editor_mut().left(),
            KeyCode::Right => self.editor_mut().right(),
            KeyCode::Home => self.editor_mut().home(),
            KeyCode::End => self.editor_mut().end(),
            KeyCode::Up => {
                if self.editor_mut().history_prev() {
                    self.live_search();
                }
            }
            KeyCode::Down => {
                if self.editor_mut().history_next() {
                    self.live_search();
                }
            }
            KeyCode::PageDown => self.move_row(self.page_rows as isize),
            KeyCode::PageUp => self.move_row(-(self.page_rows as isize)),
            KeyCode::Char(c) => {
                self.editor_mut().insert(c);
                self.live_search();
            }
            _ => {}
        }
    }

    fn table_key(&mut self, key: KeyEvent, ctrl: bool) {
        let page = self.page_rows.max(1) as isize;
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            // Esc backs out one step at a time rather than quitting, so it is
            // never a destructive keypress. `q` and Ctrl-c still exit.
            KeyCode::Esc => {
                if self.analysis_open {
                    self.analysis_open = false;
                    self.set_msg("Analysis closed. Press q to quit.", MsgKind::Info);
                } else if self.match_count.is_some() || !self.query_text().is_empty() {
                    self.clear_query();
                } else {
                    self.set_msg("Press q or Ctrl-c to quit, ? for help.", MsgKind::Info);
                }
            }
            KeyCode::Char('?') | KeyCode::Char('h') if !ctrl => {
                // `h` is also the vim-style left key; only bare `?` opens help.
                if matches!(key.code, KeyCode::Char('?')) {
                    self.screen = Screen::Help;
                    self.help_scroll = 0;
                } else {
                    self.move_col(-1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_row(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_row(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.move_row(page),
            KeyCode::PageUp => self.move_row(-page),
            KeyCode::Home | KeyCode::Char('g') => self.select_row(0),
            KeyCode::End | KeyCode::Char('G') => {
                let last = self.row_count().saturating_sub(1);
                self.select_row(last);
            }
            KeyCode::Right | KeyCode::Char('l') => self.move_col(1),
            KeyCode::Left => self.move_col(-1),
            KeyCode::Char('s') => self.toggle_sort(true),
            KeyCode::Char('S') => self.toggle_sort(false),
            KeyCode::Char('R') => self.clear_sort(),
            KeyCode::Char('a') => self.toggle_analysis(),
            KeyCode::Char('e') => self.begin_export(),
            KeyCode::Char('o') => self.begin_open(),
            KeyCode::Char('r') => self.reload(),
            KeyCode::Char('x') => self.cycle_cell_fmt(),
            KeyCode::Char('F') => self.cycle_num_fmt(),
            KeyCode::Char('c') => self.clear_query(),
            KeyCode::Char('m') => {
                let m = self.mode.next();
                self.set_mode(m);
            }
            KeyCode::Char('/') | KeyCode::Char('f') => {
                self.focus = Focus::Query;
                if matches!(key.code, KeyCode::Char('/')) {
                    self.set_mode(Mode::Fuzzy);
                }
                self.set_msg(
                    format!("{} query: {}", self.mode.title(), self.mode.hint()),
                    MsgKind::Info,
                );
            }
            KeyCode::Enter => {
                self.focus = Focus::Query;
                self.set_msg("Editing the query line.", MsgKind::Info);
            }
            // Scroll the side panels, which may be taller than the screen.
            KeyCode::Char('J') => {
                if self.analysis_open {
                    self.analysis_scroll = self.analysis_scroll.saturating_add(1);
                } else {
                    self.detail_scroll = self.detail_scroll.saturating_add(1);
                }
            }
            KeyCode::Char('K') => {
                if self.analysis_open {
                    self.analysis_scroll = self.analysis_scroll.saturating_sub(1);
                } else {
                    self.detail_scroll = self.detail_scroll.saturating_sub(1);
                }
            }
            KeyCode::Char('n') => self.detail_scroll = self.detail_scroll.saturating_add(1),
            KeyCode::Char('p') => self.detail_scroll = self.detail_scroll.saturating_sub(1),
            _ => {}
        }
    }
}

/// Expand a leading `~` to the user's home directory.
fn expand_home(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            let mut path = PathBuf::from(home);
            path.push(rest);
            return path.to_string_lossy().into_owned();
        }
    }
    if p == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).to_string_lossy().into_owned();
        }
    }
    p.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const SAMPLE: &str = "name,age,department,salary,score\n\
                          Ann,35,Engineering,12000,88.5\n\
                          Bob,45,Sales,9000,72\n\
                          Cid,52,Engineering,20000,91.25\n\
                          Dee,29,Sales,5000,64\n";

    fn app(tag: &str) -> App {
        let mut p = std::env::temp_dir();
        p.push(format!("toolk_app_{tag}_{}.csv", std::process::id()));
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(SAMPLE.as_bytes()).unwrap();
        App::new(Dataset::load(&p).unwrap())
    }

    fn press(a: &mut App, code: KeyCode) {
        a.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn ctrl(a: &mut App, c: char) {
        a.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
    }

    fn type_str(a: &mut App, s: &str) {
        for c in s.chars() {
            press(a, KeyCode::Char(c));
        }
    }

    fn names(a: &App) -> Vec<String> {
        (0..a.row_count())
            .map(|i| a.ds.cell(i, "name", CellFormat::Auto))
            .collect()
    }

    #[test]
    fn starts_on_the_table_with_all_rows() {
        let a = app("start");
        assert_eq!(a.focus, Focus::Table);
        assert_eq!(a.mode, Mode::Fuzzy);
        assert_eq!(a.row_count(), 4);
        assert_eq!(a.view_columns().len(), 5);
    }

    #[test]
    fn tab_switches_focus_and_typing_searches_live() {
        let mut a = app("live");
        press(&mut a, KeyCode::Tab);
        assert_eq!(a.focus, Focus::Query);
        type_str(&mut a, "Sales");
        // Fuzzy mode filters on every keystroke, with no Enter needed.
        assert_eq!(a.row_count(), 2);
        assert_eq!(a.match_count, Some(2));
        // Backspacing widens the result again.
        press(&mut a, KeyCode::Backspace);
        assert!(a.row_count() >= 2);
    }

    #[test]
    fn sql_like_runs_on_enter_and_reports_the_count() {
        let mut a = app("like");
        press(&mut a, KeyCode::F(3));
        assert_eq!(a.mode, Mode::SqlLike);
        press(&mut a, KeyCode::Tab);
        type_str(&mut a, "select where age > 40");
        // SQL modes wait for Enter, since a partial statement is not valid.
        assert_eq!(a.row_count(), 4);
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.row_count(), 2);
        assert_eq!(a.match_count, Some(2));
        assert_eq!(names(&a), vec!["Bob", "Cid"]);
        // Focus moves to the results so they can be browsed immediately.
        assert_eq!(a.focus, Focus::Table);
        assert!(a.message.contains('2'), "{}", a.message);
    }

    #[test]
    fn sql_mode_runs_standard_statements() {
        let mut a = app("sql");
        press(&mut a, KeyCode::F(4));
        press(&mut a, KeyCode::Tab);
        type_str(&mut a, "select * from df where department = 'Sales' and salary > 6000");
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.match_count, Some(1));
        assert_eq!(names(&a), vec!["Bob"]);
    }

    #[test]
    fn each_mode_keeps_its_own_query_line() {
        let mut a = app("modes");
        press(&mut a, KeyCode::Tab);
        type_str(&mut a, "Sales");
        press(&mut a, KeyCode::F(3));
        type_str(&mut a, "select where age > 40");
        assert_eq!(a.query_text(), "select where age > 40");
        press(&mut a, KeyCode::F(2));
        assert_eq!(a.query_text(), "Sales");
    }

    #[test]
    fn a_bad_query_shows_an_error_and_keeps_the_previous_result() {
        let mut a = app("badq");
        press(&mut a, KeyCode::F(3));
        press(&mut a, KeyCode::Tab);
        type_str(&mut a, "select where age > 40");
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.row_count(), 2);

        press(&mut a, KeyCode::Tab);
        ctrl(&mut a, 'u');
        type_str(&mut a, "select where nosuchcol = 1");
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.msg_kind, MsgKind::Error);
        assert!(a.message.contains("unknown column"), "{}", a.message);
        // The earlier result is still on screen.
        assert_eq!(a.row_count(), 2);
    }

    #[test]
    fn navigation_clamps_at_both_ends() {
        let mut a = app("nav");
        for _ in 0..20 {
            press(&mut a, KeyCode::Down);
        }
        assert_eq!(a.selected, 3);
        for _ in 0..20 {
            press(&mut a, KeyCode::Up);
        }
        assert_eq!(a.selected, 0);
        press(&mut a, KeyCode::Char('G'));
        assert_eq!(a.selected, 3);
        press(&mut a, KeyCode::Char('g'));
        assert_eq!(a.selected, 0);
    }

    #[test]
    fn column_focus_moves_and_clamps() {
        let mut a = app("cols");
        press(&mut a, KeyCode::Right);
        assert_eq!(a.focused_column().unwrap(), "age");
        press(&mut a, KeyCode::Char('l'));
        assert_eq!(a.focused_column().unwrap(), "department");
        press(&mut a, KeyCode::Char('h'));
        assert_eq!(a.focused_column().unwrap(), "age");
        for _ in 0..10 {
            press(&mut a, KeyCode::Right);
        }
        assert_eq!(a.focused_column().unwrap(), "score");
        for _ in 0..10 {
            press(&mut a, KeyCode::Left);
        }
        assert_eq!(a.focused_column().unwrap(), "name");
    }

    #[test]
    fn sort_and_reset_are_keyboard_driven() {
        let mut a = app("sort");
        press(&mut a, KeyCode::Right); // focus `age`
        press(&mut a, KeyCode::Char('s'));
        assert_eq!(names(&a), vec!["Dee", "Ann", "Bob", "Cid"]);
        press(&mut a, KeyCode::Char('s')); // toggles to descending
        assert_eq!(names(&a), vec!["Cid", "Bob", "Ann", "Dee"]);
        // A secondary key keeps the first one.
        press(&mut a, KeyCode::Char('R'));
        assert_eq!(a.sort_summary(), "none");
        press(&mut a, KeyCode::Char('S'));
        press(&mut a, KeyCode::Left);
        press(&mut a, KeyCode::Char('S'));
        assert_eq!(a.ds.sort_keys.len(), 2);
        assert!(a.sort_summary().contains("age"));
        assert!(a.sort_summary().contains("name"));
    }

    #[test]
    fn analysis_opens_for_the_focused_column_and_reformats() {
        let mut a = app("analysis");
        press(&mut a, KeyCode::Right);
        press(&mut a, KeyCode::Right);
        press(&mut a, KeyCode::Right); // `salary`
        press(&mut a, KeyCode::Char('a'));
        assert!(a.analysis_open);
        let an = a.analysis().expect("analysis available");
        let stats = an.stats.as_ref().expect("numeric stats");
        assert_eq!(stats.column, "salary");
        let mean = stats.rows.iter().find(|(k, _)| k == "mean").unwrap();
        assert_eq!(mean.1, "11500.00");
        // Distribution and correlation are computed for the same screen.
        assert!(an.distribution.is_ok());
        assert!(an.correlations.is_ok());

        // Switching format re-renders the same metrics with three sig figs.
        press(&mut a, KeyCode::Char('F'));
        assert_eq!(a.num_fmt, NumFormat::Sig3);
        let an = a.analysis().unwrap();
        let stats = an.stats.as_ref().unwrap();
        let mean = stats.rows.iter().find(|(k, _)| k == "mean").unwrap();
        assert_eq!(mean.1, "11500");
    }

    #[test]
    fn analysis_follows_the_focused_column() {
        let mut a = app("analysiscol");
        press(&mut a, KeyCode::Char('a'));
        assert_eq!(a.analysis().unwrap().stats.as_ref().unwrap().column, "name");
        press(&mut a, KeyCode::Right);
        assert_eq!(a.analysis().unwrap().stats.as_ref().unwrap().column, "age");
    }

    #[test]
    fn analysis_reflects_the_filtered_result() {
        let mut a = app("analysisfilter");
        press(&mut a, KeyCode::F(3));
        press(&mut a, KeyCode::Tab);
        type_str(&mut a, "select where department = 'Sales'");
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Right);
        press(&mut a, KeyCode::Right);
        press(&mut a, KeyCode::Right); // salary
        let an = a.analysis().unwrap();
        let stats = an.stats.as_ref().unwrap();
        let mean = stats.rows.iter().find(|(k, _)| k == "mean").unwrap();
        // Only the two Sales rows: (9000 + 5000) / 2.
        assert_eq!(mean.1, "7000.00");
    }

    #[test]
    fn export_prompt_writes_the_current_result() {
        let mut a = app("export");
        press(&mut a, KeyCode::F(3));
        press(&mut a, KeyCode::Tab);
        type_str(&mut a, "select where age > 40");
        press(&mut a, KeyCode::Enter);

        press(&mut a, KeyCode::Char('e'));
        assert!(a.prompt.is_some());
        ctrl(&mut a, 'u');
        let mut out = std::env::temp_dir();
        out.push(format!("toolk_app_export_{}.csv", std::process::id()));
        type_str(&mut a, &out.to_string_lossy());
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.msg_kind, MsgKind::Good, "{}", a.message);
        assert!(a.prompt.is_none());

        let body = std::fs::read_to_string(&out).unwrap();
        let lines: Vec<&str> = body.trim().lines().collect();
        assert_eq!(lines[0], "name,age,department,salary,score");
        assert_eq!(lines.len(), 3);
        std::fs::remove_file(&out).ok();
    }

    #[test]
    fn export_can_be_cancelled() {
        let mut a = app("exportcancel");
        press(&mut a, KeyCode::Char('e'));
        press(&mut a, KeyCode::Esc);
        assert!(a.prompt.is_none());
        // Esc cancelled the prompt without also quitting the app.
        assert!(!a.should_quit);
    }

    #[test]
    fn export_reports_a_bad_path() {
        let mut a = app("exportbad");
        press(&mut a, KeyCode::Char('e'));
        ctrl(&mut a, 'u');
        type_str(&mut a, "/proc/nonexistent-dir/x/y.csv");
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.msg_kind, MsgKind::Error);
    }

    #[test]
    fn help_opens_and_closes() {
        let mut a = app("help");
        press(&mut a, KeyCode::Char('?'));
        assert_eq!(a.screen, Screen::Help);
        press(&mut a, KeyCode::Down);
        assert_eq!(a.help_scroll, 1);
        // Help must never be a trap: any other key returns to the table.
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.screen, Screen::Main);
        assert!(!a.should_quit);
        press(&mut a, KeyCode::F(1));
        assert_eq!(a.screen, Screen::Help);
    }

    #[test]
    fn typing_a_letter_in_the_query_does_not_trigger_table_keys() {
        let mut a = app("noleak");
        press(&mut a, KeyCode::Tab);
        // `q` and `a` are table shortcuts but must be plain text while typing.
        type_str(&mut a, "qa");
        assert!(!a.should_quit);
        assert!(!a.analysis_open);
        assert_eq!(a.query_text(), "qa");
    }

    #[test]
    fn esc_backs_out_instead_of_quitting() {
        let mut a = app("escback");
        press(&mut a, KeyCode::Tab);
        type_str(&mut a, "Sales");
        press(&mut a, KeyCode::Esc); // leaves the query line
        assert_eq!(a.focus, Focus::Table);
        assert!(!a.should_quit);
        press(&mut a, KeyCode::Char('a'));
        press(&mut a, KeyCode::Esc); // closes the analysis panel
        assert!(!a.analysis_open);
        assert!(!a.should_quit);
        press(&mut a, KeyCode::Esc); // clears the query
        assert_eq!(a.row_count(), 4);
        assert!(a.query_text().is_empty());
        assert!(!a.should_quit);
        // Esc on an idle table is a no-op that points at the quit key.
        press(&mut a, KeyCode::Esc);
        assert!(!a.should_quit);
        assert!(a.message.contains('q'), "{}", a.message);
    }

    #[test]
    fn quit_keys_work_from_the_table() {
        let mut a = app("quit");
        press(&mut a, KeyCode::Char('q'));
        assert!(a.should_quit);
        let mut a = app("quit2");
        ctrl(&mut a, 'c');
        assert!(a.should_quit);
    }

    #[test]
    fn mode_cycling_wraps_in_both_directions() {
        let mut a = app("cycle");
        press(&mut a, KeyCode::Char('m'));
        assert_eq!(a.mode, Mode::SqlLike);
        press(&mut a, KeyCode::Char('m'));
        assert_eq!(a.mode, Mode::Sql);
        press(&mut a, KeyCode::Char('m'));
        assert_eq!(a.mode, Mode::Fuzzy);
        press(&mut a, KeyCode::BackTab);
        assert_eq!(a.mode, Mode::Sql);
    }

    #[test]
    fn clearing_the_query_restores_every_row() {
        let mut a = app("clear");
        press(&mut a, KeyCode::Tab);
        type_str(&mut a, "Sales");
        assert_eq!(a.row_count(), 2);
        ctrl(&mut a, 'l');
        assert_eq!(a.row_count(), 4);
        assert!(a.query_text().is_empty());
        assert_eq!(a.match_count, None);
    }

    #[test]
    fn row_detail_exposes_every_column_of_the_selection() {
        let mut a = app("detail");
        press(&mut a, KeyCode::Down);
        let cells = a.ds.row_cells(a.selected, a.cell_fmt);
        assert_eq!(cells.len(), 5);
        assert_eq!(cells[0], ("name".to_string(), "Bob".to_string()));
        assert_eq!(cells[3], ("salary".to_string(), "9000".to_string()));
    }

    #[test]
    fn query_history_is_recalled_with_the_arrow_keys() {
        let mut a = app("hist");
        press(&mut a, KeyCode::F(3));
        press(&mut a, KeyCode::Tab);
        type_str(&mut a, "select where age > 40");
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Tab);
        ctrl(&mut a, 'u');
        press(&mut a, KeyCode::Up);
        assert_eq!(a.query_text(), "select where age > 40");
    }

    #[test]
    fn reload_rereads_the_file_from_disk() {
        let mut a = app("reload");
        let path = a.ds.path.clone();
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"Eve,61,Support,7000,55\n").unwrap();
        drop(f);
        press(&mut a, KeyCode::Char('r'));
        assert_eq!(a.ds.total_rows(), 5);
        assert_eq!(a.msg_kind, MsgKind::Good);
    }

    #[test]
    fn open_prompt_loads_a_different_file() {
        let mut a = app("open");
        let mut other = std::env::temp_dir();
        other.push(format!("toolk_other_{}.csv", std::process::id()));
        std::fs::write(&other, "city,pop\nOslo,700000\nBergen,280000\n").unwrap();
        press(&mut a, KeyCode::Char('o'));
        ctrl(&mut a, 'u');
        type_str(&mut a, &other.to_string_lossy());
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.msg_kind, MsgKind::Good, "{}", a.message);
        assert_eq!(a.view_columns(), vec!["city", "pop"]);
        assert_eq!(a.row_count(), 2);
        std::fs::remove_file(&other).ok();
    }

    #[test]
    fn table_number_format_can_be_cycled() {
        let mut a = app("cellfmt");
        assert_eq!(a.ds.cell(0, "score", a.cell_fmt), "88.5");
        press(&mut a, KeyCode::Char('x'));
        assert_eq!(a.cell_fmt, CellFormat::TwoDp);
        assert_eq!(a.ds.cell(0, "score", a.cell_fmt), "88.50");
        // Integer-valued floats keep their trailing zeros in fixed mode.
        assert_eq!(a.ds.cell(1, "score", a.cell_fmt), "72.00");
    }

    #[test]
    fn key_releases_are_ignored() {
        let mut a = app("release");
        let mut ev = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        ev.kind = KeyEventKind::Release;
        a.on_key(ev);
        assert!(!a.should_quit);
    }
}
