//! Application state and all non-rendering logic.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use polars::prelude::*;

use crate::input::LineEditor;
use crate::query;
use crate::report::Report;
use crate::table::{self, TableData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryMode {
    Fuzzy,
    SqlLike,
    Sql,
}

impl QueryMode {
    pub fn label(self) -> &'static str {
        match self {
            QueryMode::Fuzzy => "Fuzzy",
            QueryMode::SqlLike => "SQL-Like",
            QueryMode::Sql => "SQL",
        }
    }

    pub fn next(self) -> Self {
        match self {
            QueryMode::Fuzzy => QueryMode::SqlLike,
            QueryMode::SqlLike => QueryMode::Sql,
            QueryMode::Sql => QueryMode::Fuzzy,
        }
    }

    pub fn prompt(self) -> &'static str {
        match self {
            QueryMode::Fuzzy => "keyword",
            QueryMode::SqlLike => "select where <condition>",
            QueryMode::Sql => "select * from df where ...",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Table,
    Query,
    Sort,
    Export,
    Analysis,
}

#[derive(Debug, Clone)]
pub struct SortKey {
    pub column: String,
    pub ascending: bool,
}

pub struct App {
    pub path: String,
    /// Full data set as loaded from disk.
    pub df: DataFrame,
    /// Result of the current query (before sorting).
    pub filtered: DataFrame,
    /// Current displayed result (query + sort applied).
    pub result: DataFrame,
    pub table: TableData,
    pub col_widths: Vec<usize>,

    pub mode: QueryMode,
    pub focus: Focus,
    pub query_editor: LineEditor,
    pub export_editor: LineEditor,

    pub sort_keys: Vec<SortKey>,
    pub sort_selected: usize,

    pub scroll_row: usize,
    pub scroll_col: usize,
    pub selected: usize,

    pub status: String,
    pub status_error: bool,
    pub show_help: bool,
    pub detail_lines: Vec<String>,

    /// Viewport dimensions, refreshed by the renderer each frame.
    pub body_rows: usize,
    pub body_cols: usize,

    pub report: Option<Report>,
    pub analysis_scroll: usize,
    pub analysis_hist_idx: usize,
}

fn load_csv(path: &str) -> Result<DataFrame, String> {
    let lf = LazyCsvReader::new(path.into())
        .with_has_header(true)
        .finish()
        .map_err(|e| format!("{e}"))?;
    lf.collect().map_err(|e| format!("{e}"))
}

fn export_csv(df: &DataFrame, path: &str) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut df = df.clone();
    let mut writer = CsvWriter::new(file);
    writer = writer.include_header(true);
    writer.finish(&mut df).map_err(|e| e.to_string())
}

fn apply_sort(df: &DataFrame, keys: &[SortKey]) -> DataFrame {
    if keys.is_empty() {
        return df.clone();
    }
    let cols: Vec<&str> = keys.iter().map(|k| k.column.as_str()).collect();
    let opts = SortMultipleOptions::default()
        .with_maintain_order(true)
        .with_nulls_last(true)
        .with_order_descending_multi(keys.iter().map(|k| !k.ascending).collect::<Vec<_>>());
    df.sort(cols, opts).unwrap_or_else(|_| df.clone())
}

pub(crate) fn describe_sort(keys: &[SortKey]) -> String {
    if keys.is_empty() {
        return "none".to_string();
    }
    keys.iter()
        .enumerate()
        .map(|(i, k)| {
            let arrow = if k.ascending { "↑" } else { "↓" };
            format!("{}.{} {}", i + 1, k.column, arrow)
        })
        .collect::<Vec<_>>()
        .join("  ")
}

