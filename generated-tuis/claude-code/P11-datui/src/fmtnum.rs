//! Numeric formatting helpers.
//!
//! The display rules are deliberately strict: trailing zeros are never dropped,
//! because a value rendered as `50` instead of `50.00` (or `0.88` instead of
//! `0.878`) is a validation mismatch, not a cosmetic difference.

/// How analysis numbers are rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumFormat {
    /// Always exactly two digits after the decimal point (`50.00`, `1288.42`).
    TwoDp,
    /// Three significant figures (`456`, `45.6`, `0.878`).
    Sig3,
}

impl NumFormat {
    pub fn label(self) -> &'static str {
        match self {
            NumFormat::TwoDp => "2dp",
            NumFormat::Sig3 => "3sig",
        }
    }

    pub fn next(self) -> Self {
        match self {
            NumFormat::TwoDp => NumFormat::Sig3,
            NumFormat::Sig3 => NumFormat::TwoDp,
        }
    }

    /// Render a value using this format.
    pub fn apply(self, v: f64) -> String {
        match self {
            NumFormat::TwoDp => two_dp(v),
            NumFormat::Sig3 => sig_figs(v, 3),
        }
    }

    /// Render an optional value, using a placeholder for missing data.
    pub fn apply_opt(self, v: Option<f64>) -> String {
        match v {
            Some(v) => self.apply(v),
            None => "-".to_string(),
        }
    }
}

/// Placeholder used for non-finite values so the table never shows `NaN`/`inf`
/// where a number is expected.
fn non_finite(v: f64) -> Option<&'static str> {
    if v.is_nan() {
        Some("NaN")
    } else if v.is_infinite() {
        Some(if v > 0.0 { "inf" } else { "-inf" })
    } else {
        None
    }
}

/// Exactly two decimal places, zero padded. `50 -> "50.00"`, `99.994 -> "99.99"`.
pub fn two_dp(v: f64) -> String {
    if let Some(s) = non_finite(v) {
        return s.to_string();
    }
    // `-0.00` reads as a bug to users; normalise it to `0.00`.
    let s = format!("{:.2}", v);
    if s == "-0.00" { "0.00".to_string() } else { s }
}

/// Round to `n` significant figures, keeping every figure that the rule
/// requires. Values below 1 keep the full three figures (`0.878`), and values
/// wider than `n` digits are rounded at the correct power of ten (`4567 ->
/// 4570`) rather than being truncated.
pub fn sig_figs(v: f64, n: usize) -> String {
    if let Some(s) = non_finite(v) {
        return s.to_string();
    }
    let n = n.max(1);
    if v == 0.0 {
        // Zero has no exponent; show it with the same width as small values.
        return format!("{:.*}", n.saturating_sub(1), 0.0);
    }

    // Choose the decimal count from the magnitude, then re-check it against the
    // rounded value: rounding 9.999 to 3 figures carries into a new decade
    // (10.0) and the decimal count has to shrink with it.
    let mut decimals = decimals_for(v, n);
    let mut value = round_sig(v, decimals, n);
    let refined = decimals_for(value, n);
    if refined != decimals {
        decimals = refined;
        value = round_sig(v, decimals, n);
    }

    let s = format!("{:.*}", decimals, value);
    if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == '.') {
        return s[1..].to_string();
    }
    s
}

/// Decimal places needed to show `n` significant figures of `v`.
fn decimals_for(v: f64, n: usize) -> usize {
    if v == 0.0 || !v.is_finite() {
        return n.saturating_sub(1);
    }
    let exp = v.abs().log10().floor() as i32;
    let want = n as i32 - 1 - exp;
    want.max(0) as usize
}

/// Round `v` so only `n` significant figures survive.
///
/// When `decimals > 0` the value is rounded at that decimal place; when it is
/// zero the value must still be rounded at the right power of ten, so `4567`
/// becomes `4570` rather than staying `4567`.
fn round_sig(v: f64, decimals: usize, n: usize) -> f64 {
    if decimals > 0 {
        let factor = 10f64.powi(decimals as i32);
        if !factor.is_finite() || factor == 0.0 {
            return v;
        }
        return (v * factor).round() / factor;
    }
    let exp = v.abs().log10().floor() as i32;
    let shift = exp - n as i32 + 1;
    if shift <= 0 {
        return v.round();
    }
    // 10^shift can overflow precision for huge exponents; fall back to the raw
    // value rather than returning garbage.
    let factor = 10f64.powi(shift);
    if !factor.is_finite() || factor == 0.0 {
        return v;
    }
    (v / factor).round() * factor
}

