//! Data layer: real CSV I/O, querying, sorting and statistics via Polars.
//!
//! Everything in this module operates on whatever columns the file actually
//! has — headers are read from the CSV and no column name is ever assumed.

use std::path::{Path, PathBuf};

use polars::prelude::*;
use polars::sql::SQLContext;

use crate::fmtnum::{self, CellFormat, NumFormat};

/// Broad classification of a column, used to pick comparison and analysis
/// behaviour without caring about the exact Polars dtype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Numeric,
    Text,
    Boolean,
    Temporal,
    Other,
}

impl ColumnKind {
    pub fn of(dt: &DataType) -> Self {
        if dt.is_primitive_numeric() {
            ColumnKind::Numeric
        } else if matches!(dt, DataType::String) {
            ColumnKind::Text
        } else if matches!(dt, DataType::Boolean) {
            ColumnKind::Boolean
        } else if dt.is_temporal() {
            ColumnKind::Temporal
        } else {
            ColumnKind::Other
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            ColumnKind::Numeric => "num",
            ColumnKind::Text => "text",
            ColumnKind::Boolean => "bool",
            ColumnKind::Temporal => "time",
            ColumnKind::Other => "other",
        }
    }

    pub fn is_numeric(self) -> bool {
        matches!(self, ColumnKind::Numeric)
    }
}

/// Find a column by name, case-insensitively, tolerating surrounding spaces.
///
/// Returns the canonical name as it appears in the file, so downstream Polars
/// expressions always use the real header.
pub fn resolve_column(
    columns: &[(String, ColumnKind)],
    name: &str,
) -> Option<(String, ColumnKind)> {
    let want = name.trim();
    if let Some((n, k)) = columns.iter().find(|(n, _)| n == want) {
        return Some((n.clone(), *k));
    }
    let lower = want.to_ascii_lowercase();
    if let Some((n, k)) = columns
        .iter()
        .find(|(n, _)| n.to_ascii_lowercase() == lower)
    {
        return Some((n.clone(), *k));
    }
    // Also allow underscores and spaces to be used interchangeably.
    let norm = |s: &str| {
        s.to_ascii_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
    let target = norm(want);
    if target.is_empty() {
        return None;
    }
    columns
        .iter()
        .find(|(n, _)| norm(n) == target)
        .map(|(n, k)| (n.clone(), *k))
}

/// Sort direction for one key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

impl SortDir {
    pub fn arrow(self) -> &'static str {
        match self {
            SortDir::Asc => "▲",
            SortDir::Desc => "▼",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortDir::Asc => "asc",
            SortDir::Desc => "desc",
        }
    }

    pub fn flip(self) -> Self {
        match self {
            SortDir::Asc => SortDir::Desc,
            SortDir::Desc => SortDir::Asc,
        }
    }
}

/// One key in a multi-column sort.
#[derive(Debug, Clone)]
pub struct SortKey {
    pub column: String,
    pub dir: SortDir,
}

/// The loaded dataset plus the current derived view.
pub struct Dataset {
    /// Path the data was loaded from.
    pub path: PathBuf,
    /// The full, unfiltered frame as read from disk.
    base: DataFrame,
    /// The current result of query + sort, i.e. what the table shows.
    view: DataFrame,
    /// Column names with their kinds, in file order.
    columns: Vec<(String, ColumnKind)>,
    /// Active sort keys, applied in order.
    pub sort_keys: Vec<SortKey>,
    /// Lowercased cell text of every base row, built on the first fuzzy search
    /// and reused afterwards. Re-rendering 200k x 30 cells on each keystroke is
    /// what makes live search feel slow, so it is only paid once per file.
    search_index: Option<Vec<Vec<String>>>,
}