impl App {
    pub fn new(path: &str) -> Result<Self, String> {
        let df = load_csv(path)?;
        let (rows, cols) = (df.height(), df.width());
        let mut app = App {
            path: path.to_string(),
            filtered: df.clone(),
            result: df.clone(),
            df,
            table: TableData::empty(),
            col_widths: Vec::new(),
            mode: QueryMode::Fuzzy,
            focus: Focus::Table,
            query_editor: LineEditor::new(),
            export_editor: LineEditor::new(),
            sort_keys: Vec::new(),
            sort_selected: 0,
            scroll_row: 0,
            scroll_col: 0,
            selected: 0,
            status: String::new(),
            status_error: false,
            show_help: false,
            detail_lines: Vec::new(),
            body_rows: 0,
            body_cols: 0,
            report: None,
            analysis_scroll: 0,
            analysis_hist_idx: 0,
        };
        app.refresh_result();
        app.set_status_ok(format!(
            "Loaded {rows} rows × {cols} columns from {path}"
        ));
        Ok(app)
    }

    fn set_status_ok(&mut self, s: String) {
        self.status = s;
        self.status_error = false;
    }

    fn set_status_err(&mut self, s: String) {
        self.status = s;
        self.status_error = true;
    }

    /// Rebuild the displayed table from the filtered frame + current sort keys.
    fn refresh_result(&mut self) {
        self.result = apply_sort(&self.filtered, &self.sort_keys);
        self.table = table::from_df(&self.result);
        self.col_widths = table::compute_widths(&self.table, 24, 1000);

        let rows = self.table.row_count();
        let cols = self.table.col_count();
        if rows == 0 {
            self.selected = 0;
            self.scroll_row = 0;
        } else {
            if self.selected >= rows {
                self.selected = rows - 1;
            }
            if self.scroll_row >= rows {
                self.scroll_row = rows - 1;
            }
        }
        if cols == 0 {
            self.scroll_col = 0;
        } else if self.scroll_col >= cols {
            self.scroll_col = cols - 1;
        }
    }

    pub fn run_query(&mut self) {
        let q = self.query_editor.content.clone();
        let outcome = match self.mode {
            QueryMode::Fuzzy => query::fuzzy(&self.df, &q),
            QueryMode::SqlLike => query::sql_like(&self.df, &q),
            QueryMode::Sql => query::sql(&self.df, &q),
        };
        match outcome {
            Ok(df) => {
                self.filtered = df;
                self.refresh_result();
                self.selected = 0;
                self.scroll_row = 0;
                let matches = self.table.row_count();
                let total = self.df.height();
                self.set_status_ok(format!(
                    "[{}] {} → {} matches / {} rows",
                    self.mode.label(),
                    if q.trim().is_empty() { "(all)" } else { &q },
                    matches,
                    total
                ));
            }
            Err(e) => self.set_status_err(e),
        }
    }

    fn toggle_sort_key(&mut self, idx: usize) {
        if idx >= self.table.col_count() {
            return;
        }
        let name = self.table.headers[idx].clone();
        if let Some(pos) = self.sort_keys.iter().position(|k| k.column == name) {
            if self.sort_keys[pos].ascending {
                self.sort_keys[pos].ascending = false;
            } else {
                self.sort_keys.remove(pos);
            }
        } else {
            self.sort_keys.push(SortKey {
                column: name,
                ascending: true,
            });
        }
        self.refresh_result();
        self.selected = 0;
        self.scroll_row = 0;
        self.set_status_ok(format!("Sort: {}", describe_sort(&self.sort_keys)));
    }

    fn reload(&mut self) {
        match load_csv(&self.path) {
            Ok(df) => {
                let (rows, cols) = (df.height(), df.width());
                self.df = df.clone();
                self.filtered = df;
                self.sort_keys.clear();
                self.query_editor.clear();
                self.scroll_row = 0;
                self.scroll_col = 0;
                self.selected = 0;
                self.refresh_result();
                self.report = None;
                self.set_status_ok(format!("Reloaded {rows} rows × {cols} columns"));
            }
            Err(e) => self.set_status_err(format!("Reload failed: {e}")),
        }
    }

