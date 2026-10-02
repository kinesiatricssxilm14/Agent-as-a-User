//! Application state and key dispatch.
//!
//! This module is deliberately terminal-free: `App` can be driven by synthetic
//! key events in tests, and `main` only supplies the event loop and the backend.

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::db::{Affinity, ColumnInfo, Database, ObjectInfo, RowKey, SchemaReport};
use crate::filter::{Filter, MatchMode};
use crate::grid::{Grid, SortDir};
use crate::input::Input;
use crate::value::Value;

/// Which content region is showing on the right-hand side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    /// Table data: grid plus the full record of the selected row.
    Data,
    /// `CREATE` statement, columns, indexes and foreign keys.
    Schema,
    /// Free-form SQL editor and its result set.
    Sql,
    /// Key reference.
    Help,
}

impl Pane {
    pub fn title(self) -> &'static str {
        match self {
            Pane::Data => "Data",
            Pane::Schema => "Schema",
            Pane::Sql => "SQL",
            Pane::Help => "Help",
        }
    }
}

/// Which region consumes navigation keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The table/view list on the left.
    Tables,
    /// The main pane's body: the data grid, the schema text, the help text, or
    /// the SQL result grid.
    Content,
    /// The SQL entry line. Only reachable while [`Pane::Sql`] is showing.
    SqlEditor,
}

impl Focus {
    pub fn label(self) -> &'static str {
        match self {
            Focus::Tables => "tables",
            Focus::Content => "content",
            Focus::SqlEditor => "sql",
        }
    }
}

/// Severity of the message shown in the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Success,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub text: String,
    pub kind: StatusKind,
}

impl Status {
    fn new(text: impl Into<String>, kind: StatusKind) -> Status {
        Status {
            text: text.into(),
            kind,
        }
    }
}

/// State of the column-filter prompt.
#[derive(Debug, Clone)]
pub struct FilterPrompt {
    pub column: usize,
    pub column_name: String,
    pub mode: MatchMode,
    pub case_sensitive: bool,
}

/// State of the cell editor.
#[derive(Debug, Clone)]
pub struct EditPrompt {
    pub object: String,
    pub column: usize,
    pub column_name: String,
    pub affinity: Affinity,
    pub key: RowKey,
    pub original: Value,
    /// Set by Ctrl+N: save SQL NULL rather than the typed text.
    pub set_null: bool,
}

/// The footer prompt currently accepting input, if any.
///
/// Prompts live in the footer rather than in a pop-up so that the grid, the row
/// detail and the status header all stay visible while typing.
#[derive(Debug, Clone)]
pub enum Prompt {
    Filter(FilterPrompt),
    Edit(EditPrompt),
    /// Incremental search over every column of the active grid.
    Search,
    /// Incremental filter over the table list.
    TableFilter,
}

impl Prompt {
    fn discriminant(&self) -> u8 {
        match self {
            Prompt::Filter(_) => 0,
            Prompt::Edit(_) => 1,
            Prompt::Search => 2,
            Prompt::TableFilter => 3,
        }
    }
}

/// The whole browser.
pub struct App {
    pub db: Database,
    /// Every table and view found in the database.
    pub objects: Vec<ObjectInfo>,
    /// Indices into `objects` that pass the table-list filter.
    pub object_view: Vec<usize>,
    pub table_cursor: usize,
    pub table_offset: usize,
    pub table_filter: String,

    /// Rows of the selected table.
    pub grid: Option<Grid>,
    /// Rows returned by the last SQL statement.
    pub query_grid: Option<Grid>,
    pub query_status: Option<Status>,
    pub schema: Option<SchemaReport>,
    pub schema_scroll: u16,
    pub help_scroll: u16,

    pub pane: Pane,
    pub focus: Focus,
    pub status: Status,
    pub prompt: Option<Prompt>,

    pub filter_input: Input,
    pub edit_input: Input,
    pub search_input: Input,
    pub sql_input: Input,
    /// Last successful search needle, for `n` / `N`.
    last_search: String,

    /// Rows of grid viewport, recorded during rendering so paging keys know the
    /// page size. Updated every frame.
    pub grid_height: usize,
    pub table_height: usize,

    pub should_quit: bool,
}

