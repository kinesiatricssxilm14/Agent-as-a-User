use std::{cmp::min, env, io, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState,
        Wrap,
    },
    Frame, Terminal,
};
use regex::Regex;
use rusqlite::{
    types::{Value, ValueRef},
    Connection,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const DEFAULT_DB: &str = "/bench/data/bench.db";
#[derive(Clone, Debug)]
struct ColumnInfo {
    name: String,
    declared_type: String,
    pk_order: i64,
}

#[derive(Clone, Debug)]
struct TableInfo {
    name: String,
    schema: String,
    columns: Vec<ColumnInfo>,
    rowid_expression: Option<&'static str>,
}

#[derive(Clone, Debug, Default)]
struct DataSet {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    typed_rows: Vec<Vec<Value>>,
    identities: Vec<RowIdentity>,
    truncated: bool,
}

#[derive(Clone, Debug, Default)]
struct RowIdentity {
    rowid: Option<i64>,
    primary_key: Vec<(String, Value)>,
}

#[derive(Clone, Debug)]
struct Filter {
    column: usize,
    mode: FilterMode,
    pattern: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilterMode {
    Contains,
    Exact,
    Regex,
}

impl FilterMode {
    fn label(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Exact => "exact",
            Self::Regex => "regex",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Contains => Self::Exact,
            Self::Exact => Self::Regex,
            Self::Regex => Self::Contains,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    fn sql(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Tables,
    Data,
    Details,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Tables => Self::Data,
            Self::Data => Self::Details,
            Self::Details => Self::Tables,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Tables => Self::Details,
            Self::Data => Self::Tables,
            Self::Details => Self::Data,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DetailTab {
    Record,
    Schema,
}

#[derive(Clone, Debug)]
enum Mode {
    Normal,
    Help {
        scroll: u16,
    },
    SqlInput {
        text: String,
        cursor: usize,
    },
    FilterColumn {
        selected: usize,
    },
    ChooseMatch {
        column: usize,
        mode: FilterMode,
    },
    FilterInput {
        column: usize,
        mode: FilterMode,
        text: String,
        cursor: usize,
    },
    SortColumn {
        selected: usize,
    },
    SortDirection {
        column: usize,
    },
    EditColumn {
        selected: usize,
    },
    EditValue {
        column: usize,
        text: String,
        cursor: usize,
    },
    ConfirmWriteSql {
        sql: String,
    },
}

struct App {
    conn: Connection,
    db_path: PathBuf,
    tables: Vec<TableInfo>,
    table_state: ListState,
    selected_table: Option<usize>,
    data: DataSet,
    data_state: TableState,
    focus: Focus,
    detail_tab: DetailTab,
    horizontal_offset: u16,
    detail_scroll: u16,
    filter: Option<Filter>,
    sort: Option<(usize, SortDirection)>,
    query_label: Option<String>,
    mode: Mode,
    status: String,
    error: bool,
}

fn main() -> Result<()> {
    let path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DB));
    let conn = Connection::open(&path)
        .with_context(|| format!("could not open SQLite database {}", path.display()))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    let mut app = App::new(conn, path)?;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(error.into());
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => {
            let _ = disable_raw_mode();
            let mut stdout = io::stdout();
            let _ = execute!(stdout, LeaveAlternateScreen);
            return Err(error.into());
        }
    };

    let result = run(&mut terminal, &mut app);
    let raw_result = disable_raw_mode();
    let screen_result = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let cursor_result = terminal.show_cursor();
    result?;
    raw_result?;
    screen_result?;
    cursor_result?;
    Ok(())
}

fn run<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == crossterm::event::KeyEventKind::Press && app.handle_key(key)? {
                    break;
                }
            }
        }
    }
    Ok(())
}

impl App {
    fn new(conn: Connection, db_path: PathBuf) -> Result<Self> {
        let mut app = Self {
            conn,
            db_path,
            tables: Vec::new(),
            table_state: ListState::default(),
            selected_table: None,
            data: DataSet::default(),
            data_state: TableState::default(),
            focus: Focus::Tables,
            detail_tab: DetailTab::Record,
            horizontal_offset: 0,
            detail_scroll: 0,
            filter: None,
            sort: None,
            query_label: None,
            mode: Mode::Normal,
            status: String::new(),
            error: false,
        };
        app.reload_tables()?;
        if !app.tables.is_empty() {
            app.table_state.select(Some(0));
            app.open_selected_table()?;
        } else {
            app.set_status("Database has no user tables", false);
        }
        Ok(app)
    }

    fn set_status(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.error = error;
    }

    fn reload_tables(&mut self) -> Result<()> {
        let old_name = self
            .selected_table
            .and_then(|i| self.tables.get(i))
            .map(|t| t.name.clone());
        let mut stmt = self.conn.prepare(
            "SELECT name, sql FROM sqlite_schema \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name COLLATE NOCASE",
        )?;
        let raw = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        self.tables.clear();
        for (name, schema) in raw {
            let columns = self.load_columns(&name)?;
            let without_rowid = schema.to_ascii_uppercase().contains("WITHOUT ROWID");
            let rowid_expression = if without_rowid {
                None
            } else {
                ["rowid", "_rowid_", "oid"].into_iter().find(|candidate| {
                    !columns
                        .iter()
                        .any(|column| column.name.eq_ignore_ascii_case(candidate))
                })
            };
            self.tables.push(TableInfo {
                name,
                rowid_expression,
                schema,
                columns,
            });
        }
        let selected = old_name
            .and_then(|name| self.tables.iter().position(|t| t.name == name))
            .or_else(|| (!self.tables.is_empty()).then_some(0));
        self.table_state.select(selected);
        self.selected_table = selected;
        Ok(())
    }

