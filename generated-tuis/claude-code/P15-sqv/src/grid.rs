//! The data grid model: all loaded rows plus the derived view (filter → sort)
//! and the cursor that moves over it.
//!
//! Filtering and sorting happen in memory over the full row set rather than by
//! re-querying, so a filter never loses the rows it hides and sorting a view
//! (which has no stable `ORDER BY` guarantee) behaves the same as a table.

use anyhow::Result;

use crate::db::{ColumnInfo, Row, RowKey};
use crate::filter::Filter;
use crate::value::Value;

/// Sort direction for the active sort column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

impl SortDir {
    pub fn flip(self) -> SortDir {
        match self {
            SortDir::Asc => SortDir::Desc,
            SortDir::Desc => SortDir::Asc,
        }
    }

    /// Arrow drawn next to the sorted column header.
    pub fn arrow(self) -> &'static str {
        match self {
            SortDir::Asc => "▲",
            SortDir::Desc => "▼",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortDir::Asc => "ascending",
            SortDir::Desc => "descending",
        }
    }
}

/// The active sort, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub column: usize,
    pub dir: SortDir,
}

/// A table (or query result) being browsed.
pub struct Grid {
    /// Which object these rows came from; `None` for ad-hoc query results.
    pub source: Option<String>,
    pub columns: Vec<ColumnInfo>,
    /// Every row loaded, in database order. Never reordered — [`Grid::view`]
    /// holds indices into this.
    rows: Vec<Row>,
    /// Indices into `rows`, after filtering and sorting.
    view: Vec<usize>,
    filters: Vec<Filter>,
    sort: Option<Sort>,
    /// Cursor position within `view`.
    cursor: usize,
    /// Selected column, used by sort/filter/edit and for horizontal scrolling.
    pub col_cursor: usize,
    /// First visible row of `view`; maintained by [`Grid::scroll_into_view`].
    pub row_offset: usize,
    /// First visible column; keeps the selected column on screen.
    pub col_offset: usize,
}

impl Grid {
    pub fn new(source: Option<String>, columns: Vec<ColumnInfo>, rows: Vec<Row>) -> Grid {
        let view = (0..rows.len()).collect();
        Grid {
            source,
            columns,
            rows,
            view,
            filters: Vec::new(),
            sort: None,
            cursor: 0,
            col_cursor: 0,
            row_offset: 0,
            col_offset: 0,
        }
    }

    /// Rebuild from freshly loaded rows, keeping filters, sort and — where
    /// possible — the cursor's row identity.
    pub fn reload(&mut self, columns: Vec<ColumnInfo>, rows: Vec<Row>) {
        let anchor = self.current_row().map(|r| r.key.clone());
        let same_shape = columns.len() == self.columns.len()
            && columns
                .iter()
                .zip(&self.columns)
                .all(|(a, b)| a.name == b.name);
        self.columns = columns;
        self.rows = rows;
        if !same_shape {
            self.filters.clear();
            self.sort = None;
            self.col_cursor = 0;
            self.col_offset = 0;
        }
        self.recompute();
        if let Some(key) = anchor {
            if key != RowKey::None {
                if let Some(pos) = self.view.iter().position(|&i| self.rows[i].key == key) {
                    self.cursor = pos;
                }
            }
        }
        self.clamp_cursor();
    }

    pub fn total_rows(&self) -> usize {
        self.rows.len()
    }

    /// Number of rows passing the current filters.
    pub fn visible_rows(&self) -> usize {
        self.view.len()
    }

    pub fn is_empty(&self) -> bool {
        self.view.is_empty()
    }

    pub fn filters(&self) -> &[Filter] {
        &self.filters
    }

    pub fn sort(&self) -> Option<Sort> {
        self.sort
    }