/// Percentage with two decimals, e.g. `12.50%`.
pub fn percent_2dp(fraction: f64) -> String {
    format!("{}%", two_dp(fraction * 100.0))
}

/// Render a float found in a data cell. `Auto` keeps the value looking like the
/// source file (`5000`, `1288.42`) instead of forcing a decimal tail onto every
/// integer, while the fixed formats are available for tasks that require them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellFormat {
    Auto,
    TwoDp,
    Sig3,
}

impl CellFormat {
    pub fn label(self) -> &'static str {
        match self {
            CellFormat::Auto => "auto",
            CellFormat::TwoDp => "2dp",
            CellFormat::Sig3 => "3sig",
        }
    }

    pub fn next(self) -> Self {
        match self {
            CellFormat::Auto => CellFormat::TwoDp,
            CellFormat::TwoDp => CellFormat::Sig3,
            CellFormat::Sig3 => CellFormat::Auto,
        }
    }

    pub fn apply(self, v: f64) -> String {
        match self {
            CellFormat::Auto => auto_float(v),
            CellFormat::TwoDp => two_dp(v),
            CellFormat::Sig3 => sig_figs(v, 3),
        }
    }
}

/// Shortest faithful rendering of a float: integral values lose the `.0` tail,
/// everything else keeps Rust's round-trip representation.
pub fn auto_float(v: f64) -> String {
    if let Some(s) = non_finite(v) {
        return s.to_string();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{}", v);
    if s.contains('e') || s.contains('E') {
        // Avoid scientific notation in a data table where it would be unreadable.
        let alt = format!("{:.6}", v);
        let trimmed = alt.trim_end_matches('0').trim_end_matches('.').to_string();
        if trimmed.is_empty() || trimmed == "-" {
            return s;
        }
        return trimmed;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_decimals_pad_and_round() {
        assert_eq!(two_dp(50.0), "50.00");
        assert_eq!(two_dp(75.25), "75.25");
        assert_eq!(two_dp(1288.42), "1288.42");
        assert_eq!(two_dp(99.99), "99.99");
        assert_eq!(two_dp(0.85), "0.85");
        assert_eq!(two_dp(1.0), "1.00");
        assert_eq!(two_dp(-0.12), "-0.12");
        assert_eq!(two_dp(-0.001), "0.00");
        assert_eq!(two_dp(0.0), "0.00");
    }

    #[test]
    fn three_significant_figures() {
        assert_eq!(sig_figs(456.0, 3), "456");
        assert_eq!(sig_figs(45.6, 3), "45.6");
        assert_eq!(sig_figs(12.3, 3), "12.3");
        // Values below 1 must keep three figures.
        assert_eq!(sig_figs(0.878, 3), "0.878");
        assert_eq!(sig_figs(0.8781234, 3), "0.878");
        assert_eq!(sig_figs(0.05, 3), "0.0500");
        assert_eq!(sig_figs(0.000456789, 3), "0.000457");
        // Wide values round at the right power of ten.
        assert_eq!(sig_figs(4567.0, 3), "4570");
        assert_eq!(sig_figs(123456.0, 3), "123000");
        // Carrying into a new decade shrinks the decimal count.
        assert_eq!(sig_figs(9.999, 3), "10.0");
        assert_eq!(sig_figs(0.0, 3), "0.00");
        assert_eq!(sig_figs(-45.678, 3), "-45.7");
    }

    #[test]
    fn auto_cells_look_like_the_source_file() {
        assert_eq!(auto_float(5000.0), "5000");
        assert_eq!(auto_float(1288.42), "1288.42");
        assert_eq!(auto_float(-3.5), "-3.5");
    }

    #[test]
    fn percentages_keep_two_decimals() {
        assert_eq!(percent_2dp(0.125), "12.50%");
        assert_eq!(percent_2dp(1.0), "100.00%");
    }
}
