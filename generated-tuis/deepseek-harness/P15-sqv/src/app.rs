//! Application state and keyboard event handling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use crate::db::{Database, QueryOutcome};
use crate::model::*;

pub struct App {
    pub db_path: String,
    pub db: Option<Database>,
    pub banner: Option<String>,

    // Table list.
    pub tables: Vec<TableInfo>,
    pub table_search: String,
    pub table_state: ListState,
    pub selected_table: Option<usize>,

    // Data for the selected table.
    pub columns: Vec<ColumnInfo>,
    pub pk_indices: Vec<usize>,
    pub all_rows: Vec<DataRow>,
    pub sorted_order: Vec<usize>,
    pub filters: Vec<ColumnFilter>,
    pub sort_col: Option<usize>,
    pub sort_asc: bool,
    pub col_widths: Vec<usize>,

    // Data grid cursor / scroll state.
    pub row_cursor: usize,
    pub col_cursor: usize,
    pub vscroll: usize,
    pub hscroll: usize,
    // Set by the renderer so the app can keep the cursor in view.
    pub data_view_width: usize,
    pub data_view_height: usize,

    // View / focus.
    pub view: View,
    pub focus: Focus,
    pub schema_scroll: usize,

    // Query view.
    pub query_buf: TextBuffer,
    pub query_history: Vec<String>,
    pub query_history_idx: Option<usize>,
    pub query_outcome: Option<QueryOutcome>,
    pub query_status: String,
    pub query_scroll: usize,
    pub query_hscroll: usize,
    pub query_row_cursor: usize,
    pub query_col_cursor: usize,
    pub query_col_widths: Vec<usize>,
    pub query_view_width: usize,
    pub query_view_height: usize,

    // Modal prompt.
    pub prompt: Option<Prompt>,

    pub help_open: bool,
    pub status: String,
    pub running: bool,
}

impl App {
    pub fn new(db_path: String) -> Self {
        Self {
            db_path,
            db: None,
            banner: None,
            tables: Vec::new(),
            table_search: String::new(),
            table_state: ListState::default(),
            selected_table: None,
            columns: Vec::new(),
            pk_indices: Vec::new(),
            all_rows: Vec::new(),
            sorted_order: Vec::new(),
            filters: Vec::new(),
            sort_col: None,
            sort_asc: true,
            col_widths: Vec::new(),
            row_cursor: 0,
            col_cursor: 0,
            vscroll: 0,
            hscroll: 0,
            data_view_width: 0,
            data_view_height: 0,
            view: View::Data,
            focus: Focus::Tables,
            schema_scroll: 0,
            query_buf: TextBuffer::new(),
            query_history: Vec::new(),
            query_history_idx: None,
            query_outcome: None,
            query_status: String::new(),
            query_scroll: 0,
            query_hscroll: 0,
            query_row_cursor: 0,
            query_col_cursor: 0,
            query_col_widths: Vec::new(),
            query_view_width: 0,
            query_view_height: 0,
            prompt: None,
            help_open: false,
            status: String::new(),
            running: true,
        }
    }

    pub fn init(&mut self) {
        match Database::open(&self.db_path) {
            Ok(db) => {
                self.db = Some(db);
                self.reload_tables(true);
                self.banner = None;
            }
            Err(e) => {
                self.banner = Some(format!("Failed to open '{}': {}", self.db_path, e));
                self.status = "open failed".to_string();
            }
        }
    }

    // ------------------------------------------------------------------
    // Database loading
    // ------------------------------------------------------------------

    fn reload_tables(&mut self, select_first: bool) {
        let tables_result = match &self.db {
            Some(db) => db.list_tables(),
            None => return,
        };
        match tables_result {
            Ok(tables) => {
                self.tables = tables;
                self.table_state = ListState::default();
                if self.tables.is_empty() {
                    self.selected_table = None;
                    self.clear_data();
                    self.status = "No tables found".to_string();
                    return;
                }
                if select_first {
                    self.selected_table = None;
                    self.table_state.select(Some(0));
                    self.select_table(0);
                }
            }
            Err(e) => {
                self.banner = Some(format!("Failed to list tables: {}", e));
            }
        }
    }

    pub fn open_db(&mut self, path: &str) {
        match Database::open(path) {
            Ok(db) => {
                self.db = Some(db);
                self.db_path = path.to_string();
                self.banner = None;
                self.reload_tables(true);
                self.status = format!("Opened {}", path);
            }
            Err(e) => {
                self.banner = Some(format!("Failed to open '{}': {}", path, e));
                self.status = "open failed".to_string();
            }
        }
    }