    fn load_columns(&self, table: &str) -> Result<Vec<ColumnInfo>> {
        let sql = format!("PRAGMA table_info({})", quote_ident(table));
        let mut stmt = self.conn.prepare(&sql)?;
        let columns = stmt
            .query_map([], |row| {
                Ok(ColumnInfo {
                    name: row.get(1)?,
                    declared_type: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    pk_order: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(columns)
    }

    fn open_selected_table(&mut self) -> Result<()> {
        let Some(index) = self.table_state.selected() else {
            return Ok(());
        };
        self.selected_table = Some(index);
        self.filter = None;
        self.sort = None;
        self.query_label = None;
        self.horizontal_offset = 0;
        self.detail_scroll = 0;
        self.load_current_table()?;
        self.focus = Focus::Data;
        Ok(())
    }

    fn load_current_table(&mut self) -> Result<()> {
        let table = self.current_table().context("no table selected")?.clone();
        if table.columns.is_empty() {
            self.data = DataSet::default();
            self.data_state.select(None);
            self.set_status(format!("Table {} has no visible columns", table.name), true);
            return Ok(());
        }
        let mut sql = String::new();
        let can_rowid = table.rowid_expression.is_some();
        if let Some(rowid_expression) = table.rowid_expression {
            sql.push_str("SELECT ");
            sql.push_str(rowid_expression);
            sql.push_str(" AS \"__toolo_rowid__\", * FROM ");
        } else {
            sql.push_str("SELECT * FROM ");
        }
        sql.push_str(&quote_ident(&table.name));
        if let Some((column, direction)) = self.sort {
            if let Some(info) = table.columns.get(column) {
                sql.push_str(" ORDER BY ");
                sql.push_str(&quote_ident(&info.name));
                sql.push(' ');
                sql.push_str(direction.sql());
            }
        }
        let mut raw = query_dataset(&self.conn, &sql, can_rowid)?;
        if !can_rowid {
            let primary_keys = table
                .columns
                .iter()
                .enumerate()
                .filter(|(_, column)| column.pk_order > 0)
                .collect::<Vec<_>>();
            for (row, identity) in raw.typed_rows.iter().zip(raw.identities.iter_mut()) {
                identity.primary_key = primary_keys
                    .iter()
                    .filter_map(|(index, column)| {
                        row.get(*index)
                            .map(|value| (column.name.clone(), value.clone()))
                    })
                    .collect();
            }
        }
        self.data = self.apply_filter(raw)?;
        self.data_state
            .select((!self.data.rows.is_empty()).then_some(0));
        let filter_text = self
            .filter
            .as_ref()
            .map(|_| format!(", {} matches", self.data.rows.len()))
            .unwrap_or_default();
        let trunc = if self.data.truncated {
            ", result truncated".to_string()
        } else {
            String::new()
        };
        self.set_status(
            format!("Loaded {}{}{}", table.name, filter_text, trunc),
            false,
        );
        Ok(())
    }

    fn apply_filter(&self, mut data: DataSet) -> Result<DataSet> {
        let Some(filter) = &self.filter else {
            return Ok(data);
        };
        let regex = if filter.mode == FilterMode::Regex {
            Some(Regex::new(&filter.pattern).context("invalid regular expression")?)
        } else {
            None
        };
        let mut rows = Vec::new();
        let mut typed_rows = Vec::new();
        let mut identities = Vec::new();
        for ((row, typed_row), identity) in data
            .rows
            .into_iter()
            .zip(data.typed_rows)
            .zip(data.identities)
        {
            let value = row.get(filter.column).map(String::as_str).unwrap_or("");
            let matched = match filter.mode {
                FilterMode::Contains => value
                    .to_lowercase()
                    .contains(&filter.pattern.to_lowercase()),
                FilterMode::Exact => value == filter.pattern,
                FilterMode::Regex => regex.as_ref().is_some_and(|r| r.is_match(value)),
            };
            if matched {
                rows.push(row);
                typed_rows.push(typed_row);
                identities.push(identity);
            }
        }
        data.rows = rows;
        data.typed_rows = typed_rows;
        data.identities = identities;
        Ok(data)
    }

    fn current_table(&self) -> Option<&TableInfo> {
        self.selected_table.and_then(|i| self.tables.get(i))
    }

    fn selected_row(&self) -> Option<usize> {
        self.data_state
            .selected()
            .filter(|i| *i < self.data.rows.len())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(true);
        }
        let mode = std::mem::replace(&mut self.mode, Mode::Normal);
        match mode {
            Mode::Normal => return self.handle_normal(key),
            Mode::Help { mut scroll } => match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {}
                KeyCode::Down | KeyCode::Char('j') => {
                    scroll = scroll.saturating_add(1);
                    self.mode = Mode::Help { scroll };
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    scroll = scroll.saturating_sub(1);
                    self.mode = Mode::Help { scroll };
                }
                KeyCode::PageDown => {
                    scroll = scroll.saturating_add(10);
                    self.mode = Mode::Help { scroll };
                }
                KeyCode::PageUp => {
                    scroll = scroll.saturating_sub(10);
                    self.mode = Mode::Help { scroll };
                }
                _ => self.mode = Mode::Help { scroll },
            },
            Mode::SqlInput {
                mut text,
                mut cursor,
            } => match edit_text_key(key, &mut text, &mut cursor) {
                InputAction::Continue => self.mode = Mode::SqlInput { text, cursor },
                InputAction::Cancel => {}
                InputAction::Submit => self.execute_sql(text)?,
            },
            Mode::FilterColumn { mut selected } => {
                if self.data.columns.is_empty() {
                    return Ok(false);
                }
                match key.code {
                    KeyCode::Esc => {}
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected = selected.saturating_sub(1);
                        self.mode = Mode::FilterColumn { selected };
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = min(selected + 1, self.data.columns.len() - 1);
                        self.mode = Mode::FilterColumn { selected };
                    }
                    KeyCode::Enter => {
                        self.mode = Mode::ChooseMatch {
                            column: selected,
                            mode: FilterMode::Contains,
                        }
                    }
                    _ => self.mode = Mode::FilterColumn { selected },
                }
            }
            Mode::ChooseMatch { column, mut mode } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                    mode = mode.next();
                    self.mode = Mode::ChooseMatch { column, mode };
                }
                KeyCode::Char('1') => {
                    self.mode = Mode::FilterInput {
                        column,
                        mode: FilterMode::Contains,
                        text: String::new(),
                        cursor: 0,
                    }
                }
                KeyCode::Char('2') => {
                    self.mode = Mode::FilterInput {
                        column,
                        mode: FilterMode::Exact,
                        text: String::new(),
                        cursor: 0,
                    }
                }
                KeyCode::Char('3') => {
                    self.mode = Mode::FilterInput {
                        column,
                        mode: FilterMode::Regex,
                        text: String::new(),
                        cursor: 0,
                    }
                }
                KeyCode::Enter => {
                    self.mode = Mode::FilterInput {
                        column,
                        mode,
                        text: String::new(),
                        cursor: 0,
                    }
                }
                _ => self.mode = Mode::ChooseMatch { column, mode },
            },
            Mode::FilterInput {
                column,
                mode,
                mut text,
                mut cursor,
            } => match edit_text_key(key, &mut text, &mut cursor) {
                InputAction::Continue => {
                    self.mode = Mode::FilterInput {
                        column,
                        mode,
                        text,
                        cursor,
                    }
                }
                InputAction::Cancel => {}
                InputAction::Submit => {
                    self.filter = Some(Filter {
                        column,
                        mode,
                        pattern: text,
                    });
                    if let Err(err) = self.load_current_table() {
                        self.filter = None;
                        self.set_status(format!("Filter error: {err:#}"), true);
                    }
                }
            },
            Mode::SortColumn { mut selected } => {
                if self.data.columns.is_empty() {
                    return Ok(false);
                }
                match key.code {
                    KeyCode::Esc => {}
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected = selected.saturating_sub(1);
                        self.mode = Mode::SortColumn { selected };
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = min(selected + 1, self.data.columns.len() - 1);
                        self.mode = Mode::SortColumn { selected };
                    }
                    KeyCode::Enter => self.mode = Mode::SortDirection { column: selected },
                    _ => self.mode = Mode::SortColumn { selected },
                }
            }
            Mode::SortDirection { column } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Char('a') | KeyCode::Left | KeyCode::Enter => {
                    self.set_sort(column, SortDirection::Asc)?
                }
                KeyCode::Char('d') | KeyCode::Right => {
                    self.set_sort(column, SortDirection::Desc)?
                }
                _ => self.mode = Mode::SortDirection { column },
            },
            Mode::EditColumn { mut selected } => {
                if self.data.columns.is_empty() {
                    return Ok(false);
                }
                match key.code {
                    KeyCode::Esc => {}
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected = selected.saturating_sub(1);
                        self.mode = Mode::EditColumn { selected };
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = min(selected + 1, self.data.columns.len() - 1);
                        self.mode = Mode::EditColumn { selected };
                    }
                    KeyCode::Enter => {
                        let value = self
                            .selected_row()
                            .and_then(|r| self.data.rows.get(r))
                            .and_then(|r| r.get(selected))
                            .cloned()
                            .unwrap_or_default();
                        let cursor = value.chars().count();
                        self.mode = Mode::EditValue {
                            column: selected,
                            text: value,
                            cursor,
                        };
                    }
                    _ => self.mode = Mode::EditColumn { selected },
                }
            }
            Mode::EditValue {
                column,
                mut text,
                mut cursor,
            } => match edit_text_key(key, &mut text, &mut cursor) {
                InputAction::Continue => {
                    self.mode = Mode::EditValue {
                        column,
                        text,
                        cursor,
                    }
                }
                InputAction::Cancel => {}
                InputAction::Submit => self.save_edit(column, text)?,
            },
            Mode::ConfirmWriteSql { sql } => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => self.execute_write_sql(&sql)?,
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.set_status("SQL write cancelled", false)
                }
                _ => self.mode = Mode::ConfirmWriteSql { sql },
            },
        }
        Ok(false)
    }

    fn handle_normal(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('Q') => return Ok(true),
            KeyCode::Char('?') => self.mode = Mode::Help { scroll: 0 },
            KeyCode::Tab => self.focus = self.focus.next(),
            KeyCode::BackTab => self.focus = self.focus.previous(),
            KeyCode::Char('q') => {
                self.mode = Mode::SqlInput {
                    text: String::new(),
                    cursor: 0,
                }
            }
            KeyCode::Char('/') => {
                if self.query_label.is_none() && !self.data.columns.is_empty() {
                    self.mode = Mode::FilterColumn { selected: 0 };
                } else {
                    self.set_status("Filtering is available in table views", true);
                }
            }
            KeyCode::Char('s') => {
                if self.query_label.is_none() && !self.data.columns.is_empty() {
                    self.mode = Mode::SortColumn { selected: 0 };
                } else {
                    self.set_status("Sorting is available in table views", true);
                }
            }
            KeyCode::Char('e') => {
                if self.query_label.is_none()
                    && self.selected_row().is_some()
                    && !self.data.columns.is_empty()
                {
                    self.mode = Mode::EditColumn { selected: 0 };
                } else {
                    self.set_status("Select a table row before editing", true);
                }
            }
            KeyCode::Char('x') => {
                self.filter = None;
                self.sort = None;
                if self.query_label.is_none() {
                    self.load_current_table()?;
                }
            }
            KeyCode::Char('r') => {
                self.reload_tables()?;
                if self.selected_table.is_some() {
                    self.load_current_table()?;
                }
                self.set_status("Reloaded database", false);
            }
            KeyCode::Char('v') => {
                self.detail_tab = if self.detail_tab == DetailTab::Record {
                    DetailTab::Schema
                } else {
                    DetailTab::Record
                }
            }
            KeyCode::Enter if self.focus == Focus::Tables => self.open_selected_table()?,
            KeyCode::Down | KeyCode::Char('j') => self.move_down(),
            KeyCode::Up | KeyCode::Char('k') => self.move_up(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::Home => self.move_home(),
            KeyCode::End => self.move_end(),
            KeyCode::Left | KeyCode::Char('h') if self.focus == Focus::Data => {
                self.horizontal_offset = self.horizontal_offset.saturating_sub(1)
            }
            KeyCode::Right | KeyCode::Char('l') if self.focus == Focus::Data => {
                let last = self.data.columns.len().saturating_sub(1) as u16;
                self.horizontal_offset = min(self.horizontal_offset.saturating_add(1), last)
            }
            KeyCode::Left | KeyCode::Char('h') if self.focus == Focus::Details => {
                self.detail_tab = DetailTab::Record
            }
            KeyCode::Right | KeyCode::Char('l') if self.focus == Focus::Details => {
                self.detail_tab = DetailTab::Schema
            }
            _ => {}
        }
        Ok(false)
    }

    fn move_down(&mut self) {
        match self.focus {
            Focus::Tables => {
                if !self.tables.is_empty() {
                    let i = self.table_state.selected().unwrap_or(0);
                    self.table_state
                        .select(Some(min(i + 1, self.tables.len() - 1)));
                }
            }
            Focus::Data => {
                if !self.data.rows.is_empty() {
                    let i = self.data_state.selected().unwrap_or(0);
                    self.data_state
                        .select(Some(min(i + 1, self.data.rows.len() - 1)));
                    self.detail_scroll = 0;
                }
            }
            Focus::Details => self.detail_scroll = self.detail_scroll.saturating_add(1),
        }
    }

    fn move_up(&mut self) {
        match self.focus {
            Focus::Tables => {
                let i = self.table_state.selected().unwrap_or(0);
                self.table_state.select(Some(i.saturating_sub(1)));
            }
            Focus::Data => {
                let i = self.data_state.selected().unwrap_or(0);
                self.data_state.select(Some(i.saturating_sub(1)));
                self.detail_scroll = 0;
            }
            Focus::Details => self.detail_scroll = self.detail_scroll.saturating_sub(1),
        }
    }

    fn page_down(&mut self) {
        for _ in 0..10 {
            self.move_down();
        }
    }
    fn page_up(&mut self) {
        for _ in 0..10 {
            self.move_up();
        }
    }
    fn move_home(&mut self) {
        match self.focus {
            Focus::Tables => self
                .table_state
                .select((!self.tables.is_empty()).then_some(0)),
            Focus::Data => self
                .data_state
                .select((!self.data.rows.is_empty()).then_some(0)),
            Focus::Details => self.detail_scroll = 0,
        }
    }
    fn move_end(&mut self) {
        match self.focus {
            Focus::Tables => self.table_state.select(self.tables.len().checked_sub(1)),
            Focus::Data => self.data_state.select(self.data.rows.len().checked_sub(1)),
            Focus::Details => self.detail_scroll = u16::MAX,
        }
    }

    fn set_sort(&mut self, column: usize, direction: SortDirection) -> Result<()> {
        self.sort = Some((column, direction));
        self.load_current_table()?;
        self.data_state
            .select((!self.data.rows.is_empty()).then_some(0));
        self.detail_tab = DetailTab::Record;
        let name = self.data.columns.get(column).cloned().unwrap_or_default();
        self.set_status(
            format!(
                "Sorted {name} {}; first row is the extreme value",
                direction.sql()
            ),
            false,
        );
        Ok(())
    }

    fn execute_sql(&mut self, text: String) -> Result<()> {
        let sql = text.trim();
        if sql.is_empty() {
            return Ok(());
        }
        if is_read_query(sql) {
            match query_dataset(&self.conn, sql, false) {
                Ok(data) => {
                    self.data = data;
                    self.data_state
                        .select((!self.data.rows.is_empty()).then_some(0));
                    self.query_label = Some(sql.to_string());
                    self.filter = None;
                    self.sort = None;
                    self.focus = Focus::Data;
                    self.horizontal_offset = 0;
                    self.set_status(
                        format!("Query returned {} rows", self.data.rows.len()),
                        false,
                    );
                }
                Err(err) => self.set_status(format!("SQL error: {err:#}"), true),
            }
        } else {
            self.mode = Mode::ConfirmWriteSql {
                sql: sql.to_string(),
            };
        }
        Ok(())
    }

    fn execute_write_sql(&mut self, sql: &str) -> Result<()> {
        match self.conn.execute_batch(sql) {
            Ok(()) => {
                self.reload_tables()?;
                if self.selected_table.is_some() {
                    self.load_current_table()?;
                }
                self.set_status("SQL write executed successfully", false);
            }
            Err(err) => self.set_status(format!("SQL error: {err}"), true),
        }
        Ok(())
    }

    fn save_edit(&mut self, column: usize, value: String) -> Result<()> {
        let row_index = self.selected_row().context("no row selected")?;
        let table = self.current_table().context("no table selected")?.clone();
        let identity = self
            .data
            .identities
            .get(row_index)
            .cloned()
            .unwrap_or_default();
        let col = table.columns.get(column).context("column unavailable")?;
        let (where_sql, where_values): (String, Vec<Value>) = if let Some(rowid) = identity.rowid {
            let rowid_expression = table
                .rowid_expression
                .context("table rowid alias is unavailable")?;
            (
                format!("{} = ?2", quote_ident(rowid_expression)),
                vec![Value::Integer(rowid)],
            )
        } else if !identity.primary_key.is_empty() {
            let clauses = identity
                .primary_key
                .iter()
                .enumerate()
                .map(|(i, (name, _))| format!("{} IS ?{}", quote_ident(name), i + 2))
                .collect::<Vec<_>>()
                .join(" AND ");
            (
                clauses,
                identity
                    .primary_key
                    .iter()
                    .map(|(_, v)| v.clone())
                    .collect(),
            )
        } else {
            self.set_status(
                "This query/table has no usable rowid or primary key; editing is disabled",
                true,
            );
            return Ok(());
        };
        let sql = format!(
            "UPDATE {} SET {} = ?1 WHERE {}",
            quote_ident(&table.name),
            quote_ident(&col.name),
            where_sql
        );
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(where_values.len() + 1);
        params.push(&value);
        for item in &where_values {
            params.push(item);
        }
        match self.conn.execute(&sql, params.as_slice()) {
            Ok(1) => {
                let mut updated_identity = identity.clone();
                if identity.rowid.is_some()
                    && col.pk_order > 0
                    && col.declared_type.eq_ignore_ascii_case("INTEGER")
                {
                    updated_identity.rowid = value.parse::<i64>().ok();
                }
                for (name, old_value) in &mut updated_identity.primary_key {
                    if name == &col.name {
                        *old_value = Value::Text(value.clone());
                    }
                }
                self.load_current_table()?;
                let target = self
                    .find_identity(&updated_identity)
                    .unwrap_or(row_index.min(self.data.rows.len().saturating_sub(1)));
                self.data_state
                    .select((!self.data.rows.is_empty()).then_some(target));
                self.detail_tab = DetailTab::Record;
                self.set_status(
                    format!(
                        "Saved {}.{}; updated complete row shown",
                        table.name, col.name
                    ),
                    false,
                );
            }
            Ok(changed) => self.set_status(
                format!("Edit affected {changed} rows; expected exactly one"),
                true,
            ),
            Err(err) => self.set_status(format!("Edit failed: {err}"), true),
        }
        Ok(())
    }

    fn find_identity(&self, needle: &RowIdentity) -> Option<usize> {
        self.data.identities.iter().position(|candidate| {
            if needle.rowid.is_some() {
                candidate.rowid == needle.rowid
            } else {
                candidate.primary_key == needle.primary_key
            }
        })
    }
}

