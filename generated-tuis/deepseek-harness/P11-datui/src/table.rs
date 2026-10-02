//! Conversion of a Polars `DataFrame` into renderable table data.

use polars::prelude::*;

use crate::format;

/// Flattened, render-ready representation of a DataFrame.
pub struct TableData {
    pub headers: Vec<String>,
    /// Cell strings, already formatted (floats fixed to two decimals).
    pub rows: Vec<Vec<String>>,
}

impl TableData {
    pub fn empty() -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn col_count(&self) -> usize {
        self.headers.len()
    }
}

/// Build render-ready table data from a DataFrame.
pub fn from_df(df: &DataFrame) -> TableData {
    let headers: Vec<String> = df
        .get_column_names()
        .iter()
        .map(|n| n.as_str().to_string())
        .collect();

    let n = df.height();
    let m = headers.len();
    let mut rows = Vec::with_capacity(n);
    for r in 0..n {
        let mut row = Vec::with_capacity(m);
        for c in 0..m {
            let col = df.column(&headers[c]);
            let av = match col {
                Ok(col) => col.get(r).unwrap_or(AnyValue::Null),
                Err(_) => AnyValue::Null,
            };
            row.push(format_cell(&av));
        }
        rows.push(row);
    }

    TableData { headers, rows }
}

/// Format a single cell. Float columns keep exactly two decimal places so
/// trailing zeros are never dropped (e.g. `50.00`); everything else uses the
/// default representation. Nulls render as empty cells.
fn format_cell(av: &AnyValue) -> String {
    match av {
        AnyValue::Null => String::new(),
        AnyValue::Float64(v) => format::two_dp(*v),
        AnyValue::Float32(v) => format::two_dp(*v as f64),
        AnyValue::Float16(v) => format::two_dp(f64::from(*v)),
        other => other.to_string(),
    }
}

/// Compute display column widths (content + 2 padding), clamped to a cap so a
/// single very long value cannot blow out the whole table. Only the first
/// `sample` rows are inspected for performance.
pub fn compute_widths(td: &TableData, cap: usize, sample: usize) -> Vec<usize> {
    let m = td.col_count();
    let mut widths = vec![0usize; m];
    for (c, h) in td.headers.iter().enumerate() {
        widths[c] = h.chars().count();
    }
    let sample = td.row_count().min(sample);
    for r in 0..sample {
        for c in 0..m {
            let w = td.rows[r][c].chars().count().min(cap);
            if w > widths[c] {
                widths[c] = w;
            }
        }
    }
    for w in widths.iter_mut() {
        *w = (*w + 2).clamp(3, cap + 2);
    }
    widths
}

/// Return the names of columns with a numeric dtype.
pub fn numeric_columns(df: &DataFrame) -> Vec<String> {
    df.get_column_names()
        .iter()
        .filter_map(|n| {
            let name = n.as_str().to_string();
            df.column(&name)
                .ok()
                .filter(|c| c.dtype().is_numeric())
                .map(|_| name)
        })
        .collect()
}

/// Extract a column as `Option<f64>`, casting numeric types to Float64.
pub fn column_f64(df: &DataFrame, name: &str) -> Vec<Option<f64>> {
    let n = df.height();
    let mut out = vec![None; n];
    if let Ok(col) = df.column(name) {
        if let Ok(cast) = col.cast(&DataType::Float64) {
            if let Ok(ca) = cast.f64() {
                out = ca.iter().collect();
            }
        }
    }
    out
}