    pub fn select_table(&mut self, idx: usize) {
        if idx >= self.tables.len() {
            return;
        }
        self.selected_table = Some(idx);
        let table = self.tables[idx].clone();

        let (cols_result, rows_result) = match &self.db {
            Some(db) => {
                let cols = db.columns(&table.name);
                let rows = cols
                    .as_ref()
                    .ok()
                    .map(|c| db.load_rows(&table.name, c.len(), table.has_rowid));
                (cols, rows)
            }
            None => return,
        };

        match cols_result {
            Ok(cols) => {
                let mut pk_pairs: Vec<(i64, usize)> = cols
                    .iter()
                    .filter(|c| c.pk > 0)
                    .map(|c| (c.pk, c.cid))
                    .collect();
                pk_pairs.sort_by_key(|(pk, _)| *pk);
                self.pk_indices = pk_pairs.into_iter().map(|(_, cid)| cid).collect();
                self.columns = cols;

                match rows_result {
                    Some(Ok(rows)) => {
                        self.all_rows = rows;
                        self.reset_view_state();
                        self.status = format!("Loaded '{}' — {} rows", table.name, self.all_rows.len());
                    }
                    Some(Err(e)) => {
                        self.banner = Some(format!("Failed to load '{}': {}", table.name, e));
                        self.clear_data();
                    }
                    None => {
                        self.banner = Some(format!("Failed to read columns of '{}'", table.name));
                        self.clear_data();
                    }
                }
            }
            Err(e) => {
                self.banner = Some(format!("Failed to read columns of '{}': {}", table.name, e));
                self.clear_data();
            }
        }
    }

    fn reset_view_state(&mut self) {
        self.filters.clear();
        self.sort_col = None;
        self.sort_asc = true;
        self.row_cursor = 0;
        self.col_cursor = 0;
        self.vscroll = 0;
        self.hscroll = 0;
        self.recompute_view();
    }

    fn clear_data(&mut self) {
        self.columns.clear();
        self.pk_indices.clear();
        self.all_rows.clear();
        self.sorted_order.clear();
        self.filters.clear();
        self.sort_col = None;
        self.col_widths.clear();
        self.row_cursor = 0;
        self.col_cursor = 0;
        self.vscroll = 0;
        self.hscroll = 0;
    }

    // ------------------------------------------------------------------
    // View computation
    // ------------------------------------------------------------------

    /// Recompute `sorted_order` from filters + sort, then column widths and
    /// cursor bounds. Also accounts for a live filter prompt in progress.
    pub fn recompute_view(&mut self) {
        let mut active: Vec<ColumnFilter> = self.filters.clone();
        if let Some(p) = &self.prompt {
            if let PromptKind::Filter { col } = p.kind {
                active.retain(|f| f.col != col);
                let f = ColumnFilter::new(col, p.mode, p.buffer.text.clone());
                if f.is_valid() {
                    active.push(f);
                }
            }
        }

        let mut filtered: Vec<usize> = Vec::new();
        for (i, row) in self.all_rows.iter().enumerate() {
            let ok = active.iter().all(|f| f.matches(&row.cells[f.col].display()));
            if ok {
                filtered.push(i);
            }
        }

        if let Some(sc) = self.sort_col {
            if sc < self.columns.len() {
                let asc = self.sort_asc;
                filtered.sort_by(|&a, &b| {
                    let c = compare_values(&self.all_rows[a].cells[sc], &self.all_rows[b].cells[sc]);
                    if asc {
                        c
                    } else {
                        c.reverse()
                    }
                });
            }
        }

        self.sorted_order = filtered;
        self.compute_col_widths();
        self.clamp_cursors();
    }

    fn compute_col_widths(&mut self) {
        let mut widths: Vec<usize> = self.columns.iter().map(|c| display_width(&c.name)).collect();
        for &r in &self.sorted_order {
            let row = &self.all_rows[r];
            for (i, cell) in row.cells.iter().enumerate() {
                if i < widths.len() {
                    let w = display_width(&cell.display());
                    if w > widths[i] {
                        widths[i] = w;
                    }
                }
            }
        }
        self.col_widths = widths;
    }

    fn clamp_cursors(&mut self) {
        if self.sorted_order.is_empty() {
            self.row_cursor = 0;
            self.vscroll = 0;
        } else if self.row_cursor >= self.sorted_order.len() {
            self.row_cursor = self.sorted_order.len() - 1;
        }
        if !self.columns.is_empty() && self.col_cursor >= self.columns.len() {
            self.col_cursor = self.columns.len() - 1;
        }
        if self.columns.is_empty() {
            self.col_cursor = 0;
        }
    }