fn query_dataset(conn: &Connection, sql: &str, first_is_rowid: bool) -> Result<DataSet> {
    let mut stmt = conn.prepare(sql)?;
    let all_names = stmt
        .column_names()
        .iter()
        .map(|s| (*s).to_string())
        .collect::<Vec<_>>();
    let display_start = usize::from(first_is_rowid);
    let columns = all_names
        .iter()
        .skip(display_start)
        .cloned()
        .collect::<Vec<_>>();
    let mut rows = stmt.query([])?;
    let mut output = Vec::new();
    let mut typed_rows = Vec::new();
    let mut identities = Vec::new();
    while let Some(row) = rows.next()? {
        let rowid = if first_is_rowid {
            row.get::<_, Option<i64>>(0).ok().flatten()
        } else {
            None
        };
        let mut values = Vec::with_capacity(columns.len());
        let mut typed_values = Vec::with_capacity(columns.len());
        for index in display_start..all_names.len() {
            let value = row.get_ref(index)?;
            values.push(value_to_string(value));
            typed_values.push(value.into());
        }
        identities.push(RowIdentity {
            rowid,
            primary_key: Vec::new(),
        });
        output.push(values);
        typed_rows.push(typed_values);
    }
    Ok(DataSet {
        columns,
        rows: output,
        typed_rows,
        identities,
        truncated: false,
    })
}