impl App {
    /// Open `path` and load the first table.
    pub fn new(db: Database) -> Result<App> {
        let objects = db.objects().context("cannot list tables")?;
        let mut app = App {
            db,
            object_view: (0..objects.len()).collect(),
            objects,
            table_cursor: 0,
            table_offset: 0,
            table_filter: String::new(),
            grid: None,
            query_grid: None,
            query_status: None,
            schema: None,
            schema_scroll: 0,
            help_scroll: 0,
            pane: Pane::Data,
            focus: Focus::Content,
            status: Status::new("", StatusKind::Info),
            prompt: None,
            filter_input: Input::new(),
            edit_input: Input::new(),
            search_input: Input::new(),
            sql_input: Input::new(),
            last_search: String::new(),
            grid_height: 10,
            table_height: 10,
            should_quit: false,
        };
        if app.objects.is_empty() {
            app.status = Status::new(
                "database contains no tables or views — press 3 to run SQL",
                StatusKind::Warn,
            );
        } else {
            app.select_object(0)?;
            let n = app.objects.len();
            app.status = Status::new(
                format!(
                    "opened {} — {n} object{} found · press ? for help",
                    app.db.path().display(),
                    if n == 1 { "" } else { "s" }
                ),
                StatusKind::Info,
            );
        }
        if app.db.read_only() {
            app.status = Status::new(
                "database opened READ-ONLY — edits are disabled",
                StatusKind::Warn,
            );
        }
        Ok(app)
    }

    // ---- object selection ------------------------------------------------

    /// Name of the table or view under the table-list cursor.
    pub fn current_object(&self) -> Option<&ObjectInfo> {
        self.object_view
            .get(self.table_cursor)
            .and_then(|&i| self.objects.get(i))
    }

    pub fn current_object_name(&self) -> Option<String> {
        self.current_object().map(|o| o.name.clone())
    }

    /// Move the table-list cursor to view position `i` and load that object.
    fn select_object(&mut self, i: usize) -> Result<()> {
        if self.object_view.is_empty() {
            self.grid = None;
            self.schema = None;
            return Ok(());
        }
        self.table_cursor = i.min(self.object_view.len() - 1);
        self.load_current_object()
    }

    /// (Re)load rows and schema for the selected object.
    fn load_current_object(&mut self) -> Result<()> {
        let Some(name) = self.current_object_name() else {
            self.grid = None;
            self.schema = None;
            return Ok(());
        };
        let (columns, rows) = self.db.load_rows(&name)?;
        match &mut self.grid {
            Some(g) if g.source.as_deref() == Some(name.as_str()) => g.reload(columns, rows),
            _ => self.grid = Some(Grid::new(Some(name.clone()), columns, rows)),
        }
        self.schema = Some(self.db.schema(&name)?);
        self.schema_scroll = 0;
        Ok(())
    }

    /// Re-read the object list from `sqlite_master`, keeping the selection.
    pub fn refresh(&mut self) -> Result<()> {
        let selected = self.current_object_name();
        self.objects = self.db.objects()?;
        self.rebuild_object_view();
        if let Some(name) = selected {
            if let Some(pos) = self
                .object_view
                .iter()
                .position(|&i| self.objects[i].name == name)
            {
                self.table_cursor = pos;
            }
        }
        if self.table_cursor >= self.object_view.len() {
            self.table_cursor = self.object_view.len().saturating_sub(1);
        }
        self.load_current_object()
    }

    /// Recompute the visible table list from `table_filter`.
    fn rebuild_object_view(&mut self) {
        let needle = self.table_filter.to_lowercase();
        self.object_view = self
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| needle.is_empty() || o.name.to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect();
        if self.table_cursor >= self.object_view.len() {
            self.table_cursor = self.object_view.len().saturating_sub(1);
        }
    }

    // ---- grid access -----------------------------------------------------

    /// The grid the content pane is currently showing.
    pub fn active_grid(&self) -> Option<&Grid> {
        match self.pane {
            Pane::Sql => self.query_grid.as_ref(),
            _ => self.grid.as_ref(),
        }
    }

    pub fn active_grid_mut(&mut self) -> Option<&mut Grid> {
        match self.pane {
            Pane::Sql => self.query_grid.as_mut(),
            _ => self.grid.as_mut(),
        }
    }

    /// True when the active grid holds an ad-hoc query result (not editable).
    fn active_is_query(&self) -> bool {
        self.pane == Pane::Sql
    }

    pub fn set_status(&mut self, text: impl Into<String>, kind: StatusKind) {
        self.status = Status::new(text, kind);
    }

    fn info(&mut self, text: impl Into<String>) {
        self.set_status(text, StatusKind::Info);
    }

    fn ok(&mut self, text: impl Into<String>) {
        self.set_status(text, StatusKind::Success);
    }

    fn warn(&mut self, text: impl Into<String>) {
        self.set_status(text, StatusKind::Warn);
    }

    fn err(&mut self, text: impl Into<String>) {
        self.set_status(text, StatusKind::Error);
    }

    // ---- key dispatch ----------------------------------------------------