    // Column geometry (char units).
    pub fn col_starts(&self) -> Vec<usize> {
        let mut v = Vec::with_capacity(self.col_widths.len());
        let mut x = 0usize;
        for w in &self.col_widths {
            v.push(x);
            x += w + 1;
        }
        v
    }

    pub fn total_width(&self) -> usize {
        let starts = self.col_starts();
        match (starts.last(), self.col_widths.last()) {
            (Some(&x), Some(&w)) => x + w,
            _ => 0,
        }
    }

    pub fn ensure_row_visible(&mut self) {
        let h = self.data_view_height.max(1);
        if self.row_cursor < self.vscroll {
            self.vscroll = self.row_cursor;
        } else if self.row_cursor >= self.vscroll + h {
            self.vscroll = self.row_cursor + 1 - h;
        }
    }

    pub fn ensure_column_visible(&mut self) {
        if self.columns.is_empty() {
            return;
        }
        let w = self.data_view_width.max(1);
        let starts = self.col_starts();
        let c = self.col_cursor.min(starts.len() - 1);
        let x = starts[c];
        let cw = self.col_widths.get(c).copied().unwrap_or(0);
        if x < self.hscroll {
            self.hscroll = x;
        } else if x + cw > self.hscroll + w {
            self.hscroll = (x + cw).saturating_sub(w);
        }
        self.hscroll = self.hscroll.min(self.total_width().saturating_sub(w));
    }

    pub fn filtered_table_indices(&self) -> Vec<usize> {
        let q = self.table_search.to_lowercase();
        (0..self.tables.len())
            .filter(|&i| self.tables[i].name.to_lowercase().contains(&q))
            .collect()
    }