fn value_to_string(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "NULL".to_string(),
        ValueRef::Integer(v) => v.to_string(),
        ValueRef::Real(v) => v.to_string(),
        ValueRef::Text(v) => String::from_utf8_lossy(v).into_owned(),
        ValueRef::Blob(v) => {
            let shown = v
                .iter()
                .take(32)
                .map(|b| format!("{b:02X}"))
                .collect::<String>();
            if v.len() > 32 {
                format!("x'{shown}…' ({} bytes)", v.len())
            } else {
                format!("x'{shown}'")
            }
        }
    }
}

fn is_read_query(sql: &str) -> bool {
    let first = sql
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(
        first.as_str(),
        "SELECT" | "WITH" | "PRAGMA" | "EXPLAIN" | "VALUES"
    )
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[derive(Debug, PartialEq, Eq)]
enum InputAction {
    Continue,
    Submit,
    Cancel,
}

fn edit_text_key(key: KeyEvent, text: &mut String, cursor: &mut usize) -> InputAction {
    match key.code {
        KeyCode::Esc => InputAction::Cancel,
        KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => InputAction::Submit,
        KeyCode::Char(c) => {
            let byte = char_to_byte(text, *cursor);
            text.insert(byte, c);
            *cursor += 1;
            InputAction::Continue
        }
        KeyCode::Backspace if *cursor > 0 => {
            let end = char_to_byte(text, *cursor);
            let start = char_to_byte(text, *cursor - 1);
            text.replace_range(start..end, "");
            *cursor -= 1;
            InputAction::Continue
        }
        KeyCode::Delete => {
            let start = char_to_byte(text, *cursor);
            if start < text.len() {
                let end = char_to_byte(text, *cursor + 1);
                text.replace_range(start..end, "");
            }
            InputAction::Continue
        }
        KeyCode::Left => {
            *cursor = cursor.saturating_sub(1);
            InputAction::Continue
        }
        KeyCode::Right => {
            *cursor = min(*cursor + 1, text.chars().count());
            InputAction::Continue
        }
        KeyCode::Home => {
            *cursor = 0;
            InputAction::Continue
        }
        KeyCode::End => {
            *cursor = text.chars().count();
            InputAction::Continue
        }
        _ => InputAction::Continue,
    }
}

fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

fn draw(frame: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(frame.area());
    draw_header(frame, app, root[0]);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(24),
            Constraint::Percentage(58),
            Constraint::Min(28),
        ])
        .split(root[1]);
    draw_tables(frame, app, body[0]);
    draw_data(frame, app, body[1]);
    draw_details(frame, app, body[2]);
    draw_status(frame, app, root[2]);
    draw_footer(frame, app, root[3]);
    draw_mode(frame, app);
}