    /// Cursor index within the filtered view.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn column_names(&self) -> Vec<&str> {
        self.columns.iter().map(|c| c.name.as_str()).collect()
    }

    /// Row at view position `i`.
    pub fn row_at(&self, i: usize) -> Option<&Row> {
        self.view.get(i).map(|&idx| &self.rows[idx])
    }

    pub fn current_row(&self) -> Option<&Row> {
        self.row_at(self.cursor)
    }

    pub fn current_cell(&self) -> Option<&Value> {
        self.current_row()?.cells.get(self.col_cursor)
    }

    pub fn current_column(&self) -> Option<&ColumnInfo> {
        self.columns.get(self.col_cursor)
    }

    /// Iterate over the visible rows, in view order.
    pub fn iter_view(&self) -> impl Iterator<Item = &Row> + '_ {
        self.view.iter().map(move |&i| &self.rows[i])
    }

    // ---- filtering -------------------------------------------------------

    /// Add a filter, replacing any existing filter on the same column so that
    /// re-filtering a column refines rather than accumulates.
    pub fn set_filter(&mut self, filter: Filter) {
        self.filters.retain(|f| f.column != filter.column);
        self.filters.push(filter);
        self.recompute();
        self.cursor = 0;
        self.row_offset = 0;
    }

    /// Drop the filter on `column`, if there is one. Returns whether it existed.
    pub fn clear_filter_on(&mut self, column: usize) -> bool {
        let before = self.filters.len();
        self.filters.retain(|f| f.column != column);
        let removed = self.filters.len() != before;
        if removed {
            self.recompute();
            self.clamp_cursor();
        }
        removed
    }

    /// Drop every filter. Returns how many were removed.
    pub fn clear_filters(&mut self) -> usize {
        let n = self.filters.len();
        if n > 0 {
            self.filters.clear();
            self.recompute();
            self.clamp_cursor();
        }
        n
    }

    pub fn filter_on(&self, column: usize) -> Option<&Filter> {
        self.filters.iter().find(|f| f.column == column)
    }

    // ---- sorting ---------------------------------------------------------

    /// Sort by `column`. Selecting the already-sorted column flips direction.
    pub fn sort_by(&mut self, column: usize, dir: SortDir) {
        self.sort = Some(Sort { column, dir });
        self.recompute();
        self.cursor = 0;
        self.row_offset = 0;
    }

    /// Toggle the sort on `column`: ascending first, then descending.
    pub fn toggle_sort(&mut self, column: usize) -> SortDir {
        let dir = match self.sort {
            Some(s) if s.column == column => s.dir.flip(),
            _ => SortDir::Asc,
        };
        self.sort_by(column, dir);
        dir
    }

    /// Return to database order.
    pub fn clear_sort(&mut self) -> bool {
        if self.sort.is_none() {
            return false;
        }
        self.sort = None;
        self.recompute();
        self.clamp_cursor();
        true
    }

    /// Recompute `view` from `rows`: filter, then sort.
    fn recompute(&mut self) {
        let filters = &self.filters;
        let rows = &self.rows;
        self.view = (0..rows.len())
            .filter(|&i| {
                filters
                    .iter()
                    .all(|f| rows[i].cells.get(f.column).is_some_and(|c| f.matches(c)))
            })
            .collect();

        if let Some(Sort { column, dir }) = self.sort {
            let view = &mut self.view;
            // Stable sort, so rows comparing equal keep database order — which
            // makes "first row after sorting" deterministic.
            view.sort_by(|&a, &b| {
                let av = rows[a].cells.get(column);
                let bv = rows[b].cells.get(column);
                let ord = match (av, bv) {
                    (Some(x), Some(y)) => x.sqlite_cmp(y),
                    _ => std::cmp::Ordering::Equal,
                };
                match dir {
                    SortDir::Asc => ord,
                    SortDir::Desc => ord.reverse(),
                }
            });
        }
    }

    // ---- cursor movement -------------------------------------------------

    fn clamp_cursor(&mut self) {
        if self.view.is_empty() {
            self.cursor = 0;
            self.row_offset = 0;
        } else if self.cursor >= self.view.len() {
            self.cursor = self.view.len() - 1;
        }
        if self.col_cursor >= self.columns.len() {
            self.col_cursor = self.columns.len().saturating_sub(1);
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        if self.view.is_empty() {
            return;
        }
        let last = self.view.len() as isize - 1;
        let next = (self.cursor as isize + delta).clamp(0, last);
        self.cursor = next as usize;
    }

    pub fn cursor_to(&mut self, i: usize) {
        if !self.view.is_empty() {
            self.cursor = i.min(self.view.len() - 1);
        }
    }

    pub fn cursor_first(&mut self) {
        self.cursor = 0;
    }

    pub fn cursor_last(&mut self) {
        self.cursor = self.view.len().saturating_sub(1);
    }

    pub fn move_col(&mut self, delta: isize) {
        if self.columns.is_empty() {
            return;
        }
        let last = self.columns.len() as isize - 1;
        self.col_cursor = (self.col_cursor as isize + delta).clamp(0, last) as usize;
    }

    pub fn col_first(&mut self) {
        self.col_cursor = 0;
    }

    pub fn col_last(&mut self) {
        self.col_cursor = self.columns.len().saturating_sub(1);
    }

    /// Keep the cursor row inside a viewport `height` rows tall.
    pub fn scroll_into_view(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.cursor < self.row_offset {
            self.row_offset = self.cursor;
        } else if self.cursor >= self.row_offset + height {
            self.row_offset = self.cursor + 1 - height;
        }
        let max_offset = self.view.len().saturating_sub(height);
        self.row_offset = self.row_offset.min(max_offset);
    }

    /// Apply a cell edit that has already been written to the database.
    ///
    /// `cells` is the row as re-read from SQLite, so the grid shows what was
    /// actually stored. Re-runs filter/sort because the edit may have moved the
    /// row — the cursor follows it.
    pub fn apply_row_update(&mut self, cells: Vec<Value>) -> Result<()> {
        let Some(&idx) = self.view.get(self.cursor) else {
            anyhow::bail!("no row selected");
        };
        self.rows[idx].cells = cells;
        // The row's own primary key may have changed; refresh the key too.
        if let RowKey::Primary(pairs) = &self.rows[idx].key {
            let names: Vec<String> = pairs.iter().map(|(n, _)| n.clone()).collect();
            let refreshed = names
                .into_iter()
                .map(|n| {
                    let ci = self.columns.iter().position(|c| c.name == n);
                    let v = ci
                        .and_then(|i| self.rows[idx].cells.get(i))
                        .cloned()
                        .unwrap_or(Value::Null);
                    (n, v)
                })
                .collect();
            self.rows[idx].key = RowKey::Primary(refreshed);
        }
        self.recompute();
        // Follow the edited row if it is still visible, otherwise stay put.
        if let Some(pos) = self.view.iter().position(|&i| i == idx) {
            self.cursor = pos;
        } else {
            self.clamp_cursor();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::MatchMode;

    fn col(name: &str, ty: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            decl_type: ty.into(),
            not_null: false,
            default: None,
            pk_index: 0,
        }
    }

    /// name / salary / dept, deliberately unsorted, with a NULL salary.
    fn grid() -> Grid {
        let cols = vec![col("name", "TEXT"), col("salary", "REAL"), col("dept", "TEXT")];
        let data: Vec<(&str, Option<f64>, &str)> = vec![
            ("Linus", Some(99000.0), "ops"),
            ("Grace", Some(145000.0), "eng"),
            ("Ada", Some(120000.0), "eng"),
            ("Nobody", None, "ops"),
        ];
        let rows = data
            .into_iter()
            .enumerate()
            .map(|(i, (n, s, d))| Row {
                key: RowKey::Rowid(i as i64 + 1),
                cells: vec![
                    Value::Text(n.into()),
                    s.map(Value::Real).unwrap_or(Value::Null),
                    Value::Text(d.into()),
                ],
            })
            .collect();
        Grid::new(Some("employees".into()), cols, rows)
    }

    fn names(g: &Grid) -> Vec<String> {
        g.iter_view().map(|r| r.cells[0].display()).collect()
    }

    #[test]
    fn starts_in_database_order_with_no_sort() {
        let g = grid();
        assert_eq!(names(&g), vec!["Linus", "Grace", "Ada", "Nobody"]);
        assert!(g.sort().is_none());
        assert_eq!(g.visible_rows(), 4);
    }

    #[test]
    fn sort_ascending_puts_nulls_first_like_sqlite() {
        let mut g = grid();
        g.sort_by(1, SortDir::Asc);
        assert_eq!(names(&g), vec!["Nobody", "Linus", "Ada", "Grace"]);
        // Extreme-value row is where the cursor lands.
        assert_eq!(g.cursor(), 0);
        assert!(g.current_row().unwrap().cells[1].is_null());
    }

    #[test]
    fn sort_descending_reverses() {
        let mut g = grid();
        g.sort_by(1, SortDir::Desc);
        assert_eq!(names(&g), vec!["Grace", "Ada", "Linus", "Nobody"]);
        assert_eq!(g.current_row().unwrap().cells[0], Value::Text("Grace".into()));
    }

    #[test]
    fn toggle_sort_cycles_asc_then_desc_on_same_column() {
        let mut g = grid();
        assert_eq!(g.toggle_sort(0), SortDir::Asc);
        assert_eq!(names(&g)[0], "Ada");
        assert_eq!(g.toggle_sort(0), SortDir::Desc);
        assert_eq!(names(&g)[0], "Nobody");
        // A different column restarts at ascending.
        assert_eq!(g.toggle_sort(2), SortDir::Asc);
    }

    #[test]
    fn clear_sort_restores_database_order() {
        let mut g = grid();
        g.sort_by(0, SortDir::Asc);
        assert!(g.clear_sort());
        assert_eq!(names(&g), vec!["Linus", "Grace", "Ada", "Nobody"]);
        assert!(!g.clear_sort());
    }

    #[test]
    fn sort_is_stable_for_equal_keys() {
        let mut g = grid();
        g.sort_by(2, SortDir::Asc); // dept: eng, eng, ops, ops
        assert_eq!(names(&g), vec!["Grace", "Ada", "Linus", "Nobody"]);
    }

    #[test]
    fn filter_narrows_view_and_reports_count() {
        let mut g = grid();
        g.set_filter(Filter::new(2, "dept", MatchMode::Exact, "eng", false).unwrap());
        assert_eq!(g.visible_rows(), 2);
        assert_eq!(g.total_rows(), 4);
        assert_eq!(names(&g), vec!["Grace", "Ada"]);
    }

    #[test]
    fn regex_filter_selects_rows() {
        let mut g = grid();
        g.set_filter(Filter::new(0, "name", MatchMode::Regex, "^[AG]", false).unwrap());
        assert_eq!(names(&g), vec!["Grace", "Ada"]);
    }

    #[test]
    fn filters_on_different_columns_are_conjunctive() {
        let mut g = grid();
        g.set_filter(Filter::new(2, "dept", MatchMode::Exact, "eng", false).unwrap());
        g.set_filter(Filter::new(0, "name", MatchMode::Contains, "ad", false).unwrap());
        assert_eq!(names(&g), vec!["Ada"]);
        assert_eq!(g.filters().len(), 2);
    }

    #[test]
    fn refiltering_same_column_replaces_not_accumulates() {
        let mut g = grid();
        g.set_filter(Filter::new(2, "dept", MatchMode::Exact, "eng", false).unwrap());
        g.set_filter(Filter::new(2, "dept", MatchMode::Exact, "ops", false).unwrap());
        assert_eq!(g.filters().len(), 1);
        assert_eq!(names(&g), vec!["Linus", "Nobody"]);
    }

    #[test]
    fn clearing_filters_restores_all_rows() {
        let mut g = grid();
        g.set_filter(Filter::new(2, "dept", MatchMode::Exact, "eng", false).unwrap());
        assert_eq!(g.clear_filters(), 1);
        assert_eq!(g.visible_rows(), 4);
        assert_eq!(g.clear_filters(), 0);
    }

    #[test]
    fn clear_filter_on_specific_column() {
        let mut g = grid();
        g.set_filter(Filter::new(2, "dept", MatchMode::Exact, "eng", false).unwrap());
        g.set_filter(Filter::new(0, "name", MatchMode::Contains, "a", false).unwrap());
        assert!(g.clear_filter_on(2));
        assert!(!g.clear_filter_on(2));
        assert_eq!(g.filters().len(), 1);
    }

    #[test]
    fn filter_and_sort_compose() {
        let mut g = grid();
        g.set_filter(Filter::new(2, "dept", MatchMode::Contains, "eng", false).unwrap());
        g.sort_by(1, SortDir::Desc);
        assert_eq!(names(&g), vec!["Grace", "Ada"]);
    }

    #[test]
    fn filter_that_matches_nothing_empties_view_safely() {
        let mut g = grid();
        g.set_filter(Filter::new(0, "name", MatchMode::Exact, "zzz", false).unwrap());
        assert_eq!(g.visible_rows(), 0);
        assert!(g.is_empty());
        assert!(g.current_row().is_none());
        g.move_cursor(5);
        assert_eq!(g.cursor(), 0);
    }

    #[test]
    fn cursor_is_clamped_at_both_ends() {
        let mut g = grid();
        g.move_cursor(-10);
        assert_eq!(g.cursor(), 0);
        g.move_cursor(100);
        assert_eq!(g.cursor(), 3);
        g.cursor_first();
        assert_eq!(g.cursor(), 0);
        g.cursor_last();
        assert_eq!(g.cursor(), 3);
    }

    #[test]
    fn column_cursor_is_clamped() {
        let mut g = grid();
        g.move_col(-1);
        assert_eq!(g.col_cursor, 0);
        g.move_col(99);
        assert_eq!(g.col_cursor, 2);
        g.col_first();
        assert_eq!(g.col_cursor, 0);
        g.col_last();
        assert_eq!(g.col_cursor, 2);
    }

    #[test]
    fn scroll_follows_cursor_within_viewport() {
        let mut g = grid();
        g.cursor_to(3);
        g.scroll_into_view(2);
        assert_eq!(g.row_offset, 2);
        g.cursor_to(0);
        g.scroll_into_view(2);
        assert_eq!(g.row_offset, 0);
    }

    #[test]
    fn scroll_offset_never_exceeds_row_count() {
        let mut g = grid();
        g.cursor_to(3);
        g.scroll_into_view(10);
        assert_eq!(g.row_offset, 0);
    }

    #[test]
    fn applying_edit_updates_row_in_place() {
        let mut g = grid();
        g.cursor_to(0);
        g.apply_row_update(vec![
            Value::Text("Linus".into()),
            Value::Real(101000.0),
            Value::Text("ops".into()),
        ])
        .unwrap();
        assert_eq!(g.current_row().unwrap().cells[1], Value::Real(101000.0));
    }

    #[test]
    fn cursor_follows_edited_row_when_sort_moves_it() {
        let mut g = grid();
        g.sort_by(1, SortDir::Desc); // Grace, Ada, Linus, Nobody
        g.cursor_to(2); // Linus
        assert_eq!(g.current_row().unwrap().cells[0], Value::Text("Linus".into()));
        g.apply_row_update(vec![
            Value::Text("Linus".into()),
            Value::Real(999999.0),
            Value::Text("ops".into()),
        ])
        .unwrap();
        // Linus now sorts first, and the cursor moved with him.
        assert_eq!(g.cursor(), 0);
        assert_eq!(g.current_row().unwrap().cells[0], Value::Text("Linus".into()));
    }

    #[test]
    fn edited_row_hidden_by_filter_leaves_cursor_valid() {
        let mut g = grid();
        g.set_filter(Filter::new(2, "dept", MatchMode::Exact, "ops", false).unwrap());
        g.cursor_to(0); // Linus
        g.apply_row_update(vec![
            Value::Text("Linus".into()),
            Value::Real(99000.0),
            Value::Text("eng".into()),
        ])
        .unwrap();
        assert_eq!(g.visible_rows(), 1);
        assert!(g.current_row().is_some());
        assert_eq!(g.current_row().unwrap().cells[0], Value::Text("Nobody".into()));
    }

    #[test]
    fn reload_keeps_filter_sort_and_row_identity() {
        let mut g = grid();
        g.sort_by(0, SortDir::Asc);
        g.set_filter(Filter::new(2, "dept", MatchMode::Exact, "eng", false).unwrap());
        g.cursor_to(1); // Grace, sorted asc within eng: Ada, Grace
        let key = g.current_row().unwrap().key.clone();

        let cols = g.columns.clone();
        let rows: Vec<Row> = g.rows.clone();
        g.reload(cols, rows);
        assert_eq!(g.filters().len(), 1);
        assert_eq!(g.sort().unwrap().column, 0);
        assert_eq!(g.current_row().unwrap().key, key);
    }

    #[test]
    fn reload_with_different_columns_resets_filters() {
        let mut g = grid();
        g.set_filter(Filter::new(2, "dept", MatchMode::Exact, "eng", false).unwrap());
        g.reload(vec![col("other", "INTEGER")], vec![]);
        assert!(g.filters().is_empty());
        assert!(g.sort().is_none());
        assert_eq!(g.col_cursor, 0);
    }
}