    /// Feed one key event to the app. Errors are turned into status messages so
    /// a failed operation never tears down the UI.
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let result = if self.prompt.is_some() {
            self.on_key_prompt(key)
        } else {
            self.on_key_normal(key)
        };
        if let Err(e) = result {
            self.err(format!("{e:#}"));
        }
    }

    fn on_key_normal(&mut self, key: KeyEvent) -> Result<()> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // Global bindings. In the SQL editor a bare letter is text, so the
        // single-letter globals are only consulted when the editor is not focused.
        let typing = self.focus == Focus::SqlEditor;
        match (key.code, ctrl) {
            (KeyCode::Char('c'), true) => {
                self.should_quit = true;
                return Ok(());
            }
            (KeyCode::Char('q'), false) if !typing => {
                self.should_quit = true;
                return Ok(());
            }
            (KeyCode::F(1), _) => {
                self.toggle_help();
                return Ok(());
            }
            (KeyCode::Char('?'), false) if !typing => {
                self.toggle_help();
                return Ok(());
            }
            (KeyCode::Char('1'), false) if !typing => {
                self.show_pane(Pane::Data);
                return Ok(());
            }
            (KeyCode::Char('2'), false) if !typing => {
                self.show_pane(Pane::Schema);
                return Ok(());
            }
            (KeyCode::Char('3'), false) | (KeyCode::Char(':'), false) if !typing => {
                self.show_pane(Pane::Sql);
                return Ok(());
            }
            (KeyCode::Tab, false) | (KeyCode::BackTab, _) => {
                let back = key.code == KeyCode::BackTab;
                self.cycle_focus(back);
                return Ok(());
            }
            (KeyCode::F(5), _) => {
                self.refresh()?;
                self.ok("reloaded from database");
                return Ok(());
            }
            (KeyCode::Char('r'), false) if !typing => {
                self.refresh()?;
                self.ok("reloaded from database");
                return Ok(());
            }
            _ => {}
        }

        match self.focus {
            Focus::Tables => self.on_key_tables(key),
            Focus::SqlEditor => self.on_key_sql_editor(key),
            Focus::Content => match self.pane {
                Pane::Data | Pane::Sql => self.on_key_grid(key),
                Pane::Schema => {
                    let mut s = self.schema_scroll;
                    self.on_key_scroll(key, &mut s);
                    self.schema_scroll = s;
                    Ok(())
                }
                Pane::Help => {
                    let mut s = self.help_scroll;
                    self.on_key_scroll(key, &mut s);
                    self.help_scroll = s;
                    Ok(())
                }
            },
        }
    }

    fn toggle_help(&mut self) {
        if self.pane == Pane::Help {
            self.pane = Pane::Data;
            self.focus = Focus::Content;
        } else {
            self.pane = Pane::Help;
            self.focus = Focus::Content;
            self.help_scroll = 0;
            self.info("help — j/k or PgUp/PgDn to scroll, Esc or ? to return");
        }
    }

    /// Switch the content pane, putting focus where typing is most likely.
    fn show_pane(&mut self, pane: Pane) {
        self.pane = pane;
        self.focus = match pane {
            Pane::Sql => Focus::SqlEditor,
            _ => Focus::Content,
        };
        match pane {
            Pane::Sql => self.info(
                "SQL: type a statement · Enter runs · Up/Down history · Tab to the result grid",
            ),
            Pane::Schema => self.info("schema view — j/k scrolls, 1 returns to the data"),
            _ => {}
        }
    }

    /// Tab / Shift+Tab through the focusable regions of the current pane.
    fn cycle_focus(&mut self, back: bool) {
        // The SQL pane has three stops; every other pane has two.
        let ring: &[Focus] = if self.pane == Pane::Sql {
            &[Focus::Tables, Focus::SqlEditor, Focus::Content]
        } else {
            &[Focus::Tables, Focus::Content]
        };
        let here = ring.iter().position(|f| *f == self.focus).unwrap_or(0);
        let n = ring.len();
        let next = if back {
            (here + n - 1) % n
        } else {
            (here + 1) % n
        };
        self.focus = ring[next];
    }

    fn on_key_tables(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.table_cursor > 0 {
                    let i = self.table_cursor - 1;
                    self.select_object(i)?;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = self.table_cursor + 1;
                if i < self.object_view.len() {
                    self.select_object(i)?;
                }
            }
            KeyCode::Home | KeyCode::Char('g') => self.select_object(0)?,
            KeyCode::End | KeyCode::Char('G') => {
                let last = self.object_view.len().saturating_sub(1);
                self.select_object(last)?;
            }
            KeyCode::PageUp => {
                let step = self.table_height.max(1);
                let i = self.table_cursor.saturating_sub(step);
                self.select_object(i)?;
            }
            KeyCode::PageDown => {
                let step = self.table_height.max(1);
                let i = (self.table_cursor + step).min(self.object_view.len().saturating_sub(1));
                self.select_object(i)?;
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                self.load_current_object()?;
                self.focus = Focus::Content;
                if self.pane == Pane::Help || self.pane == Pane::Sql {
                    self.pane = Pane::Data;
                }
            }
            KeyCode::Char('/') => {
                self.prompt = Some(Prompt::TableFilter);
                self.search_input.set(self.table_filter.clone());
                self.info("filter table list — Enter keeps it, Esc clears");
            }
            KeyCode::Char('x') | KeyCode::Esc => {
                if !self.table_filter.is_empty() {
                    self.table_filter.clear();
                    self.rebuild_object_view();
                    self.load_current_object()?;
                    self.info("table list filter cleared");
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Vertical scrolling shared by the schema and help panes.
    fn on_key_scroll(&mut self, key: KeyEvent, scroll: &mut u16) {
        let page = self.grid_height.max(1) as u16;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => *scroll = scroll.saturating_add(1),
            KeyCode::PageUp => *scroll = scroll.saturating_sub(page),
            KeyCode::PageDown => *scroll = scroll.saturating_add(page),
            KeyCode::Home | KeyCode::Char('g') => *scroll = 0,
            KeyCode::Esc => {
                self.pane = Pane::Data;
                self.focus = Focus::Content;
            }
            _ => {}
        }
    }

    fn on_key_grid(&mut self, key: KeyEvent) -> Result<()> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let page = self.grid_height.max(1) as isize;

        match (key.code, ctrl) {
            (KeyCode::Up, false) | (KeyCode::Char('k'), false) => self.with_grid(|g| g.move_cursor(-1)),
            (KeyCode::Down, false) | (KeyCode::Char('j'), false) => self.with_grid(|g| g.move_cursor(1)),
            (KeyCode::Left, false) | (KeyCode::Char('h'), false) => self.with_grid(|g| g.move_col(-1)),
            (KeyCode::Right, false) | (KeyCode::Char('l'), false) => self.with_grid(|g| g.move_col(1)),
            (KeyCode::PageUp, _) | (KeyCode::Char('b'), true) => {
                self.with_grid(|g| g.move_cursor(-page))
            }
            (KeyCode::PageDown, _) | (KeyCode::Char('f'), true) => {
                self.with_grid(|g| g.move_cursor(page))
            }
            (KeyCode::Home, false) | (KeyCode::Char('g'), false) => self.with_grid(|g| g.cursor_first()),
            (KeyCode::End, false) | (KeyCode::Char('G'), false) => self.with_grid(|g| g.cursor_last()),
            (KeyCode::Char('^'), false) | (KeyCode::Home, true) => self.with_grid(|g| g.col_first()),
            (KeyCode::Char('$'), false) | (KeyCode::End, true) => self.with_grid(|g| g.col_last()),

            (KeyCode::Char('s'), false) => self.sort_current(Some(SortDir::Asc))?,
            (KeyCode::Char('S'), false) => self.sort_current(Some(SortDir::Desc))?,
            (KeyCode::Char('o'), false) => self.sort_current(None)?,
            (KeyCode::Char('u'), false) => {
                let cleared = self.active_grid_mut().is_some_and(|g| g.clear_sort());
                if cleared {
                    self.info("sort cleared — showing database order");
                } else {
                    self.info("no sort to clear");
                }
            }

            (KeyCode::Char('f'), false) => self.open_filter_prompt()?,
            (KeyCode::Char('x'), false) => self.clear_column_filter(),
            (KeyCode::Char('X'), false) => {
                let n = self.active_grid_mut().map(|g| g.clear_filters()).unwrap_or(0);
                if n == 0 {
                    self.info("no filters active");
                } else {
                    let total = self.active_grid().map(|g| g.total_rows()).unwrap_or(0);
                    self.ok(format!(
                        "cleared {n} filter{} — showing all {total} rows",
                        if n == 1 { "" } else { "s" }
                    ));
                }
            }

            (KeyCode::Char('/'), false) => {
                self.prompt = Some(Prompt::Search);
                self.search_input.clear();
                self.info("search all columns — Enter keeps position, Esc cancels");
            }
            (KeyCode::Char('n'), false) => self.repeat_search(1),
            (KeyCode::Char('N'), false) => self.repeat_search(-1),

            (KeyCode::Enter, _) | (KeyCode::Char('e'), false) => self.open_cell_editor()?,
            (KeyCode::Esc, _) => {
                if self.pane == Pane::Sql {
                    // Back to the entry line rather than out of the pane, so the
                    // statement being composed is not lost.
                    self.focus = Focus::SqlEditor;
                } else {
                    self.focus = Focus::Tables;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle typing on the SQL entry line.
    fn on_key_sql_editor(&mut self, key: KeyEvent) -> Result<()> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match (key.code, ctrl) {
            (KeyCode::Char(c), false) => self.sql_input.insert(c),
            (KeyCode::Backspace, _) => self.sql_input.backspace(),
            (KeyCode::Delete, _) => self.sql_input.delete(),
            (KeyCode::Char('w'), true) => self.sql_input.delete_word(),
            (KeyCode::Char('u'), true) => self.sql_input.kill_to_start(),
            (KeyCode::Char('k'), true) => self.sql_input.kill_to_end(),
            (KeyCode::Left, false) => self.sql_input.left(),
            (KeyCode::Right, false) => self.sql_input.right(),
            (KeyCode::Left, true) => self.sql_input.word_left(),
            (KeyCode::Right, true) => self.sql_input.word_right(),
            (KeyCode::Home, _) => self.sql_input.home(),
            (KeyCode::End, _) => self.sql_input.end(),
            (KeyCode::Up, _) => self.sql_input.history_prev(),
            (KeyCode::Down, _) => self.sql_input.history_next(),
            (KeyCode::Enter, _) => self.run_sql(),
            (KeyCode::Esc, _) => {
                // Leave the SQL pane; the statement text is kept for next time.
                self.pane = Pane::Data;
                self.focus = Focus::Content;
                self.info("back to the data view");
            }
            _ => {}
        }
        Ok(())
    }

    fn with_grid(&mut self, f: impl FnOnce(&mut Grid)) {
        if let Some(g) = self.active_grid_mut() {
            f(g);
        }
    }

    // ---- sorting ---------------------------------------------------------

    /// Sort the active grid by the selected column. `None` toggles direction.
    fn sort_current(&mut self, dir: Option<SortDir>) -> Result<()> {
        let Some(g) = self.active_grid_mut() else {
            return Ok(());
        };
        let Some(col) = g.current_column().map(|c| c.name.clone()) else {
            return Ok(());
        };
        let idx = g.col_cursor;
        let applied = match dir {
            Some(d) => {
                g.sort_by(idx, d);
                d
            }
            None => g.toggle_sort(idx),
        };
        let extreme = g
            .current_row()
            .and_then(|r| r.cells.get(idx))
            .map(|v| v.display())
            .unwrap_or_else(|| "—".into());
        self.ok(format!(
            "sorted by {col} {} — top row {col} = {extreme}",
            applied.label()
        ));
        Ok(())
    }

    // ---- filtering -------------------------------------------------------

    fn open_filter_prompt(&mut self) -> Result<()> {
        let Some(g) = self.active_grid() else {
            self.warn("no data to filter");
            return Ok(());
        };
        let Some(col) = g.current_column() else {
            self.warn("no column selected");
            return Ok(());
        };
        let column = g.col_cursor;
        let column_name = col.name.clone();
        // Re-opening a filtered column pre-loads the existing filter so it can
        // be tweaked rather than retyped.
        let existing = g.filter_on(column).cloned();
        let (mode, case_sensitive, text) = match existing {
            Some(f) => (f.mode, f.case_sensitive(), f.needle.clone()),
            None => (MatchMode::Contains, false, String::new()),
        };
        self.filter_input.set(text);
        self.prompt = Some(Prompt::Filter(FilterPrompt {
            column,
            column_name: column_name.clone(),
            mode,
            case_sensitive,
        }));
        self.info(format!("filter column {column_name}"));
        Ok(())
    }

    fn clear_column_filter(&mut self) {
        let Some(g) = self.active_grid_mut() else { return };
        let col = g.col_cursor;
        let name = g
            .current_column()
            .map(|c| c.name.clone())
            .unwrap_or_default();
        if g.clear_filter_on(col) {
            let (shown, total) = (g.visible_rows(), g.total_rows());
            self.ok(format!(
                "filter on {name} cleared — {shown} of {total} rows shown"
            ));
        } else {
            self.info(format!("no filter on {name}"));
        }
    }

    /// Build the filter described by the prompt and the typed needle.
    fn build_filter(&self, p: &FilterPrompt) -> Result<Filter> {
        Filter::new(
            p.column,
            p.column_name.clone(),
            p.mode,
            self.filter_input.text(),
            p.case_sensitive,
        )
    }

    /// How many rows the pending filter would keep — shown live in the prompt.
    pub fn filter_preview(&self) -> Option<Result<usize>> {
        let Some(Prompt::Filter(p)) = &self.prompt else {
            return None;
        };
        let g = self.active_grid()?;
        Some(self.build_filter(p).map(|f| {
            g.iter_view()
                .filter(|r| r.cells.get(f.column).is_some_and(|c| f.matches(c)))
                .count()
        }))
    }

    // ---- search ----------------------------------------------------------

    /// Move to the next row (in `dir`) whose any column matches `needle`.
    fn search_from(&mut self, needle: &str, dir: isize, from_current: bool) -> Option<usize> {
        let g = self.active_grid()?;
        if needle.is_empty() || g.is_empty() {
            return None;
        }
        let n = g.visible_rows();
        let low = needle.to_lowercase();
        let start = g.cursor();
        let offset = usize::from(!from_current);
        for step in 0..n {
            let idx = if dir >= 0 {
                (start + offset + step) % n
            } else {
                (start + n - offset - step) % n
            };
            let row = g.row_at(idx)?;
            if row
                .cells
                .iter()
                .any(|c| c.filter_text().to_lowercase().contains(&low))
            {
                return Some(idx);
            }
        }
        None
    }

    /// Live search as the needle is typed: jump to the first match at or after
    /// the current row.
    fn search_incremental(&mut self) {
        let needle = self.search_input.text().to_string();
        if needle.is_empty() {
            return;
        }
        match self.search_from(&needle, 1, true) {
            Some(i) => {
                self.with_grid(|g| g.cursor_to(i));
                self.info(format!("match at row {}", i + 1));
            }
            None => self.warn(format!("no row matching \"{needle}\"")),
        }
    }

    fn repeat_search(&mut self, dir: isize) {
        let needle = self.last_search.clone();
        if needle.is_empty() {
            self.info("no previous search — press / to search");
            return;
        }
        match self.search_from(&needle, dir, false) {
            Some(i) => {
                self.with_grid(|g| g.cursor_to(i));
                self.info(format!("\"{needle}\" — row {}", i + 1));
            }
            None => self.warn(format!("no other row matching \"{needle}\"")),
        }
    }

    // ---- cell editing ----------------------------------------------------

    fn open_cell_editor(&mut self) -> Result<()> {
        if self.active_is_query() {
            self.warn("query results are read-only — open the table (1) to edit");
            return Ok(());
        }
        if self.db.read_only() {
            self.warn("database is open read-only — cannot edit");
            return Ok(());
        }
        let Some(object) = self.current_object_name() else {
            return Ok(());
        };
        if self
            .current_object()
            .is_some_and(|o| o.kind == crate::db::ObjectKind::View)
        {
            self.warn("views cannot be edited — select the underlying table");
            return Ok(());
        }
        let Some(g) = self.grid.as_ref() else {
            return Ok(());
        };
        let Some(row) = g.current_row() else {
            self.warn("no row selected");
            return Ok(());
        };
        if row.key == RowKey::None {
            self.warn("this row has no rowid or primary key — cannot edit safely");
            return Ok(());
        }
        let Some(col) = g.current_column() else {
            return Ok(());
        };
        let original = row
            .cells
            .get(g.col_cursor)
            .cloned()
            .unwrap_or(Value::Null);
        if original.is_blob() {
            self.warn("blob values cannot be edited in the cell editor");
            return Ok(());
        }
        let prompt = EditPrompt {
            object,
            column: g.col_cursor,
            column_name: col.name.clone(),
            affinity: col.affinity(),
            key: row.key.clone(),
            original: original.clone(),
            set_null: false,
        };
        self.edit_input.set(original.edit_text());
        self.info(format!(
            "editing {} — Enter saves, Esc cancels, Ctrl+N sets NULL",
            prompt.column_name
        ));
        self.prompt = Some(Prompt::Edit(prompt));
        Ok(())
    }

    /// Write the edited cell to the database and refresh the row in place.
    fn commit_edit(&mut self, p: EditPrompt) -> Result<()> {
        let value = if p.set_null {
            Value::Null
        } else {
            coerce(self.edit_input.text(), p.affinity)
        };
        if value == p.original {
            self.info(format!("{} unchanged", p.column_name));
            return Ok(());
        }
        let cells = self
            .db
            .update_cell(&p.object, &p.key, &p.column_name, &value)?;
        let shown = cells
            .get(p.column)
            .cloned()
            .unwrap_or(Value::Null);
        if let Some(g) = self.grid.as_mut() {
            g.apply_row_update(cells)?;
        }
        // Row counts do not change on UPDATE, but the object list caches them;
        // leaving them alone keeps the write cheap.
        self.ok(format!(
            "saved {}.{} = {} ({})",
            p.object,
            p.column_name,
            shown.display(),
            shown.type_name()
        ));
        Ok(())
    }

    // ---- SQL -------------------------------------------------------------

    fn run_sql(&mut self) {
        let sql = self.sql_input.text().trim().to_string();
        if sql.is_empty() {
            self.warn("nothing to run");
            return;
        }
        match self.db.execute_sql(&sql) {
            Ok(result) => {
                self.sql_input.push_history();
                if let Some(changes) = result.changes {
                    self.query_grid = None;
                    let msg = format!("statement ok — {changes} row(s) affected");
                    self.query_status = Some(Status::new(msg.clone(), StatusKind::Success));
                    self.ok(msg);
                    // DDL/DML may have changed the schema or the current table.
                    if let Err(e) = self.refresh() {
                        self.err(format!("reload after statement failed: {e:#}"));
                    }
                } else {
                    let n = result.rows.len();
                    let columns: Vec<ColumnInfo> = result
                        .columns
                        .iter()
                        .map(|name| ColumnInfo {
                            name: name.clone(),
                            decl_type: String::new(),
                            not_null: false,
                            default: None,
                            pk_index: 0,
                        })
                        .collect();
                    self.query_grid = Some(Grid::new(None, columns, result.rows));
                    let msg = format!("{n} row(s) returned");
                    self.query_status = Some(Status::new(msg.clone(), StatusKind::Success));
                    self.ok(msg);
                }
            }
            Err(e) => {
                let msg = format!("{e:#}");
                self.query_status = Some(Status::new(msg.clone(), StatusKind::Error));
                self.err(msg);
            }
        }
    }

    // ---- prompt handling -------------------------------------------------

    fn on_key_prompt(&mut self, key: KeyEvent) -> Result<()> {
        let Some(prompt) = self.prompt.clone() else {
            return Ok(());
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // Keys that mean the same thing in every prompt.
        match (key.code, ctrl) {
            (KeyCode::Esc, _) | (KeyCode::Char('c'), true) => {
                self.cancel_prompt(&prompt)?;
                return Ok(());
            }
            (KeyCode::Enter, _) => {
                self.prompt = None;
                match prompt {
                    Prompt::Filter(p) => {
                        let filter = self.build_filter(&p)?;
                        let summary = filter.summary();
                        if let Some(g) = self.active_grid_mut() {
                            g.set_filter(filter);
                            let (shown, total) = (g.visible_rows(), g.total_rows());
                            self.ok(format!(
                                "filter {summary} — {shown} of {total} rows match"
                            ));
                        }
                        self.filter_input.push_history();
                    }
                    Prompt::Edit(p) => self.commit_edit(p)?,
                    Prompt::Search => {
                        self.last_search = self.search_input.text().to_string();
                        self.search_input.push_history();
                        if !self.last_search.is_empty() {
                            self.info(format!(
                                "search \"{}\" — n next, N previous",
                                self.last_search
                            ));
                        }
                    }
                    Prompt::TableFilter => {
                        self.info(format!(
                            "{} object(s) listed",
                            self.object_view.len()
                        ));
                    }
                }
                return Ok(());
            }
            _ => {}
        }

        // Prompt-specific bindings.
        match (&prompt, key.code, ctrl) {
            (Prompt::Filter(p), KeyCode::Tab, _) => {
                let mut p = p.clone();
                p.mode = p.mode.next();
                let mode = p.mode;
                self.prompt = Some(Prompt::Filter(p));
                self.info(format!("match mode: {}", mode.label()));
                return Ok(());
            }
            (Prompt::Filter(p), KeyCode::Char('a'), true) => {
                let mut p = p.clone();
                p.case_sensitive = !p.case_sensitive;
                let cs = p.case_sensitive;
                self.prompt = Some(Prompt::Filter(p));
                self.info(if cs {
                    "case sensitive"
                } else {
                    "case insensitive"
                });
                return Ok(());
            }
            (Prompt::Edit(p), KeyCode::Char('n'), true) => {
                let mut p = p.clone();
                p.set_null = !p.set_null;
                let null = p.set_null;
                self.prompt = Some(Prompt::Edit(p));
                self.info(if null {
                    "will save SQL NULL — Ctrl+N again to type a value"
                } else {
                    "will save the typed text"
                });
                return Ok(());
            }
            (Prompt::Edit(p), KeyCode::Char('r'), true) => {
                let text = p.original.edit_text();
                self.edit_input.set(text);
                let mut p = p.clone();
                p.set_null = false;
                self.prompt = Some(Prompt::Edit(p));
                self.info("restored original value");
                return Ok(());
            }
            _ => {}
        }

        // Everything else edits the prompt's text buffer.
        let disc = prompt.discriminant();
        let input = match disc {
            0 => &mut self.filter_input,
            1 => &mut self.edit_input,
            _ => &mut self.search_input,
        };
        let mut text_changed = false;
        match (key.code, ctrl) {
            (KeyCode::Char(c), false) => {
                input.insert(c);
                text_changed = true;
            }
            (KeyCode::Backspace, _) => {
                input.backspace();
                text_changed = true;
            }
            (KeyCode::Delete, _) => {
                input.delete();
                text_changed = true;
            }
            (KeyCode::Char('w'), true) => {
                input.delete_word();
                text_changed = true;
            }
            (KeyCode::Char('u'), true) => {
                input.kill_to_start();
                text_changed = true;
            }
            (KeyCode::Char('k'), true) => {
                input.kill_to_end();
                text_changed = true;
            }
            (KeyCode::Left, false) => input.left(),
            (KeyCode::Right, false) => input.right(),
            (KeyCode::Left, true) => input.word_left(),
            (KeyCode::Right, true) => input.word_right(),
            (KeyCode::Home, _) => input.home(),
            (KeyCode::End, _) => input.end(),
            (KeyCode::Up, _) => input.history_prev(),
            (KeyCode::Down, _) => input.history_next(),
            _ => {}
        }

        // A typed character in an edit prompt cancels a pending NULL, and a
        // typed character in a search/table-filter prompt re-runs it live.
        if text_changed {
            match &prompt {
                Prompt::Edit(p) if p.set_null => {
                    let mut p = p.clone();
                    p.set_null = false;
                    self.prompt = Some(Prompt::Edit(p));
                }
                Prompt::Search => self.search_incremental(),
                Prompt::TableFilter => {
                    self.table_filter = self.search_input.text().to_string();
                    self.rebuild_object_view();
                    self.load_current_object()?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn cancel_prompt(&mut self, prompt: &Prompt) -> Result<()> {
        self.prompt = None;
        match prompt {
            Prompt::TableFilter => {
                self.table_filter.clear();
                self.rebuild_object_view();
                self.load_current_object()?;
                self.info("table filter cancelled");
            }
            Prompt::Edit(p) => self.info(format!("edit of {} cancelled", p.column_name)),
            Prompt::Filter(_) => self.info("filter cancelled"),
            Prompt::Search => self.info("search cancelled"),
        }
        Ok(())
    }
}

/// Coerce edited text the way SQLite would for a column of this affinity, so the
/// value stored matches what the user would get from a hand-written `UPDATE`.
pub fn coerce(text: &str, affinity: Affinity) -> Value {
    let t = text.trim();
    match affinity {
        Affinity::Text => Value::Text(text.to_string()),
        Affinity::Blob => {
            // No declared type: store text unless it is cleanly numeric.
            Value::Text(text.to_string())
        }
        Affinity::Integer => {
            if let Ok(i) = t.parse::<i64>() {
                Value::Int(i)
            } else if let Ok(f) = t.parse::<f64>() {
                // Lossless float → integer, matching SQLite's INTEGER affinity.
                if f.fract() == 0.0 && f.abs() < 9.223_372_036_854_776e18 {
                    Value::Int(f as i64)
                } else {
                    Value::Real(f)
                }
            } else {
                Value::Text(text.to_string())
            }
        }
        Affinity::Real => match t.parse::<f64>() {
            Ok(f) => Value::Real(f),
            Err(_) => Value::Text(text.to_string()),
        },
        Affinity::Numeric => {
            if let Ok(i) = t.parse::<i64>() {
                Value::Int(i)
            } else if let Ok(f) = t.parse::<f64>() {
                Value::Real(f)
            } else {
                Value::Text(text.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_affinity_prefers_integers() {
        assert_eq!(coerce("42", Affinity::Integer), Value::Int(42));
        assert_eq!(coerce(" 42 ", Affinity::Integer), Value::Int(42));
        assert_eq!(coerce("42.0", Affinity::Integer), Value::Int(42));
        assert_eq!(coerce("42.5", Affinity::Integer), Value::Real(42.5));
        assert_eq!(
            coerce("forty", Affinity::Integer),
            Value::Text("forty".into())
        );
    }

    #[test]
    fn real_affinity_always_stores_a_float_when_numeric() {
        assert_eq!(coerce("42", Affinity::Real), Value::Real(42.0));
        assert_eq!(coerce("1e3", Affinity::Real), Value::Real(1000.0));
        assert_eq!(coerce("x", Affinity::Real), Value::Text("x".into()));
    }

    #[test]
    fn text_affinity_preserves_input_verbatim() {
        assert_eq!(coerce("  42  ", Affinity::Text), Value::Text("  42  ".into()));
        assert_eq!(coerce("", Affinity::Text), Value::Text(String::new()));
    }

    #[test]
    fn numeric_affinity_narrows_to_int_then_real() {
        assert_eq!(coerce("7", Affinity::Numeric), Value::Int(7));
        assert_eq!(coerce("7.5", Affinity::Numeric), Value::Real(7.5));
        assert_eq!(coerce("n/a", Affinity::Numeric), Value::Text("n/a".into()));
    }
}