fn focus_style(active: bool) -> Style {
    if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let title = Line::from(vec![
        Span::styled(
            " toolo ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  SQLite browser  "),
        Span::styled(
            app.db_path.display().to_string(),
            Style::default().fg(Color::Yellow),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(title).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn draw_tables(frame: &mut Frame, app: &mut App, area: Rect) {
    let items = app
        .tables
        .iter()
        .map(|t| ListItem::new(t.name.clone()))
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Tables [Enter] ")
                .border_style(focus_style(app.focus == Focus::Tables)),
        )
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(list, area, &mut app.table_state);
}

fn draw_data(frame: &mut Frame, app: &mut App, area: Rect) {
    let title = if let Some(query) = &app.query_label {
        format!(" Query results: {} ", truncate(query, 42))
    } else if let Some(table) = app.current_table() {
        let mut t = format!(" Data: {} ", table.name);
        if let Some(filter) = &app.filter {
            let name = app
                .data
                .columns
                .get(filter.column)
                .map(String::as_str)
                .unwrap_or("?");
            t.push_str(&format!(
                "[{} {} {:?}] ",
                name,
                filter.mode.label(),
                filter.pattern
            ));
        }
        if let Some((column, direction)) = app.sort {
            let name = app
                .data
                .columns
                .get(column)
                .map(String::as_str)
                .unwrap_or("?");
            t.push_str(&format!("[sort {} {}] ", name, direction.sql()));
        }
        t
    } else {
        " Data ".to_string()
    };
    let inner_width = area.width.saturating_sub(4) as usize;
    let start = min(
        app.horizontal_offset as usize,
        app.data.columns.len().saturating_sub(1),
    );
    let all_widths = app
        .data
        .columns
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let max_cell = app
                .data
                .rows
                .iter()
                .take(100)
                .filter_map(|r| r.get(i))
                .map(|v| UnicodeWidthStr::width(v.as_str()))
                .max()
                .unwrap_or(0);
            min(name.width().max(max_cell).max(3) + 2, 32)
        })
        .collect::<Vec<_>>();
    let mut end = start;
    let mut used = 0usize;
    while end < all_widths.len() {
        let next = all_widths[end] + usize::from(end > start);
        if end > start && used + next > inner_width {
            break;
        }
        used += next;
        end += 1;
    }
    if end == start && start < app.data.columns.len() {
        end += 1;
    }
    let widths = app
        .data
        .columns
        .get(start..end)
        .unwrap_or(&[])
        .iter()
        .enumerate()
        .map(|(offset, _)| Constraint::Length(all_widths[start + offset] as u16))
        .collect::<Vec<_>>();
    let header = Row::new(
        app.data
            .columns
            .get(start..end)
            .unwrap_or(&[])
            .iter()
            .map(|c| Cell::from(c.clone())),
    )
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .height(1)
    .bottom_margin(1);
    let rows = app.data.rows.iter().map(|values| {
        Row::new(
            values
                .get(start..end)
                .unwrap_or(&[])
                .iter()
                .map(|v| Cell::from(truncate(v, 30))),
        )
    });
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(focus_style(app.focus == Focus::Data)),
        )
        .row_highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
        .highlight_symbol("▶ ")
        .column_spacing(1);
    frame.render_stateful_widget(table, area, &mut app.data_state);
    if app.data.columns.is_empty() && inner_width > 0 {
        let msg = if app.data.rows.is_empty() {
            "No rows / no query result columns"
        } else {
            "No columns"
        };
        frame.render_widget(
            Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
            area.inner(ratatui::layout::Margin {
                horizontal: 2,
                vertical: 2,
            }),
        );
    }
}

