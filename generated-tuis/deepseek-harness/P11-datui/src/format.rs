//! Numeric formatting helpers.
//!
//! The project requires fixed digit counts with zero-padding so that trailing
//! zeros are never silently dropped:
//! * two decimal places  -> `75.25`, `50.00`, `-0.12`
//! * three significant figures -> `456`, `45.6`, `12.3`, `0.878`
//! * correlation coefficients -> fixed two decimals -> `0.85`, `1.00`

/// Format a float with exactly two decimal places (trailing zeros kept).
pub fn two_dp(v: f64) -> String {
    // Normalize negative zero so we never print "-0.00".
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{v:.2}")
}

/// Format a float to exactly `sig` significant figures without scientific
/// notation. Values equal to zero are rendered with trailing zeros so the
/// requested number of significant figures is still visually present
/// (e.g. `0.878` for three figures, `0.00` for zero at two figures).
pub fn sig_figs(v: f64, sig: u32) -> String {
    if sig == 0 {
        return String::new();
    }
    if v == 0.0 {
        return format!("0.{}", "0".repeat((sig as usize).saturating_sub(1)));
    }
    if !v.is_finite() {
        return if v.is_nan() {
            "NaN".to_string()
        } else if v > 0.0 {
            "inf".to_string()
        } else {
            "-inf".to_string()
        };
    }

    let neg = v.is_sign_negative();
    let av = v.abs();
    // Decimal exponent of the value: for 456.0 -> 2, for 0.878 -> -1.
    let exp = av.log10().floor() as i32;
    let decimals_i32 = sig as i32 - 1 - exp;

    // Round to `sig` significant figures.
    let factor = 10f64.powi(sig as i32 - 1 - exp);
    let rounded = (av * factor).round() / factor;

    let s = if decimals_i32 >= 0 {
        format!("{:.*}", decimals_i32 as usize, rounded)
    } else {
        // The value is large enough that the significant-figure rounding lands
        // on whole powers of ten (e.g. 123456 -> 123000).
        let unit = 10f64.powi(-decimals_i32);
        let r = (av / unit).round() * unit;
        format!("{r:.0}")
    };

    if neg {
        format!("-{s}")
    } else {
        s
    }
}

/// Format a correlation coefficient: always two decimals.
pub fn corr(v: f64) -> String {
    two_dp(v)
}

/// Format a percentage with exactly two decimal places.
pub fn pct(v: f64) -> String {
    format!("{:.2}%", v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_decimals_keep_trailing_zeros() {
        assert_eq!(two_dp(75.25), "75.25");
        assert_eq!(two_dp(1288.42), "1288.42");
        assert_eq!(two_dp(99.99), "99.99");
        assert_eq!(two_dp(50.0), "50.00");
        assert_eq!(two_dp(1.0), "1.00");
        assert_eq!(two_dp(-0.12), "-0.12");
        assert_eq!(two_dp(0.0), "0.00");
        assert_eq!(two_dp(-0.0), "0.00");
    }

    #[test]
    fn three_sig_figs() {
        assert_eq!(sig_figs(456.0, 3), "456");
        assert_eq!(sig_figs(45.6, 3), "45.6");
        assert_eq!(sig_figs(12.3, 3), "12.3");
        assert_eq!(sig_figs(0.878, 3), "0.878");
        assert_eq!(sig_figs(0.0, 3), "0.00");
        assert_eq!(sig_figs(0.05, 3), "0.0500");
        assert_eq!(sig_figs(1288.42, 3), "1290");
        assert_eq!(sig_figs(123456.0, 3), "123000");
        assert_eq!(sig_figs(-12.3, 3), "-12.3");
    }

    #[test]
    fn correlation_two_decimals() {
        assert_eq!(corr(0.85), "0.85");
        assert_eq!(corr(1.0), "1.00");
        assert_eq!(corr(-0.12), "-0.12");
    }
}