    fn export(&mut self) {
        let path = self.export_editor.content.trim().to_string();
        if path.is_empty() {
            self.set_status_err("Export path is empty".to_string());
            return;
        }
        match export_csv(&self.result, &path) {
            Ok(()) => {
                self.set_status_ok(format!(
                    "Exported {} rows × {} columns to {path}",
                    self.table.row_count(),
                    self.table.col_count()
                ));
            }
            Err(e) => self.set_status_err(format!("Export failed: {e}")),
        }
    }

    pub fn open_analysis(&mut self) {
        self.analysis_hist_idx = 0;
        self.analysis_scroll = 0;
        self.rebuild_report();
        self.focus = Focus::Analysis;
    }

    fn rebuild_report(&mut self) {
        self.report = Some(Report::for_df(&self.result, self.analysis_hist_idx));
    }

    fn shift_hist(&mut self, delta: isize) {
        if let Some(rep) = &self.report {
            let n = rep.numeric_cols.len();
            if n > 0 {
                let cur = self.analysis_hist_idx as isize;
                self.analysis_hist_idx = (cur + delta).rem_euclid(n as isize) as usize;
                self.analysis_scroll = 0;
                self.rebuild_report();
            }
        }
    }

    fn page_scroll(&mut self, delta: isize) {
        let rows = self.table.row_count();
        if rows == 0 {
            return;
        }
        let new = (self.selected as isize + delta).clamp(0, rows as isize - 1) as usize;
        self.selected = new;
    }

    /// Keep the selected row inside the visible window.
    pub fn ensure_selected_visible(&mut self, viewport_rows: usize) {
        if viewport_rows == 0 {
            return;
        }
        let rows = self.table.row_count();
        if rows == 0 {
            return;
        }
        if self.selected < self.scroll_row {
            self.scroll_row = self.selected;
        } else if self.selected >= self.scroll_row + viewport_rows {
            self.scroll_row = self.selected - viewport_rows + 1;
        }
    }

    /// Build the wrapped detail lines for the currently selected row.
    pub fn build_detail(&mut self, width: usize) {
        self.detail_lines = detail_for_row(&self.table, self.selected, width);
    }