    // ------------------------------------------------------------------
    // Key handling
    // ------------------------------------------------------------------

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Help overlay intercepts everything.
        if self.help_open {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter => self.help_open = false,
                KeyCode::Char('q') => {
                    self.help_open = false;
                    self.running = false;
                }
                _ => {}
            }
            return;
        }

        // Modal prompt intercepts everything.
        if self.prompt.is_some() {
            self.handle_prompt_key(key);
            return;
        }

        // Global keys.
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
                return;
            }
            KeyCode::Char('q') if self.focus != Focus::QueryInput => {
                self.running = false;
                return;
            }
            KeyCode::F(1) => {
                self.help_open = !self.help_open;
                return;
            }
            KeyCode::Char('?') if self.focus != Focus::QueryInput => {
                self.help_open = !self.help_open;
                return;
            }
            KeyCode::Char('o') if self.focus != Focus::QueryInput => {
                self.start_open_db_prompt();
                return;
            }
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.start_open_db_prompt();
                return;
            }
            KeyCode::Tab => {
                self.cycle_focus(true);
                return;
            }
            KeyCode::BackTab => {
                self.cycle_focus(false);
                return;
            }
            KeyCode::Char('1') if self.focus != Focus::QueryInput => {
                self.view = View::Data;
                self.focus = Focus::Main;
                return;
            }
            KeyCode::Char('2') if self.focus != Focus::QueryInput => {
                self.view = View::Schema;
                self.focus = Focus::Main;
                return;
            }
            KeyCode::Char('3') if self.focus != Focus::QueryInput => {
                self.view = View::Query;
                self.focus = Focus::QueryInput;
                return;
            }
            _ => {}
        }

        match self.focus {
            Focus::Tables => self.handle_tables_key(key),
            Focus::Main => match self.view {
                View::Data => self.handle_data_key(key),
                View::Schema => self.handle_schema_key(key),
                View::Query => {}
            },
            Focus::QueryInput => self.handle_query_input_key(key),
            Focus::QueryResult => self.handle_query_result_key(key),
        }
    }

    fn cycle_focus(&mut self, forward: bool) {
        let stops: Vec<Focus> = match self.view {
            View::Data | View::Schema => vec![Focus::Tables, Focus::Main],
            View::Query => vec![Focus::Tables, Focus::QueryInput, Focus::QueryResult],
        };
        let cur = stops.iter().position(|s| *s == self.focus).unwrap_or(0);
        let n = stops.len();
        let next = if forward { (cur + 1) % n } else { (cur + n - 1) % n };
        self.focus = stops[next];
        self.status = format!("Focus: {}", focus_label(self.focus));
    }

    fn handle_tables_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_table_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_table_selection(1),
            KeyCode::Home | KeyCode::Char('g') => self.table_selection_to(0),
            KeyCode::End | KeyCode::Char('G') => self.table_selection_to(usize::MAX),
            KeyCode::Enter => self.select_current_table(),
            KeyCode::Char('/') => self.start_table_search(),
            _ => {}
        }
    }

    fn move_table_selection(&mut self, delta: isize) {
        let idxs = self.filtered_table_indices();
        if idxs.is_empty() {
            self.table_state.select(None);
            return;
        }
        let cur = self.table_state.selected().unwrap_or(0).min(idxs.len() - 1);
        let newpos = (cur as isize + delta).clamp(0, idxs.len() as isize - 1) as usize;
        self.table_state.select(Some(newpos));
    }

    fn table_selection_to(&mut self, pos: usize) {
        let idxs = self.filtered_table_indices();
        if idxs.is_empty() {
            self.table_state.select(None);
            return;
        }
        let p = pos.min(idxs.len() - 1);
        self.table_state.select(Some(p));
    }

    fn select_current_table(&mut self) {
        let idxs = self.filtered_table_indices();
        let pos = self.table_state.selected().unwrap_or(0);
        if let Some(&actual) = idxs.get(pos) {
            self.select_table(actual);
            self.view = View::Data;
            self.focus = Focus::Main;
        }
    }

    fn handle_data_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.row_cursor > 0 {
                    self.row_cursor -= 1;
                }
                self.ensure_row_visible();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.row_cursor + 1 < self.sorted_order.len() {
                    self.row_cursor += 1;
                }
                self.ensure_row_visible();
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if self.col_cursor > 0 {
                    self.col_cursor -= 1;
                }
                self.ensure_column_visible();
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.col_cursor + 1 < self.columns.len() {
                    self.col_cursor += 1;
                }
                self.ensure_column_visible();
            }
            KeyCode::PageUp => {
                let h = self.data_view_height.max(1);
                self.row_cursor = self.row_cursor.saturating_sub(h);
                self.ensure_row_visible();
            }
            KeyCode::PageDown => {
                let h = self.data_view_height.max(1);
                if !self.sorted_order.is_empty() {
                    self.row_cursor = (self.row_cursor + h).min(self.sorted_order.len() - 1);
                }
                self.ensure_row_visible();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let h = self.data_view_height.max(1);
                self.row_cursor = self.row_cursor.saturating_sub(h);
                self.ensure_row_visible();
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let h = self.data_view_height.max(1);
                if !self.sorted_order.is_empty() {
                    self.row_cursor = (self.row_cursor + h).min(self.sorted_order.len() - 1);
                }
                self.ensure_row_visible();
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.row_cursor = 0;
                self.ensure_row_visible();
            }
            KeyCode::End | KeyCode::Char('G') => {
                if !self.sorted_order.is_empty() {
                    self.row_cursor = self.sorted_order.len() - 1;
                }
                self.ensure_row_visible();
            }
            KeyCode::Char('e') | KeyCode::Enter => self.start_edit_prompt(),
            KeyCode::Char('f') => self.start_filter_prompt(),
            KeyCode::Char('s') => self.sort_current(true),
            KeyCode::Char('S') => self.sort_current(false),
            KeyCode::Char('u') => {
                self.sort_col = None;
                self.recompute_view();
                self.status = "Sort cleared".to_string();
            }
            KeyCode::Char('x') => {
                self.filters.clear();
                self.recompute_view();
                self.status = "Filters cleared".to_string();
            }
            KeyCode::Char('r') => self.reload_current_table(),
            KeyCode::Esc => self.focus = Focus::Tables,
            _ => {}
        }
    }

    fn handle_schema_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.schema_scroll = self.schema_scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.schema_scroll = self.schema_scroll.saturating_add(1),
            KeyCode::PageUp => self.schema_scroll = self.schema_scroll.saturating_sub(10),
            KeyCode::PageDown => self.schema_scroll = self.schema_scroll.saturating_add(10),
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.schema_scroll = self.schema_scroll.saturating_sub(10);
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.schema_scroll = self.schema_scroll.saturating_add(10);
            }
            KeyCode::Home => self.schema_scroll = 0,
            KeyCode::End => self.schema_scroll = 100_000,
            KeyCode::Esc => self.focus = Focus::Tables,
            _ => {}
        }
    }

    fn handle_query_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.query_buf.clear(),
            KeyCode::Enter => self.execute_query(),
            KeyCode::Left => self.query_buf.move_left(),
            KeyCode::Right => self.query_buf.move_right(),
            KeyCode::Home => self.query_buf.home(),
            KeyCode::End => self.query_buf.end(),
            KeyCode::Backspace => self.query_buf.backspace(),
            KeyCode::Delete => self.query_buf.delete(),
            KeyCode::Up => self.query_history_prev(),
            KeyCode::Down => self.query_history_next(),
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => self.query_buf.clear(),
            KeyCode::Char(c) => self.query_buf.insert_char(c),
            _ => {}
        }
    }

    fn handle_query_result_key(&mut self, key: KeyEvent) {
        let nrows = self.query_row_count();
        let ncols = self.query_col_count();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.query_row_cursor > 0 {
                    self.query_row_cursor -= 1;
                }
                self.ensure_query_row_visible();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.query_row_cursor + 1 < nrows {
                    self.query_row_cursor += 1;
                }
                self.ensure_query_row_visible();
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if self.query_col_cursor > 0 {
                    self.query_col_cursor -= 1;
                }
                self.ensure_query_col_visible();
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.query_col_cursor + 1 < ncols {
                    self.query_col_cursor += 1;
                }
                self.ensure_query_col_visible();
            }
            KeyCode::PageUp => {
                self.query_scroll = self.query_scroll.saturating_sub(self.query_view_height.max(1));
            }
            KeyCode::PageDown => {
                let h = self.query_view_height.max(1);
                self.query_scroll = (self.query_scroll + h).min(nrows.saturating_sub(1));
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.query_row_cursor = 0;
                self.query_scroll = 0;
            }
            KeyCode::End | KeyCode::Char('G') => {
                if nrows > 0 {
                    self.query_row_cursor = nrows - 1;
                }
                self.ensure_query_row_visible();
            }
            KeyCode::Esc => self.focus = Focus::QueryInput,
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Actions
    // ------------------------------------------------------------------

    fn sort_current(&mut self, asc: bool) {
        let col = self.col_cursor;
        if col >= self.columns.len() {
            return;
        }
        self.sort_col = Some(col);
        self.sort_asc = asc;
        self.recompute_view();
        if !self.sorted_order.is_empty() {
            self.row_cursor = 0;
            self.vscroll = 0;
        }
        self.col_cursor = col;
        self.ensure_column_visible();
        let name = self.columns[col].name.clone();
        let dir = if asc { "ASC" } else { "DESC" };
        self.status = format!("Sorted by '{}' {} — extreme value at top", name, dir);
    }

    fn start_filter_prompt(&mut self) {
        let col = self.col_cursor;
        if col >= self.columns.len() {
            return;
        }
        let (mode, initial) = self
            .filters
            .iter()
            .find(|f| f.col == col)
            .map(|f| (f.mode, f.value.clone()))
            .unwrap_or((FilterMode::Contains, String::new()));
        self.prompt = Some(Prompt {
            title: format!("Filter '{}'", self.columns[col].name),
            buffer: TextBuffer::from_str(&initial),
            kind: PromptKind::Filter { col },
            mode,
        });
        self.recompute_view();
    }

    fn start_edit_prompt(&mut self) {
        if self.sorted_order.is_empty() {
            return;
        }
        let row = self.row_cursor;
        let col = self.col_cursor;
        if col >= self.columns.len() {
            return;
        }
        let idx = self.sorted_order[row];
        let cur = self.all_rows[idx].cells[col].display();
        self.prompt = Some(Prompt {
            title: format!("Edit '{}' (row {})", self.columns[col].name, row + 1),
            buffer: TextBuffer::from_str(&cur),
            kind: PromptKind::Edit { row, col },
            mode: FilterMode::Exact,
        });
    }

    fn start_open_db_prompt(&mut self) {
        self.prompt = Some(Prompt {
            title: "Open database path".to_string(),
            buffer: TextBuffer::from_str(&self.db_path),
            kind: PromptKind::OpenDb,
            mode: FilterMode::Exact,
        });
    }

    fn start_table_search(&mut self) {
        self.prompt = Some(Prompt {
            title: "Filter tables".to_string(),
            buffer: TextBuffer::from_str(&self.table_search),
            kind: PromptKind::TableSearch,
            mode: FilterMode::Contains,
        });
    }

    fn reload_current_table(&mut self) {
        let Some(idx) = self.selected_table else { return };
        let (name, has_rowid) = (self.tables[idx].name.clone(), self.tables[idx].has_rowid);
        let loaded = match &self.db {
            Some(db) => db.load_rows(&name, self.columns.len(), has_rowid),
            None => return,
        };
        match loaded {
            Ok(rows) => {
                self.all_rows = rows;
                self.recompute_view();
                self.status = format!("Reloaded '{}' — {} rows", name, self.all_rows.len());
            }
            Err(e) => self.status = format!("Reload failed: {}", e),
        }
    }

    fn row_is_editable(&self, row: &DataRow) -> bool {
        row.rowid.is_some() || !self.pk_indices.is_empty()
    }

    fn do_edit(&mut self, display_row: usize, col: usize, text: String) {
        let Some(&idx) = self.sorted_order.get(display_row) else {
            return;
        };
        let row = self.all_rows[idx].clone();
        if !self.row_is_editable(&row) {
            self.status = "Cannot edit: table has no rowid and no primary key".to_string();
            return;
        }
        let declared_type = match self.columns.get(col) {
            Some(c) => c.declared_type.clone(),
            None => return,
        };
        let new_value = parse_value(&text, &declared_type);
        let Some(sel) = self.selected_table else { return };
        let table = self.tables[sel].name.clone();
        let col_name = self.columns.get(col).map(|c| c.name.clone()).unwrap_or_default();
        let has_rowid = self.tables[sel].has_rowid;

        let result = match &self.db {
            Some(db) => db.update_cell(&table, &self.columns, &self.pk_indices, &row, col, &new_value),
            None => return,
        };
        match result {
            Ok(_) => {
                self.reload_after_edit(&table, has_rowid, row.rowid);
                self.status = format!("Updated {}.{}", table, col_name);
            }
            Err(e) => self.status = format!("Update failed: {}", e),
        }
    }

    fn reload_after_edit(&mut self, table: &str, has_rowid: bool, target_rowid: Option<i64>) {
        let loaded = match &self.db {
            Some(db) => db.load_rows(table, self.columns.len(), has_rowid),
            None => return,
        };
        match loaded {
            Ok(rows) => {
                self.all_rows = rows;
                self.recompute_view();
                if let Some(rid) = target_rowid {
                    if let Some(pos) = self.sorted_order.iter().position(|&i| self.all_rows[i].rowid == Some(rid)) {
                        self.row_cursor = pos;
                        self.ensure_row_visible();
                    }
                }
            }
            Err(_) => self.status = "Edit saved but reload failed".to_string(),
        }
    }

    fn execute_query(&mut self) {
        let sql = self.query_buf.text.clone();
        if sql.trim().is_empty() {
            self.query_status = "Empty query".to_string();
            return;
        }
        self.query_history.push(sql.clone());
        self.query_history_idx = None;
        let outcome = match &self.db {
            Some(db) => db.run_query(&sql),
            None => {
                self.query_status = "No database open".to_string();
                return;
            }
        };
        match outcome {
            Ok(out) => {
                match &out {
                    QueryOutcome::Rows { columns, rows } => {
                        self.query_status = format!("{} column(s), {} row(s)", columns.len(), rows.len());
                        self.query_col_widths = compute_widths_from(columns, rows);
                    }
                    QueryOutcome::Affected { n } => {
                        self.query_status = format!("{} row(s) affected", n);
                        self.query_col_widths.clear();
                    }
                }
                self.query_outcome = Some(out);
                self.query_scroll = 0;
                self.query_hscroll = 0;
                self.query_row_cursor = 0;
                self.query_col_cursor = 0;
            }
            Err(e) => self.query_status = format!("Error: {}", e),
        }
    }

    fn query_history_prev(&mut self) {
        if self.query_history.is_empty() {
            return;
        }
        let idx = match self.query_history_idx {
            None => self.query_history.len() - 1,
            Some(i) if i > 0 => i - 1,
            Some(i) => i,
        };
        self.query_history_idx = Some(idx);
        self.query_buf = TextBuffer::from_str(&self.query_history[idx]);
    }

    fn query_history_next(&mut self) {
        match self.query_history_idx {
            Some(i) if i + 1 < self.query_history.len() => {
                self.query_history_idx = Some(i + 1);
                self.query_buf = TextBuffer::from_str(&self.query_history[i + 1]);
            }
            Some(_) => {
                self.query_history_idx = None;
                self.query_buf.clear();
            }
            None => {}
        }
    }

    pub fn query_row_count(&self) -> usize {
        match &self.query_outcome {
            Some(QueryOutcome::Rows { rows, .. }) => rows.len(),
            _ => 0,
        }
    }

    pub fn query_col_count(&self) -> usize {
        match &self.query_outcome {
            Some(QueryOutcome::Rows { columns, .. }) => columns.len(),
            _ => 0,
        }
    }

    pub fn query_cell(&self, r: usize, c: usize) -> String {
        match &self.query_outcome {
            Some(QueryOutcome::Rows { rows, .. }) => rows.get(r).and_then(|row| row.get(c)).map(|v| v.display()).unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn ensure_query_row_visible(&mut self) {
        let h = self.query_view_height.max(1);
        if self.query_row_cursor < self.query_scroll {
            self.query_scroll = self.query_row_cursor;
        } else if self.query_row_cursor >= self.query_scroll + h {
            self.query_scroll = self.query_row_cursor + 1 - h;
        }
    }

    fn ensure_query_col_visible(&mut self) {
        if self.query_col_widths.is_empty() {
            return;
        }
        let w = self.query_view_width.max(1);
        let starts = col_starts(&self.query_col_widths);
        let c = self.query_col_cursor.min(starts.len() - 1);
        let x = starts[c];
        let cw = self.query_col_widths.get(c).copied().unwrap_or(0);
        if x < self.query_hscroll {
            self.query_hscroll = x;
        } else if x + cw > self.query_hscroll + w {
            self.query_hscroll = (x + cw).saturating_sub(w);
        }
        let total = total_width_of(&self.query_col_widths);
        self.query_hscroll = self.query_hscroll.min(total.saturating_sub(w));
    }

    // ------------------------------------------------------------------
    // Modal prompt handling
    // ------------------------------------------------------------------

    fn handle_prompt_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.cancel_prompt();
                return;
            }
            KeyCode::Enter => {
                self.confirm_prompt();
                return;
            }
            KeyCode::Tab => {
                if let Some(p) = self.prompt.as_mut() {
                    if let PromptKind::Filter { .. } = p.kind {
                        p.mode = p.mode.next();
                        self.recompute_view();
                    }
                }
                return;
            }
            KeyCode::BackTab => {
                if let Some(p) = self.prompt.as_mut() {
                    if let PromptKind::Filter { .. } = p.kind {
                        p.mode = p.mode.prev();
                        self.recompute_view();
                    }
                }
                return;
            }
            KeyCode::Left => {
                if let Some(p) = self.prompt.as_mut() {
                    p.buffer.move_left();
                }
                return;
            }
            KeyCode::Right => {
                if let Some(p) = self.prompt.as_mut() {
                    p.buffer.move_right();
                }
                return;
            }
            KeyCode::Home => {
                if let Some(p) = self.prompt.as_mut() {
                    p.buffer.home();
                }
                return;
            }
            KeyCode::End => {
                if let Some(p) = self.prompt.as_mut() {
                    p.buffer.end();
                }
                return;
            }
            KeyCode::Backspace => {
                if let Some(p) = self.prompt.as_mut() {
                    p.buffer.backspace();
                }
                self.prompt_changed();
                return;
            }
            KeyCode::Delete => {
                if let Some(p) = self.prompt.as_mut() {
                    p.buffer.delete();
                }
                self.prompt_changed();
                return;
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(p) = self.prompt.as_mut() {
                    p.buffer.clear();
                }
                self.prompt_changed();
                return;
            }
            KeyCode::Char(c) => {
                if let Some(p) = self.prompt.as_mut() {
                    p.buffer.insert_char(c);
                }
                self.prompt_changed();
                return;
            }
            _ => {}
        }
    }

    fn prompt_changed(&mut self) {
        let (kind, text) = match &self.prompt {
            Some(p) => (p.kind.clone(), p.buffer.text.clone()),
            None => return,
        };
        match kind {
            PromptKind::Filter { .. } => self.recompute_view(),
            PromptKind::TableSearch => {
                self.table_search = text;
                let idxs = self.filtered_table_indices();
                if idxs.is_empty() {
                    self.table_state.select(None);
                } else {
                    let cur = self.table_state.selected().unwrap_or(0).min(idxs.len() - 1);
                    self.table_state.select(Some(cur));
                }
            }
            _ => {}
        }
    }

    fn confirm_prompt(&mut self) {
        let Some(p) = self.prompt.take() else { return };
        match p.kind {
            PromptKind::Filter { col } => {
                let f = ColumnFilter::new(col, p.mode, p.buffer.text.clone());
                if f.is_valid() {
                    self.filters.retain(|x| x.col != col);
                    self.filters.push(f);
                    self.status = format!("Filter '{}' {} '{}'", self.column_name(col), p.mode.label(), p.buffer.text);
                } else {
                    self.status = "Invalid regex — filter not applied".to_string();
                }
                self.recompute_view();
                self.row_cursor = 0;
                self.vscroll = 0;
            }
            PromptKind::Edit { row, col } => {
                self.do_edit(row, col, p.buffer.text.clone());
            }
            PromptKind::OpenDb => {
                let path = p.buffer.text.trim().to_string();
                if !path.is_empty() {
                    self.open_db(&path);
                }
            }
            PromptKind::TableSearch => {
                self.table_search = p.buffer.text.clone();
                let idxs = self.filtered_table_indices();
                if !idxs.is_empty() {
                    self.table_state.select(Some(0));
                }
            }
        }
    }

    fn cancel_prompt(&mut self) {
        self.prompt = None;
        self.recompute_view();
    }

    fn column_name(&self, col: usize) -> String {
        self.columns
            .get(col)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "?".to_string())
    }
}

