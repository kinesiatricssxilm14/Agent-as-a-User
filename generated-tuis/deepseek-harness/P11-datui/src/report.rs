//! Build the human-readable analysis report shown in the Analysis view.
//!
//! The report combines descriptive statistics, a Pearson correlation matrix
//! and a distribution histogram for one selected numeric column — all of which
//! are displayed on the same screen (scrollable).

use polars::prelude::*;

use crate::analysis::{histogram, pearson_pairwise, summarize, HistogramBin};
use crate::format;
use crate::table;

/// Max histogram bar length in characters.
const BAR_MAX: usize = 40;
/// Left label column width for the descriptive-statistics table.
const LABEL_W: usize = 10;

/// Everything needed to render the analysis view.
pub struct Report {
    /// The report content, one string per line.
    pub lines: Vec<String>,
    /// Numeric columns (also the histogram switch candidates).
    pub numeric_cols: Vec<String>,
}

impl Report {
    pub fn for_df(df: &DataFrame, hist_idx: usize) -> Report {
        let numeric_cols = table::numeric_columns(df);
        if numeric_cols.is_empty() {
            return Report {
                lines: vec![
                    String::new(),
                    "No numeric columns found in the current result.".to_string(),
                    "Numeric columns are required for statistics / correlation.".to_string(),
                ],
                numeric_cols,
            };
        }

        let mut lines = Vec::new();

        // ---- Descriptive statistics -------------------------------------
        lines.push("Descriptive Statistics (numeric columns)".to_string());
        lines.push(format!("rows: {}   numeric columns: {}", df.height(), numeric_cols.len()));
        lines.push(String::new());

        let col_w = numeric_cols
            .iter()
            .map(|c| c.chars().count() + 1)
            .max()
            .unwrap_or(12)
            .clamp(12, 24);

        // Header row.
        let mut header = pad_right("", LABEL_W);
        for c in &numeric_cols {
            header.push_str(&pad_right(&truncate(c, col_w), col_w));
        }
        lines.push(header);

        let summaries: Vec<_> = numeric_cols
            .iter()
            .map(|c| {
                let values = table::column_f64(df, c);
                let missing = values.iter().filter(|v| v.is_none()).count();
                let obs: Vec<f64> = values.iter().flatten().copied().collect();
                summarize(&obs, missing)
            })
            .collect();

        let stat_rows: Vec<(&str, Box<dyn Fn(&crate::analysis::NumericSummary) -> String>)> = vec![
            ("count", Box::new(|s| s.count.to_string())),
            ("missing", Box::new(|s| s.missing.to_string())),
            ("mean", Box::new(|s| format::sig_figs(s.mean, 3))),
            ("std", Box::new(|s| format::sig_figs(s.std, 3))),
            ("min", Box::new(|s| format::sig_figs(s.min, 3))),
            ("q1", Box::new(|s| format::sig_figs(s.q1, 3))),
            ("median", Box::new(|s| format::sig_figs(s.median, 3))),
            ("q3", Box::new(|s| format::sig_figs(s.q3, 3))),
            ("max", Box::new(|s| format::sig_figs(s.max, 3))),
            ("skewness", Box::new(|s| format::sig_figs(s.skewness, 3))),
            ("kurtosis", Box::new(|s| format::sig_figs(s.kurtosis, 3))),
        ];

        for (label, f) in &stat_rows {
            let mut row = pad_right(label, LABEL_W);
            for s in &summaries {
                row.push_str(&pad_left(&f(s), col_w));
            }
            lines.push(row);
        }

        // ---- Correlation matrix ----------------------------------------
        lines.push(String::new());
        lines.push("Pearson Correlation (2 decimals)".to_string());
        lines.push(String::new());

        let cw = numeric_cols
            .iter()
            .map(|c| c.chars().count() + 1)
            .max()
            .unwrap_or(8)
            .clamp(8, 24);

        let mut corr_header = pad_right("", cw);
        for c in &numeric_cols {
            corr_header.push_str(&pad_left(&truncate(c, cw), cw));
        }
        lines.push(corr_header);

        for a in &numeric_cols {
            let mut row = pad_right(&truncate(a, cw), cw);
            let xa = table::column_f64(df, a);
            for b in &numeric_cols {
                let xb = table::column_f64(df, b);
                let r = pearson_pairwise(&xa, &xb);
                row.push_str(&pad_left(&format::corr(r), cw));
            }
            lines.push(row);
        }

        // ---- Distribution histogram ------------------------------------
        let idx = hist_idx.min(numeric_cols.len().saturating_sub(1));
        let col = &numeric_cols[idx];
        let values: Vec<f64> = table::column_f64(df, col).iter().flatten().copied().collect();

        lines.push(String::new());
        lines.push(format!(
            "Distribution: {col}   (column {}/{}, ←/→ to switch)",
            idx + 1,
            numeric_cols.len()
        ));
        lines.push(String::new());

        let bins = histogram(&values, 10);
        let max_count = bins.iter().map(|b| b.count).max().unwrap_or(1).max(1);
        for b in &bins {
            lines.push(render_bin(b, max_count, values.len()));
        }

        Report {
            lines,
            numeric_cols,
        }
    }
}

fn render_bin(b: &HistogramBin, max_count: usize, total: usize) -> String {
    let bar_len = if max_count == 0 {
        0
    } else {
        ((b.count as f64 / max_count as f64) * BAR_MAX as f64).round() as usize
    };
    let bar = "█".repeat(bar_len);
    let pct = if total == 0 {
        0.0
    } else {
        b.count as f64 / total as f64 * 100.0
    };
    let label = if (b.start - b.end).abs() < f64::EPSILON {
        format!("[{}]", format::two_dp(b.start))
    } else {
        format!("[{}, {})", format::two_dp(b.start), format::two_dp(b.end))
    };
    format!(
        "{:>24}  {:<width$} {:>5}  {:>8}",
        label,
        bar,
        b.count,
        format::pct(pct),
        width = BAR_MAX
    )
}

fn truncate(s: &str, w: usize) -> String {
    if s.chars().count() <= w {
        s.to_string()
    } else if w <= 1 {
        "~".to_string()
    } else {
        let mut out: String = s.chars().take(w - 1).collect();
        out.push('~');
        out
    }
}

fn pad_left(s: &str, w: usize) -> String {
    format!("{s:>w$}")
}

fn pad_right(s: &str, w: usize) -> String {
    format!("{s:<w$}")
}