fn draw_details(frame: &mut Frame, app: &App, area: Rect) {
    let title = match app.detail_tab {
        DetailTab::Record => " Complete record [v/←] ",
        DetailTab::Schema => " Schema [v/→] ",
    };
    let text = match app.detail_tab {
        DetailTab::Record => selected_record_text(app),
        DetailTab::Schema => schema_text(app),
    };
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(focus_style(app.focus == Focus::Details)),
            ),
        area,
    );
}

fn selected_record_text(app: &App) -> Text<'static> {
    let Some(index) = app.selected_row() else {
        return Text::from("No row selected.");
    };
    let Some(row) = app.data.rows.get(index) else {
        return Text::from("No row selected.");
    };
    let mut lines = vec![
        Line::from(vec![Span::styled(
            format!("Row {} of {}", index + 1, app.data.rows.len()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
    ];
    for (name, value) in app.data.columns.iter().zip(row) {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{name}: "),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(value.clone()),
        ]));
    }
    Text::from(lines)
}

fn schema_text(app: &App) -> Text<'static> {
    let Some(table) = app.current_table() else {
        return Text::from("Custom query result. Select a table to inspect its schema.");
    };
    let mut lines = vec![
        Line::from(Span::styled(
            table.schema.clone(),
            Style::default().fg(Color::Green),
        )),
        Line::from(""),
    ];
    for col in &table.columns {
        let pk = if col.pk_order > 0 {
            format!(" PK#{}", col.pk_order)
        } else {
            String::new()
        };
        lines.push(Line::from(format!(
            "• {}  {}{}",
            col.name, col.declared_type, pk
        )));
    }
    Text::from(lines)
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let count = format!(" rows: {} ", app.data.rows.len());
    let color = if app.error { Color::Red } else { Color::Green };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(count, Style::default().fg(Color::Black).bg(Color::Cyan)),
            Span::raw(" "),
            Span::styled(app.status.clone(), Style::default().fg(color)),
        ]))
        .block(Block::default().borders(Borders::TOP)),
        area,
    );
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let focus = match app.focus {
        Focus::Tables => "tables",
        Focus::Data => "data",
        Focus::Details => "details",
    };
    let line = format!(" Tab focus ({focus})  ↑↓ navigate  ←→ scroll  / filter  s sort  e edit  q SQL  v record/schema  x clear  r reload  ? help  Q quit ");
    frame.render_widget(
        Paragraph::new(line).style(Style::default().fg(Color::Black).bg(Color::White)),
        area,
    );
}