// ----------------------------------------------------------------------
// Free helpers
// ----------------------------------------------------------------------

/// Convert user text into a `CellValue` honoring SQLite's column affinity.
pub fn parse_value(text: &str, declared_type: &str) -> CellValue {
    let t = declared_type.to_ascii_uppercase();
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("null") {
        return CellValue::Null;
    }
    if t.contains("INT") {
        if let Ok(i) = trimmed.parse::<i64>() {
            return CellValue::Integer(i);
        }
        return CellValue::Text(text.to_string());
    }
    if t.contains("CHAR") || t.contains("CLOB") || t.contains("TEXT") {
        return CellValue::Text(text.to_string());
    }
    if t.contains("BLOB") || t.trim().is_empty() {
        return CellValue::Text(text.to_string());
    }
    if t.contains("REAL") || t.contains("FLOA") || t.contains("DOUB") {
        if let Ok(f) = trimmed.parse::<f64>() {
            return CellValue::Real(f);
        }
        return CellValue::Text(text.to_string());
    }
    // NUMERIC affinity (default): coerce int, then real, then keep text.
    if let Ok(i) = trimmed.parse::<i64>() {
        return CellValue::Integer(i);
    }
    if let Ok(f) = trimmed.parse::<f64>() {
        return CellValue::Real(f);
    }
    CellValue::Text(text.to_string())
}