impl Dataset {
    /// Load a CSV file from disk. Headers are taken from the first row.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Err(format!("file not found: {}", path.display()));
        }
        if path.is_dir() {
            return Err(format!("{} is a directory, not a CSV file", path.display()));
        }

        let df = read_csv(path)?;
        let columns = column_kinds(&df);
        if columns.is_empty() {
            return Err(format!("{} has no columns", path.display()));
        }

        Ok(Dataset {
            path: path.to_path_buf(),
            view: df.clone(),
            base: df,
            columns,
            sort_keys: Vec::new(),
            search_index: None,
        })
    }

    pub fn columns(&self) -> &[(String, ColumnKind)] {
        &self.columns
    }

    /// Number of rows in the source file.
    pub fn total_rows(&self) -> usize {
        self.base.height()
    }

    /// Number of rows in the current view.
    pub fn view_rows(&self) -> usize {
        self.view.height()
    }

    /// Column names of the current view, which may be a projection subset.
    pub fn view_columns(&self) -> Vec<String> {
        self.view
            .get_column_names()
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// Kind of a column in the current view.
    pub fn view_column_kind(&self, name: &str) -> ColumnKind {
        self.view
            .schema()
            .get(name)
            .map(ColumnKind::of)
            .unwrap_or(ColumnKind::Other)
    }

    /// Replace the view with `df` and re-apply the active sort.
    fn set_view(&mut self, df: DataFrame) -> Result<(), String> {
        self.view = df;
        self.apply_sort()
    }

    /// Reset the view to the full dataset.
    pub fn clear_query(&mut self) -> Result<(), String> {
        let base = self.base.clone();
        self.set_view(base)
    }

    /// Rank rows by fuzzy score, best first, for the incremental search list.
    pub fn fuzzy_ranked(&mut self, needle: &str) -> Result<usize, String> {
        let needle = needle.trim();
        if needle.is_empty() {
            self.clear_query()?;
            return Ok(self.view.height());
        }
        // The needle is lowercased once here, to match the cached index.
        let lowered = needle.to_lowercase();
        let terms: Vec<&str> = lowered.split_whitespace().collect();
        let index = self.search_index();
        let mut scored: Vec<(i64, IdxSize)> = Vec::new();
        for (i, cells) in index.iter().enumerate() {
            if let Some(score) = score_row(cells, &terms) {
                scored.push((score, i as IdxSize));
            }
        }
        // Highest score first; ties keep file order for a stable display.
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let idx: Vec<IdxSize> = scored.iter().map(|(_, i)| *i).collect();
        let n = idx.len();
        let taken = take_rows(&self.base, &idx)?;
        // A ranked search defines its own order, so skip the column sort.
        self.view = taken;
        Ok(n)
    }

    /// Lowercased text of every base cell, built once per loaded file.
    fn search_index(&mut self) -> &Vec<Vec<String>> {
        if self.search_index.is_none() {
            let cols: Vec<Vec<String>> = self
                .base
                .columns()
                .iter()
                .map(|c| {
                    let s = c.as_materialized_series();
                    (0..s.len())
                        .map(|i| match s.get(i) {
                            Ok(av) => any_value_text(&av, CellFormat::Auto).to_lowercase(),
                            Err(_) => String::new(),
                        })
                        .collect()
                })
                .collect();
            let h = self.base.height();
            let mut rows = Vec::with_capacity(h);
            for i in 0..h {
                rows.push(
                    cols.iter()
                        .map(|c| c.get(i).cloned().unwrap_or_default())
                        .collect(),
                );
            }
            self.search_index = Some(rows);
        }
        self.search_index.as_ref().expect("just built")
    }

    /// SQL-Like query: `select where <condition>`.
    pub fn query_sql_like(&mut self, query: &str) -> Result<usize, String> {
        let parsed = crate::sqllike::parse(query, &self.columns)?;
        let mut lf = self.base.clone().lazy();
        if let Some(pred) = parsed.predicate {
            lf = lf.filter(pred);
        }
        if !parsed.projection.is_empty() {
            let cols: Vec<Expr> = parsed
                .projection
                .iter()
                .map(|c| col(c.as_str()))
                .collect();
            lf = lf.select(cols);
        }
        let out = lf.collect().map_err(pretty_polars)?;
        let n = out.height();
        self.set_view(out)?;
        Ok(n)
    }

    /// Standard SQL query against the frame, registered as `df`.
    ///
    /// The table is also registered under the file stem and as `data`, so
    /// `select * from employees` works too.
    pub fn query_sql(&mut self, query: &str) -> Result<usize, String> {
        let sql = query.trim().trim_end_matches(';').trim();
        if sql.is_empty() {
            self.clear_query()?;
            return Ok(self.view.height());
        }

        let mut ctx = SQLContext::new();
        ctx.register("df", self.base.clone().lazy());
        ctx.register("data", self.base.clone().lazy());
        ctx.register("csv", self.base.clone().lazy());
        if let Some(stem) = self.path.file_stem().and_then(|s| s.to_str()) {
            let stem = stem.trim();
            if !stem.is_empty() && stem != "df" {
                ctx.register(stem, self.base.clone().lazy());
            }
        }

        let out = ctx
            .execute(sql)
            .and_then(|lf| lf.collect())
            .map_err(pretty_polars)?;
        let n = out.height();
        self.set_view(out)?;
        Ok(n)
    }

    /// Add or update a sort key. Sorting by an already-sorted column flips it.
    pub fn toggle_sort(&mut self, column: &str) -> Result<SortDir, String> {
        let dir = match self.sort_keys.iter().position(|k| k.column == column) {
            Some(i) => {
                let d = self.sort_keys[i].dir.flip();
                self.sort_keys[i].dir = d;
                d
            }
            None => {
                self.sort_keys.push(SortKey {
                    column: column.to_string(),
                    dir: SortDir::Asc,
                });
                SortDir::Asc
            }
        };
        self.apply_sort()?;
        Ok(dir)
    }

    /// Append `column` as an additional (secondary) sort key.
    pub fn add_sort_key(&mut self, column: &str, dir: SortDir) -> Result<(), String> {
        if let Some(i) = self.sort_keys.iter().position(|k| k.column == column) {
            self.sort_keys[i].dir = dir;
        } else {
            self.sort_keys.push(SortKey {
                column: column.to_string(),
                dir,
            });
        }
        self.apply_sort()
    }

    pub fn clear_sort(&mut self) -> Result<(), String> {
        self.sort_keys.clear();
        Ok(())
    }

    /// Re-apply the active sort keys to the current view.
    pub fn apply_sort(&mut self) -> Result<(), String> {
        if self.sort_keys.is_empty() {
            return Ok(());
        }
        let available = self.view_columns();
        let keys: Vec<&SortKey> = self
            .sort_keys
            .iter()
            .filter(|k| available.iter().any(|c| c == &k.column))
            .collect();
        if keys.is_empty() {
            return Ok(());
        }
        let names: Vec<PlSmallStr> = keys.iter().map(|k| k.column.as_str().into()).collect();
        let desc: Vec<bool> = keys.iter().map(|k| k.dir == SortDir::Desc).collect();
        let opts = SortMultipleOptions::new()
            .with_order_descending_multi(desc)
            .with_nulls_last(true)
            .with_maintain_order(true);
        self.view = self.view.sort(names, opts).map_err(pretty_polars)?;
        Ok(())
    }

    /// Read one cell of the current view as display text.
    pub fn cell(&self, row: usize, column: &str, fmt: CellFormat) -> String {
        match self.view.column(column) {
            Ok(c) => match c.get(row) {
                Ok(av) => any_value_text(&av, fmt),
                Err(_) => String::new(),
            },
            Err(_) => String::new(),
        }
    }

    /// All cells of one row of the view, in view-column order.
    pub fn row_cells(&self, row: usize, fmt: CellFormat) -> Vec<(String, String)> {
        self.view_columns()
            .into_iter()
            .map(|name| {
                let v = self.cell(row, &name, fmt);
                (name, v)
            })
            .collect()
    }

    /// Export the current view to `path` as CSV with a header row.
    ///
    /// Returns the number of data rows written.
    pub fn export_csv(&self, path: &Path) -> Result<usize, String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
            }
        }
        let mut file = std::fs::File::create(path)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        let mut out = self.view.clone();
        CsvWriter::new(&mut file)
            .include_header(true)
            .finish(&mut out)
            .map_err(pretty_polars)?;
        Ok(out.height())
    }

    /// Descriptive statistics for one column of the current view.
    pub fn describe(&self, column: &str, fmt: NumFormat) -> Result<Stats, String> {
        let c = self
            .view
            .column(column)
            .map_err(|_| format!("column `{column}` is not in the current result"))?;
        let s = c.as_materialized_series();
        let kind = ColumnKind::of(s.dtype());
        let n = s.len();
        let nulls = s.null_count();
        let unique = s.n_unique().unwrap_or(0);

        let mut rows: Vec<(String, String)> = Vec::new();
        rows.push(("count".into(), (n - nulls).to_string()));
        rows.push(("missing".into(), nulls.to_string()));
        rows.push(("unique".into(), unique.to_string()));

        if kind.is_numeric() || kind == ColumnKind::Boolean {
            let f = s
                .cast(&DataType::Float64)
                .map_err(|_| format!("`{column}` cannot be read as numbers"))?;
            let ca = f.f64().map_err(pretty_polars)?;
            let q = |p: f64| ca.quantile(p, QuantileMethod::Linear).ok().flatten();
            rows.push(("mean".into(), fmt.apply_opt(ca.mean())));
            rows.push(("std".into(), fmt.apply_opt(ca.std(1))));
            rows.push(("min".into(), fmt.apply_opt(ca.min())));
            rows.push(("p25".into(), fmt.apply_opt(q(0.25))));
            rows.push(("median".into(), fmt.apply_opt(q(0.5))));
            rows.push(("p75".into(), fmt.apply_opt(q(0.75))));
            rows.push(("max".into(), fmt.apply_opt(ca.max())));
            let sum: f64 = ca.sum().unwrap_or(0.0);
            rows.push(("sum".into(), fmt.apply(sum)));
            if let (Some(min), Some(max)) = (ca.min(), ca.max()) {
                rows.push(("range".into(), fmt.apply(max - min)));
            }
            // Population variance is what `std^2` reports here (ddof=1).
            rows.push(("variance".into(), fmt.apply_opt(ca.var(1))));
        } else {
            // Non-numeric columns get length and frequency statistics instead.
            let texts = series_texts(s);
            let non_null: Vec<&String> = texts.iter().filter(|t| !t.is_empty()).collect();
            if !non_null.is_empty() {
                let lens: Vec<f64> = non_null.iter().map(|t| t.chars().count() as f64).collect();
                let mean = lens.iter().sum::<f64>() / lens.len() as f64;
                let min = lens.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = lens.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                rows.push(("mean length".into(), fmt.apply(mean)));
                rows.push(("min length".into(), fmt.apply(min)));
                rows.push(("max length".into(), fmt.apply(max)));
            }
            if let Some((val, count)) = top_value(s) {
                rows.push(("most common".into(), val));
                rows.push(("its count".into(), count.to_string()));
                let share = count as f64 / (n - nulls).max(1) as f64;
                rows.push(("its share".into(), fmtnum::percent_2dp(share)));
            }
        }

        Ok(Stats {
            column: column.to_string(),
            kind,
            rows,
        })
    }

    /// Value distribution for one column, most frequent first.
    pub fn distribution(&self, column: &str, limit: usize) -> Result<Distribution, String> {
        let c = self
            .view
            .column(column)
            .map_err(|_| format!("column `{column}` is not in the current result"))?;
        let s = c.as_materialized_series();
        let total = s.len();

        let kind = ColumnKind::of(s.dtype());
        // A numeric column is far more informative as a histogram once its
        // values stop behaving like categories: either there are more distinct
        // values than we can list, or nearly every row is distinct (continuous
        // data such as a salary or a score).
        let unique = s.n_unique().unwrap_or(0);
        let use_bins = kind.is_numeric()
            && total > 2
            && (unique > limit || unique * 2 > total);

        let mut bars: Vec<Bar> = Vec::new();
        if use_bins {
            let f = s.cast(&DataType::Float64).map_err(pretty_polars)?;
            let ca = f.f64().map_err(pretty_polars)?;
            let vals: Vec<f64> = (0..ca.len()).filter_map(|i| ca.get(i)).collect();
            if vals.is_empty() {
                return Ok(Distribution {
                    column: column.to_string(),
                    total,
                    binned: true,
                    bars,
                });
            }
            let min = vals.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let bins = limit.clamp(2, 20);
            let width = if (max - min).abs() < f64::EPSILON {
                1.0
            } else {
                (max - min) / bins as f64
            };
            let mut counts = vec![0usize; bins];
            for v in &vals {
                let mut b = ((v - min) / width).floor() as isize;
                if b < 0 {
                    b = 0;
                }
                let b = (b as usize).min(bins - 1);
                counts[b] += 1;
            }
            for (i, count) in counts.iter().enumerate() {
                let lo = min + width * i as f64;
                let hi = lo + width;
                bars.push(Bar {
                    label: format!("{} .. {}", fmtnum::two_dp(lo), fmtnum::two_dp(hi)),
                    count: *count,
                    share: *count as f64 / vals.len() as f64,
                });
            }
        } else {
            let vc = s
                .value_counts(true, false, "count".into(), false)
                .map_err(pretty_polars)?;
            let names = vc.get_column_names();
            let value_col = names[0].to_string();
            let count_col = names[1].to_string();
            let values = vc.column(&value_col).map_err(pretty_polars)?;
            let counts = vc.column(&count_col).map_err(pretty_polars)?;
            let rows = vc.height().min(limit);
            for i in 0..rows {
                let label = values
                    .get(i)
                    .map(|av| any_value_text(&av, CellFormat::Auto))
                    .unwrap_or_default();
                let count = counts
                    .get(i)
                    .ok()
                    .and_then(|av| av.try_extract::<i64>().ok())
                    .unwrap_or(0) as usize;
                bars.push(Bar {
                    label: if label.is_empty() {
                        "(null)".to_string()
                    } else {
                        label
                    },
                    count,
                    share: count as f64 / total.max(1) as f64,
                });
            }
        }

        Ok(Distribution {
            column: column.to_string(),
            total,
            binned: use_bins,
            bars,
        })
    }

    /// Pearson correlation between every pair of numeric columns in the view.
    pub fn correlations(&self, fmt: NumFormat) -> Result<Correlations, String> {
        let mut names: Vec<String> = Vec::new();
        let mut series: Vec<Vec<Option<f64>>> = Vec::new();
        for name in self.view_columns() {
            let c = match self.view.column(&name) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let s = c.as_materialized_series();
            if !ColumnKind::of(s.dtype()).is_numeric() {
                continue;
            }
            let f = match s.cast(&DataType::Float64) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let ca = match f.f64() {
                Ok(ca) => ca.clone(),
                Err(_) => continue,
            };
            series.push((0..ca.len()).map(|i| ca.get(i)).collect());
            names.push(name);
        }

        if names.len() < 2 {
            return Err(
                "correlation needs at least two numeric columns in the current result".to_string(),
            );
        }

        let mut matrix = vec![vec![String::new(); names.len()]; names.len()];
        let mut pairs: Vec<(String, String, f64)> = Vec::new();
        for i in 0..names.len() {
            for j in 0..names.len() {
                let r = pearson(&series[i], &series[j]);
                matrix[i][j] = match r {
                    // Correlations keep fixed two decimals when that format is
                    // selected: `1.00`, `-0.12`, never `1` or `-0.1`.
                    Some(v) => fmt.apply(v),
                    None => "-".to_string(),
                };
                if i < j {
                    if let Some(v) = r {
                        pairs.push((names[i].clone(), names[j].clone(), v));
                    }
                }
            }
        }
        // Strongest relationships first, regardless of sign.
        pairs.sort_by(|a, b| {
            b.2.abs()
                .partial_cmp(&a.2.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(Correlations {
            names,
            matrix,
            pairs: pairs
                .into_iter()
                .map(|(a, b, v)| (a, b, fmt.apply(v), strength(v)))
                .collect(),
        })
    }
}

/// Descriptive statistics result: label/value pairs shown on one screen.
pub struct Stats {
    pub column: String,
    pub kind: ColumnKind,
    pub rows: Vec<(String, String)>,
}

/// One bar of a distribution.
pub struct Bar {
    pub label: String,
    pub count: usize,
    pub share: f64,
}

/// Distribution of a column's values.
pub struct Distribution {
    pub column: String,
    pub total: usize,
    /// True when values were bucketed into numeric bins.
    pub binned: bool,
    pub bars: Vec<Bar>,
}

/// Correlation matrix plus a ranked pair list.
pub struct Correlations {
    pub names: Vec<String>,
    pub matrix: Vec<Vec<String>>,
    /// (column a, column b, formatted r, qualitative strength)
    pub pairs: Vec<(String, String, String, &'static str)>,
}

fn strength(r: f64) -> &'static str {
    let a = r.abs();
    if a >= 0.9 {
        "very strong"
    } else if a >= 0.7 {
        "strong"
    } else if a >= 0.5 {
        "moderate"
    } else if a >= 0.3 {
        "weak"
    } else {
        "negligible"
    }
}

/// Pearson correlation over pairwise-complete observations.
fn pearson(a: &[Option<f64>], b: &[Option<f64>]) -> Option<f64> {
    let n = a.len().min(b.len());
    let mut xs: Vec<f64> = Vec::with_capacity(n);
    let mut ys: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        if let (Some(x), Some(y)) = (a[i], b[i]) {
            if x.is_finite() && y.is_finite() {
                xs.push(x);
                ys.push(y);
            }
        }
    }
    if xs.len() < 2 {
        return None;
    }
    let m = xs.len() as f64;
    let mx = xs.iter().sum::<f64>() / m;
    let my = ys.iter().sum::<f64>() / m;
    let mut num = 0.0;
    let mut dx = 0.0;
    let mut dy = 0.0;
    for i in 0..xs.len() {
        let a = xs[i] - mx;
        let b = ys[i] - my;
        num += a * b;
        dx += a * a;
        dy += b * b;
    }
    if dx <= 0.0 || dy <= 0.0 {
        // A constant column has no correlation to report.
        return None;
    }
    Some(num / (dx.sqrt() * dy.sqrt()))
}

/// Read a CSV with header inference, falling back to a permissive parse when
/// the strict schema inference fails on ragged data.
fn read_csv(path: &Path) -> Result<DataFrame, String> {
    let attempt = CsvReadOptions::default()
        .with_has_header(true)
        .with_infer_schema_length(Some(4096))
        .with_ignore_errors(true)
        .try_into_reader_with_file_path(Some(path.to_path_buf()))
        .and_then(|r| r.finish());

    match attempt {
        Ok(df) => Ok(df),
        Err(first) => {
            // Retry with everything as text: a badly typed column should not
            // stop the file from being browsable.
            let fallback = CsvReadOptions::default()
                .with_has_header(true)
                .with_infer_schema_length(Some(0))
                .with_ignore_errors(true)
                .try_into_reader_with_file_path(Some(path.to_path_buf()))
                .and_then(|r| r.finish());
            match fallback {
                Ok(df) => Ok(df),
                Err(_) => Err(format!("cannot read {}: {}", path.display(), pretty_polars(first))),
            }
        }
    }
}

fn column_kinds(df: &DataFrame) -> Vec<(String, ColumnKind)> {
    df.schema()
        .iter()
        .map(|(n, dt)| (n.to_string(), ColumnKind::of(dt)))
        .collect()
}

/// Take rows by index, preserving the given order.
fn take_rows(df: &DataFrame, idx: &[IdxSize]) -> Result<DataFrame, String> {
    let ca = IdxCa::from_vec("idx".into(), idx.to_vec());
    df.take(&ca).map_err(pretty_polars)
}

/// Render a whole series as display text, one entry per row.
fn series_texts(s: &Series) -> Vec<String> {
    (0..s.len())
        .map(|i| match s.get(i) {
            Ok(av) => any_value_text(&av, CellFormat::Auto),
            Err(_) => String::new(),
        })
        .collect()
}

/// Most frequent value of a series, with its count.
fn top_value(s: &Series) -> Option<(String, usize)> {
    let vc = s.value_counts(true, false, "count".into(), false).ok()?;
    if vc.height() == 0 {
        return None;
    }
    let names = vc.get_column_names();
    let val = vc.column(names[0].as_str()).ok()?.get(0).ok()?;
    let count = vc
        .column(names[1].as_str())
        .ok()?
        .get(0)
        .ok()?
        .try_extract::<i64>()
        .ok()?;
    Some((any_value_text(&val, CellFormat::Auto), count as usize))
}

/// Convert one cell to display text.
pub fn any_value_text(av: &AnyValue, fmt: CellFormat) -> String {
    match av {
        AnyValue::Null => String::new(),
        AnyValue::String(s) => s.to_string(),
        AnyValue::StringOwned(s) => s.to_string(),
        AnyValue::Boolean(b) => b.to_string(),
        AnyValue::Float32(v) => fmt.apply(*v as f64),
        AnyValue::Float64(v) => fmt.apply(*v),
        other => {
            // Integers and temporal values already render exactly; only floats
            // need the formatting rules applied.
            let s = other.str_value().to_string();
            if let (CellFormat::TwoDp | CellFormat::Sig3, Ok(v)) = (fmt, s.parse::<f64>()) {
                return fmt.apply(v);
            }
            s
        }
    }
}

/// Best fuzzy score for one row against pre-lowercased terms.
///
/// Space-separated terms must all match somewhere in the row, which makes
/// `Sales 45` behave like an implicit AND.
fn score_row(cells: &[String], terms: &[&str]) -> Option<i64> {
    if terms.is_empty() {
        return Some(0);
    }
    let mut total = 0i64;
    for term in terms {
        let mut best: Option<i64> = None;
        for cell in cells {
            if let Some(s) = score_lowered(cell, term) {
                if best.map_or(true, |b| s > b) {
                    best = Some(s);
                }
                if s >= 1000 {
                    // An exact cell match is the maximum; stop looking.
                    break;
                }
            }
        }
        match best {
            Some(s) => total += s,
            // One unmatched term rejects the row.
            None => return None,
        }
    }
    Some(total)
}

/// Score a single cell against a needle, case-insensitively. The live search
/// path pre-folds its text, so this wrapper exists for callers that have not.
#[cfg(test)]
fn fuzzy_score(haystack: &str, needle: &str) -> Option<i64> {
    score_lowered(&haystack.to_lowercase(), &needle.to_lowercase())
}

/// Score an already-lowercased cell against an already-lowercased needle.
///
/// Exact and prefix matches outrank substring matches, which outrank scattered
/// subsequence matches; consecutive runs score higher than gappy ones.
///
/// This runs once per cell per keystroke during live search (millions of times
/// on a large file), so it allocates nothing and bails out as early as it can.
fn score_lowered(hs: &str, ns: &str) -> Option<i64> {
    if ns.is_empty() {
        return Some(0);
    }
    // Cheap byte-length gate: a needle longer than the haystack cannot match as
    // a subsequence either, since folding never shortens text below one byte
    // per character.
    if ns.len() > hs.len() {
        return None;
    }

    if hs == ns {
        return Some(1000);
    }
    if let Some(pos) = hs.find(ns) {
        // Contiguous match: strongly preferred, and earlier is better.
        let base = if pos == 0 { 800 } else { 600 };
        let tightness = (ns.chars().count() as i64 * 10) / hs.chars().count().max(1) as i64;
        return Some(base - pos.min(50) as i64 + tightness);
    }

    // Subsequence match with a run bonus, walking both sides as iterators so
    // nothing is allocated.
    let mut score = 0i64;
    let mut hay = hs.chars().enumerate();
    let mut last_hit: Option<usize> = None;
    for ch in ns.chars() {
        let pos = loop {
            match hay.next() {
                Some((i, c)) if c == ch => break i,
                Some(_) => continue,
                // The haystack ran out before the needle did.
                None => return None,
            }
        };
        score += 10;
        if let Some(prev) = last_hit {
            if pos == prev + 1 {
                score += 8;
            } else {
                // Penalise gaps so tighter matches rank first.
                score -= ((pos - prev - 1).min(10)) as i64;
            }
        }
        last_hit = Some(pos);
    }
    Some(score.max(1))
}

/// Turn a Polars error into a single readable line.
pub fn pretty_polars(e: PolarsError) -> String {
    let s = e.to_string();
    let mut line = s
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if line.len() > 400 {
        line.truncate(400);
        line.push('…');
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_csv(name: &str, body: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("toolk_test_{name}_{}.csv", std::process::id()));
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        p
    }

    const SAMPLE: &str = "name,age,department,salary,score\n\
                          Ann,35,Engineering,12000,88.5\n\
                          Bob,45,Sales,9000,72\n\
                          Cid,52,Engineering,20000,91.25\n\
                          Dee,29,Sales,5000,64\n";

    fn ds(name: &str) -> Dataset {
        let p = write_csv(name, SAMPLE);
        Dataset::load(&p).unwrap()
    }

    fn names_of(d: &Dataset) -> Vec<String> {
        (0..d.view_rows())
            .map(|i| d.cell(i, "name", CellFormat::Auto))
            .collect()
    }

    #[test]
    fn loads_all_columns_from_the_header() {
        let d = ds("load");
        assert_eq!(
            d.columns().iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            vec!["name", "age", "department", "salary", "score"]
        );
        assert_eq!(d.total_rows(), 4);
        assert!(d.columns()[1].1.is_numeric());
        assert_eq!(d.columns()[0].1, ColumnKind::Text);
    }

    #[test]
    fn fuzzy_matches_across_columns() {
        let mut d = ds("fuzzy");
        let n = d.fuzzy_ranked("Sales").unwrap();
        assert_eq!(n, 2);
        assert_eq!(names_of(&d), vec!["Bob", "Dee"]);
        // Every column stays available for the selected row.
        assert_eq!(d.view_columns().len(), 5);
        // Subsequence matching, not just substring.
        let n = d.fuzzy_ranked("Egnr").unwrap();
        assert!(n >= 2, "expected engineering rows, got {n}");
    }

    #[test]
    fn fuzzy_terms_are_anded() {
        let mut d = ds("fuzzyand");
        let n = d.fuzzy_ranked("Sales 45").unwrap();
        assert_eq!(n, 1);
        assert_eq!(names_of(&d), vec!["Bob"]);
    }

    #[test]
    fn sql_like_filters_with_and_or() {
        let mut d = ds("like");
        let n = d.query_sql_like("select where age > 40").unwrap();
        assert_eq!(n, 2);
        assert_eq!(names_of(&d), vec!["Bob", "Cid"]);

        let n = d
            .query_sql_like("select where department = 'Engineering' and salary > 10000")
            .unwrap();
        assert_eq!(n, 2);
        assert_eq!(names_of(&d), vec!["Ann", "Cid"]);

        let n = d
            .query_sql_like("select where department = 'Sales' or salary >= 20000")
            .unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    fn sql_runs_standard_statements() {
        let mut d = ds("sql");
        let n = d
            .query_sql("select * from df where department = 'Sales' and salary > 6000")
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(names_of(&d), vec!["Bob"]);

        let n = d
            .query_sql("select * from df where age = 35 and salary > 5000 and salary < 15000")
            .unwrap();
        assert_eq!(n, 1);

        // Aggregates work too, and the view adopts the result's columns.
        let n = d
            .query_sql("select department, count(*) as n from df group by department")
            .unwrap();
        assert_eq!(n, 2);
        assert!(d.view_columns().contains(&"n".to_string()));
    }

    #[test]
    fn sql_accepts_the_file_stem_as_a_table_name() {
        let p = write_csv("stem", SAMPLE);
        let mut d = Dataset::load(&p).unwrap();
        let stem = p.file_stem().unwrap().to_str().unwrap().to_string();
        let n = d.query_sql(&format!("select * from {stem} where age > 40")).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn sort_is_multi_column_and_toggles() {
        let mut d = ds("sort");
        d.toggle_sort("department").unwrap();
        d.add_sort_key("salary", SortDir::Desc).unwrap();
        // Engineering first (asc), then by salary descending inside each group.
        assert_eq!(names_of(&d), vec!["Cid", "Ann", "Bob", "Dee"]);
        let dir = d.toggle_sort("department").unwrap();
        assert_eq!(dir, SortDir::Desc);
        assert_eq!(names_of(&d), vec!["Bob", "Dee", "Cid", "Ann"]);
    }

    #[test]
    fn sort_survives_a_new_query() {
        let mut d = ds("sortquery");
        d.toggle_sort("salary").unwrap();
        d.query_sql_like("select where salary > 6000").unwrap();
        assert_eq!(names_of(&d), vec!["Bob", "Ann", "Cid"]);
    }

    #[test]
    fn export_writes_header_and_rows() {
        let mut d = ds("export");
        d.query_sql_like("select where age > 40").unwrap();
        let mut out = std::env::temp_dir();
        out.push(format!("toolk_export_{}.csv", std::process::id()));
        let n = d.export_csv(&out).unwrap();
        assert_eq!(n, 2);
        let body = std::fs::read_to_string(&out).unwrap();
        let lines: Vec<&str> = body.trim().lines().collect();
        assert_eq!(lines[0], "name,age,department,salary,score");
        assert_eq!(lines.len(), 3);
        assert!(lines[1].starts_with("Bob,45"));
        std::fs::remove_file(&out).ok();
    }

    #[test]
    fn describe_formats_numbers_with_fixed_decimals() {
        let d = ds("describe");
        let s = d.describe("salary", NumFormat::TwoDp).unwrap();
        let get = |k: &str| {
            s.rows
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(get("count"), "4");
        assert_eq!(get("mean"), "11500.00");
        assert_eq!(get("min"), "5000.00");
        assert_eq!(get("max"), "20000.00");
        assert_eq!(get("sum"), "46000.00");
        // Integer-valued statistics keep their trailing zeros.
        assert_eq!(get("median"), "10500.00");
        assert_eq!(get("std"), "6350.85");
    }

    #[test]
    fn describe_handles_text_columns() {
        let d = ds("describetext");
        let s = d.describe("department", NumFormat::TwoDp).unwrap();
        assert_eq!(s.kind, ColumnKind::Text);
        assert!(s.rows.iter().any(|(k, _)| k == "most common"));
        assert!(s.rows.iter().any(|(k, v)| k == "its share" && v.contains('%')));
    }

    #[test]
    fn distribution_counts_categories() {
        let d = ds("dist");
        let dist = d.distribution("department", 10).unwrap();
        assert!(!dist.binned);
        assert_eq!(dist.bars.len(), 2);
        assert_eq!(dist.bars.iter().map(|b| b.count).sum::<usize>(), 4);
        assert_eq!(dist.bars[0].count, 2);
    }

    #[test]
    fn distribution_bins_wide_numeric_columns() {
        let d = ds("distnum");
        // 4 distinct salaries with a 3-row limit falls back to numeric bins.
        let dist = d.distribution("salary", 3).unwrap();
        assert!(dist.binned);
        assert_eq!(dist.bars.iter().map(|b| b.count).sum::<usize>(), 4);
    }

    #[test]
    fn correlation_keeps_two_decimals() {
        let d = ds("corr");
        let c = d.correlations(NumFormat::TwoDp).unwrap();
        // Diagonal is exactly 1.00, never "1".
        for i in 0..c.names.len() {
            assert_eq!(c.matrix[i][i], "1.00");
        }
        for row in &c.matrix {
            for v in row {
                assert!(v == "-" || v.contains('.'), "unformatted correlation {v}");
            }
        }
        assert!(!c.pairs.is_empty());
    }

    #[test]
    fn correlation_needs_two_numeric_columns() {
        let p = write_csv("corrfail", "a,b\nx,y\nz,w\n");
        let d = Dataset::load(&p).unwrap();
        assert!(d.correlations(NumFormat::TwoDp).is_err());
    }

    #[test]
    fn column_lookup_is_forgiving() {
        let cols = vec![("First Name".to_string(), ColumnKind::Text)];
        assert!(resolve_column(&cols, "first name").is_some());
        assert!(resolve_column(&cols, "first_name").is_some());
        assert!(resolve_column(&cols, "FirstName").is_some());
        assert!(resolve_column(&cols, "last").is_none());
    }

    #[test]
    fn missing_file_is_reported() {
        let e = match Dataset::load(Path::new("/nonexistent/toolk/nope.csv")) {
            Err(e) => e,
            Ok(_) => panic!("expected a load failure"),
        };
        assert!(e.contains("not found"), "{e}");
    }

    #[test]
    fn bad_query_reports_an_error_and_keeps_the_view() {
        let mut d = ds("badquery");
        let before = d.view_rows();
        assert!(d.query_sql("select * from nosuchtable").is_err());
        assert_eq!(d.view_rows(), before);
        assert!(d.query_sql_like("select where nope = 1").is_err());
        assert_eq!(d.view_rows(), before);
    }

    #[test]
    fn fuzzy_scores_rank_exact_over_scattered() {
        let exact = fuzzy_score("Sales", "Sales").unwrap();
        let prefix = fuzzy_score("Salesforce", "Sales").unwrap();
        let sub = fuzzy_score("Global Sales", "Sales").unwrap();
        let scattered = fuzzy_score("Specialist Analyst", "sals").unwrap();
        assert!(exact > prefix, "{exact} vs {prefix}");
        assert!(prefix > sub, "{prefix} vs {sub}");
        assert!(sub > scattered, "{sub} vs {scattered}");
        assert!(fuzzy_score("Sales", "xyz").is_none());
    }
}