fn draw_mode(frame: &mut Frame, app: &App) {
    match &app.mode {
        Mode::Normal => {}
        Mode::Help { scroll } => {
            let area = centered_rect(78, 88, frame.area());
            frame.render_widget(Clear, area);
            let help = "KEYBOARD HELP\n\nGLOBAL\n  Tab / Shift-Tab    Move focus among always-visible panes\n  ?                  Open/close this help\n  Q / Ctrl-C         Quit\n  r                  Reload tables and current data\n\nBROWSE\n  ↑↓ or j/k          Select table/row; scroll details\n  Enter              Open highlighted table\n  ←→ or h/l          Horizontally scroll data; choose detail view\n  PgUp/PgDn Home/End Move quickly\n  v                  Toggle complete record / CREATE statement\n\nDATA OPERATIONS\n  s                  Choose any column, then ascending/descending\n                     The first selected row is the extreme-value row\n  /                  Choose column, contains/exact/regex, expression\n                     Match count appears on-screen\n  x                  Clear sort and filter\n  e                  Choose a column and edit selected row\n                     Uses a real SQLite UPDATE; complete updated row remains shown\n\nSQL\n  q                  Enter arbitrary SQL\n  Enter              Execute (writes require y confirmation)\n  Esc                Cancel any workflow\n\nNOTES\n  The center pane scrolls horizontally so every column is available.\n  The right pane displays every value in the selected record simultaneously,\n  wrapping and vertically scrolling when necessary. NULL is entered as text;\n  use custom SQL when a true SQL NULL or typed expression is required.";
            frame.render_widget(
                Paragraph::new(help)
                    .wrap(Wrap { trim: false })
                    .scroll((*scroll, 0))
                    .block(
                        Block::default()
                            .title(" Help (Esc closes) ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Cyan)),
                    ),
                area,
            );
        }
        Mode::SqlInput { text, cursor } => {
            draw_input(frame, " SQL (Enter executes, Esc cancels) ", text, *cursor)
        }
        Mode::FilterColumn { selected } => draw_picker(
            frame,
            " Filter: choose column ",
            &app.data.columns,
            *selected,
            "↑↓ choose, Enter next, Esc cancel",
        ),
        Mode::ChooseMatch { column, mode } => {
            let name = app.data.columns.get(*column).cloned().unwrap_or_default();
            let area = centered_rect(54, 24, frame.area());
            frame.render_widget(Clear, area);
            let text = format!("Column: {name}\n\n1 contains    2 exact    3 regex\n\nSelected: {}\n\n←→/Tab cycle, Enter continue, Esc cancel", mode.label());
            frame.render_widget(
                Paragraph::new(text).block(
                    Block::default()
                        .title(" Filter mode ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Cyan)),
                ),
                area,
            );
        }
        Mode::FilterInput {
            column,
            mode,
            text,
            cursor,
        } => {
            let name = app
                .data
                .columns
                .get(*column)
                .map(String::as_str)
                .unwrap_or("");
            draw_input(
                frame,
                &format!(" Filter {name} ({}) ", mode.label()),
                text,
                *cursor,
            );
        }
        Mode::SortColumn { selected } => draw_picker(
            frame,
            " Sort: choose column ",
            &app.data.columns,
            *selected,
            "↑↓ choose, Enter direction, Esc cancel",
        ),
        Mode::SortDirection { column } => {
            let name = app.data.columns.get(*column).cloned().unwrap_or_default();
            let area = centered_rect(50, 20, frame.area());
            frame.render_widget(Clear, area);
            frame.render_widget(Paragraph::new(format!("Sort {name}\n\n[a] Ascending     [d] Descending\n\nEnter also chooses ascending; Esc cancels")).block(Block::default().title(" Sort direction ").borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan))), area);
        }
        Mode::EditColumn { selected } => draw_picker(
            frame,
            " Edit: choose column ",
            &app.data.columns,
            *selected,
            "↑↓ choose, Enter edit, Esc cancel",
        ),
        Mode::EditValue {
            column,
            text,
            cursor,
        } => {
            let name = app
                .data
                .columns
                .get(*column)
                .map(String::as_str)
                .unwrap_or("");
            draw_input(
                frame,
                &format!(" Edit {name} (Enter saves) "),
                text,
                *cursor,
            );
        }
        Mode::ConfirmWriteSql { sql } => {
            let area = centered_rect(70, 30, frame.area());
            frame.render_widget(Clear, area);
            frame.render_widget(
                Paragraph::new(format!(
                    "This statement may modify the database:\n\n{}\n\nExecute? [y] yes  [n/Esc] no",
                    truncate(sql, 300)
                ))
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .title(" Confirm SQL write ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Yellow)),
                ),
                area,
            );
        }
    }
}