pub fn col_starts(widths: &[usize]) -> Vec<usize> {
    let mut v = Vec::with_capacity(widths.len());
    let mut x = 0usize;
    for w in widths {
        v.push(x);
        x += w + 1;
    }
    v
}

pub fn total_width_of(widths: &[usize]) -> usize {
    let starts = col_starts(widths);
    match (starts.last(), widths.last()) {
        (Some(&x), Some(&w)) => x + w,
        _ => 0,
    }
}

pub fn compute_widths_from(headers: &[String], rows: &[Vec<CellValue>]) -> Vec<usize> {
    let mut widths: Vec<usize> = headers.iter().map(|h| display_width(h)).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                let w = display_width(&cell.display());
                if w > widths[i] {
                    widths[i] = w;
                }
            }
        }
    }
    widths
}

pub fn focus_label(f: Focus) -> &'static str {
    match f {
        Focus::Tables => "Tables",
        Focus::Main => "Main",
        Focus::QueryInput => "Query input",
        Focus::QueryResult => "Query result",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CellValue;

    #[test]
    fn parse_value_by_affinity() {
        assert_eq!(parse_value("42", "INTEGER"), CellValue::Integer(42));
        assert_eq!(parse_value("abc", "INTEGER"), CellValue::Text("abc".into()));
        assert_eq!(parse_value("3.14", "REAL"), CellValue::Real(3.14));
        assert_eq!(parse_value("hello", "TEXT"), CellValue::Text("hello".into()));
        assert_eq!(parse_value("NULL", "TEXT"), CellValue::Null);
        // Empty declared type -> BLOB affinity, no coercion.
        assert_eq!(parse_value("7", ""), CellValue::Text("7".into()));
        // NUMERIC affinity coerces integers and reals.
        assert_eq!(parse_value("7", "NUMERIC"), CellValue::Integer(7));
        assert_eq!(parse_value("7.5", "NUMERIC"), CellValue::Real(7.5));
    }
}