    /// Handle a key event. Returns `true` when the application should quit.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if let KeyCode::Char('c') | KeyCode::Char('C') = key.code {
                return true;
            }
        }

        if self.show_help {
            self.show_help = false;
            return false;
        }

        match self.focus {
            Focus::Query => self.handle_query_key(key),
            Focus::Export => self.handle_export_key(key),
            Focus::Sort => self.handle_sort_key(key),
            Focus::Analysis => self.handle_analysis_key(key),
            Focus::Table => {
                if self.handle_table_key(key) {
                    return true;
                }
            }
        }
        false
    }

    fn handle_table_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('/') | KeyCode::Char('i') => {
                self.focus = Focus::Query;
                self.query_editor.end();
            }
            KeyCode::Char('m') | KeyCode::Tab => {
                self.mode = self.mode.next();
                self.set_status_ok(format!("Mode: {}", self.mode.label()));
            }
            KeyCode::Char('s') => {
                self.focus = Focus::Sort;
                self.sort_selected = 0;
            }
            KeyCode::Char('a') => self.open_analysis(),
            KeyCode::Char('e') => {
                self.export_editor.set_content(String::new());
                self.focus = Focus::Export;
            }
            KeyCode::Char('r') => self.reload(),
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Enter => self.run_query(),
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let last = self.table.row_count().saturating_sub(1);
                if self.selected < last {
                    self.selected += 1;
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.scroll_col = self.scroll_col.saturating_sub(1);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let last = self.table.col_count().saturating_sub(1);
                if self.scroll_col < last {
                    self.scroll_col += 1;
                }
            }
            KeyCode::PageUp => self.page_scroll(-(self.body_rows.max(1) as isize)),
            KeyCode::PageDown => self.page_scroll(self.body_rows.max(1) as isize),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.table.row_count().saturating_sub(1),
            _ => {}
        }
        false
    }

    fn handle_query_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.run_query();
                self.focus = Focus::Table;
            }
            KeyCode::Esc => {
                if self.query_editor.is_empty() {
                    self.focus = Focus::Table;
                } else {
                    self.query_editor.clear();
                    self.live_fuzzy_if_applicable();
                }
            }
            KeyCode::Backspace => {
                self.query_editor.backspace();
                self.live_fuzzy_if_applicable();
            }
            KeyCode::Delete => {
                self.query_editor.delete();
                self.live_fuzzy_if_applicable();
            }
            KeyCode::Left => self.query_editor.move_left(),
            KeyCode::Right => self.query_editor.move_right(),
            KeyCode::Home => self.query_editor.home(),
            KeyCode::End => self.query_editor.end(),
            KeyCode::Char(c) => {
                self.query_editor.insert_char(c);
                self.live_fuzzy_if_applicable();
            }
            _ => {}
        }
    }

    /// In Fuzzy mode, re-run the query live as the user types so results are
    /// reflected in real time. SQL modes run explicitly on Enter.
    fn live_fuzzy_if_applicable(&mut self) {
        if self.mode != QueryMode::Fuzzy {
            return;
        }
        let q = self.query_editor.content.clone();
        if let Ok(df) = query::fuzzy(&self.df, &q) {
            self.filtered = df;
            self.refresh_result();
            self.set_status_ok(format!(
                "[Fuzzy] {} → {} matches / {} rows",
                if q.trim().is_empty() { "(all)" } else { &q },
                self.table.row_count(),
                self.df.height()
            ));
        }
    }

    fn handle_export_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.export();
                self.focus = Focus::Table;
            }
            KeyCode::Esc => self.focus = Focus::Table,
            KeyCode::Backspace => self.export_editor.backspace(),
            KeyCode::Delete => self.export_editor.delete(),
            KeyCode::Left => self.export_editor.move_left(),
            KeyCode::Right => self.export_editor.move_right(),
            KeyCode::Home => self.export_editor.home(),
            KeyCode::End => self.export_editor.end(),
            KeyCode::Char(c) => self.export_editor.insert_char(c),
            _ => {}
        }
    }

    fn handle_sort_key(&mut self, key: KeyEvent) {
        let last = self.table.col_count().saturating_sub(1);
        match key.code {
            KeyCode::Esc => self.focus = Focus::Table,
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Left | KeyCode::Char('h') => {
                self.sort_selected = self.sort_selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Right | KeyCode::Char('l') => {
                if self.sort_selected < last {
                    self.sort_selected += 1;
                }
            }
            KeyCode::Enter => self.toggle_sort_key(self.sort_selected),
            KeyCode::Char('x') => {
                self.sort_keys.clear();
                self.refresh_result();
                self.selected = 0;
                self.scroll_row = 0;
                self.set_status_ok("Sort cleared".to_string());
            }
            _ => {}
        }
    }

    fn handle_analysis_key(&mut self, key: KeyEvent) {
        let page = self.body_rows.max(1);
        match key.code {
            KeyCode::Esc => self.focus = Focus::Table,
            KeyCode::Up | KeyCode::Char('k') => {
                self.analysis_scroll = self.analysis_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => self.analysis_scroll += 1,
            KeyCode::Left | KeyCode::Char('h') => self.shift_hist(-1),
            KeyCode::Right | KeyCode::Char('l') => self.shift_hist(1),
            KeyCode::PageUp => self.analysis_scroll = self.analysis_scroll.saturating_sub(page),
            KeyCode::PageDown => self.analysis_scroll += page,
            KeyCode::Home => self.analysis_scroll = 0,
            _ => {}
        }
    }
}

/// Render the currently selected row as `name = value` pairs, wrapped so that
/// every column is visible at once.
fn detail_for_row(td: &TableData, selected: usize, width: usize) -> Vec<String> {
    if td.row_count() == 0 || selected >= td.row_count() {
        return vec![String::new()];
    }
    let header = format!("Row {}/{}:", selected + 1, td.row_count());
    let mut parts = Vec::with_capacity(td.col_count());
    for (i, h) in td.headers.iter().enumerate() {
        let v = &td.rows[selected][i];
        let val = if v.is_empty() { "(null)".to_string() } else { v.clone() };
        parts.push(format!("{h} = {val}"));
    }
    let body = parts.join("  │  ");
    let text = format!("{header} {body}");
    wrap_text(&text, width)
}

/// Greedy word-wrap that also hard-breaks words longer than the width, so no
/// content is ever hidden.
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();

    for word in text.split_whitespace() {
        let mut remaining = word;
        loop {
            let cur_len = cur.chars().count();
            let leading = if cur_len == 0 { 0 } else { 1 };
            let remaining_len = remaining.chars().count();

            if cur_len + leading + remaining_len <= width {
                if leading == 1 {
                    cur.push(' ');
                }
                cur.push_str(remaining);
                break;
            } else if cur_len == 0 && remaining_len > width {
                // Hard-break an over-long token.
                let head: String = remaining.chars().take(width).collect();
                let head_len = head.len();
                lines.push(head);
                remaining = &remaining[head_len..];
            } else {
                lines.push(std::mem::take(&mut cur));
            }
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = "\
id,name,department,age,salary,country,score\n\
1,Alice,Engineering,45,12000.50,US,92.3\n\
2,Bob,Sales,38,8500.00,UK,78.0\n\
3,Carol,Engineering,52,15000.00,US,88.9\n\
4,Dan,Sales,29,6200.75,Canada,65.5\n\
5,Eve,Marketing,41,9300.00,US,71.2\n";

    fn make_app(tag: &str) -> App {
        let path = std::env::temp_dir().join(format!(
            "toolk_app_{}_{}.csv",
            std::process::id(),
            tag
        ));
        std::fs::write(&path, CSV).unwrap();
        let app = App::new(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        app
    }

    #[test]
    fn full_query_sort_export_workflow() {
        let mut a = make_app("wf");
        assert_eq!(a.table.row_count(), 5);
        assert_eq!(a.table.col_count(), 7);

        a.mode = QueryMode::Fuzzy;
        a.query_editor.set_content("Sales");
        a.run_query();
        assert_eq!(a.table.row_count(), 2);

        a.mode = QueryMode::SqlLike;
        a.query_editor.set_content("select where age > 40");
        a.run_query();
        assert_eq!(a.table.row_count(), 3);

        a.mode = QueryMode::Sql;
        a.query_editor
            .set_content("select * from df where country = 'US' and score > 80");
        a.run_query();
        assert_eq!(a.table.row_count(), 2);

        // Sort by age ascending (index 3), then check first row is the youngest.
        a.toggle_sort_key(3);
        let first_age = a.result.column("age").unwrap().get(0).unwrap().to_string();
        assert_eq!(first_age, "45");

        // Export the current result.
        let out = std::env::temp_dir().join(format!("toolk_export_{}.csv", std::process::id()));
        a.export_editor.set_content(out.to_str().unwrap().to_string());
        a.export();
        let content = std::fs::read_to_string(&out).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 rows
        assert!(lines[0].contains("id") && lines[0].contains("score"));
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn report_has_correlation_identity() {
        let a = make_app("rep");
        let r = Report::for_df(&a.result, 0);
        assert!(r.numeric_cols.len() >= 4); // age, salary, score (+ id)
        assert!(!r.lines.is_empty());
        // Correlation matrix diagonal must render as exactly "1.00".
        assert!(r.lines.iter().any(|l| l.contains("1.00")));
    }
}