fn draw_picker(frame: &mut Frame, title: &str, items: &[String], selected: usize, hint: &str) {
    let area = centered_rect(54, 60, frame.area());
    frame.render_widget(Clear, area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(2)])
        .split(area);
    let list_items = items
        .iter()
        .map(|v| ListItem::new(v.clone()))
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    state.select((!items.is_empty()).then_some(selected));
    let list = List::new(list_items)
        .highlight_symbol("▶ ")
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );
    frame.render_stateful_widget(list, chunks[0], &mut state);
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::Yellow)),
        chunks[1],
    );
}

fn draw_input(frame: &mut Frame, title: &str, text: &str, cursor: usize) {
    let area = centered_rect(76, 24, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(text.to_string())
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
        area,
    );
    let inner_width = area.width.saturating_sub(2).max(1);
    let before = text.chars().take(cursor).collect::<String>();
    let visual = before
        .chars()
        .map(|c| c.width().unwrap_or(0) as u16)
        .sum::<u16>();
    let x = area.x + 1 + visual.min(inner_width.saturating_sub(1));
    frame.set_cursor_position((x, area.y + 1));
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        format!(
            "{}…",
            text.chars()
                .take(max_chars.saturating_sub(1))
                .collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_identifiers() {
        assert_eq!(quote_ident("odd\"name"), "\"odd\"\"name\"");
    }

    #[test]
    fn edits_unicode_by_character() {
        let mut text = "aEnglish-only textb".to_string();
        let mut cursor = 2;
        assert_eq!(
            edit_text_key(
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
                &mut text,
                &mut cursor
            ),
            InputAction::Continue
        );
        assert_eq!(text, "ab");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn filter_modes_work() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t(a TEXT, b INTEGER); INSERT INTO t VALUES ('Alpha',1),('beta',2),('alphabet',3);").unwrap();
        let mut app = App::new(conn, PathBuf::from(":memory:")).unwrap();
        app.filter = Some(Filter {
            column: 0,
            mode: FilterMode::Contains,
            pattern: "ALP".into(),
        });
        app.load_current_table().unwrap();
        assert_eq!(app.data.rows.len(), 2);
        app.filter = Some(Filter {
            column: 0,
            mode: FilterMode::Regex,
            pattern: "^b.*a$".into(),
        });
        app.load_current_table().unwrap();
        assert_eq!(app.data.rows.len(), 1);
    }
}
