//! Small helpers for money formatting/parsing and date/month handling.

use chrono::{Datelike, Local, NaiveDate};

/// Format integer cents as a fixed two-decimal, zero-padded yuan string.
/// e.g. 123456 -> "1234.56", 80005 -> "800.05", -71491 -> "-714.91".
pub fn fmt_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let a = cents.unsigned_abs();
    format!("{}{}.{:02}", sign, a / 100, a % 100)
}

/// Parse a yuan amount string ("1234.56", "800.05", "42") into integer cents.
pub fn parse_money(s: &str) -> Result<i64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Amount is required".into());
    }
    let (whole, frac) = match s.split_once('.') {
        Some((w, f)) => {
            if f.contains('.') {
                return Err(format!("Invalid amount \"{}\"", s));
            }
            (w, Some(f))
        }
        None => (s, None),
    };
    if whole.is_empty() {
        return Err(format!("Invalid amount \"{}\"", s));
    }
    let w: i64 = whole
        .parse()
        .map_err(|_| format!("Invalid amount \"{}\"", s))?;
    if w < 0 {
        return Err("Amount cannot be negative".into());
    }
    let mut cents = w * 100;
    if let Some(f) = frac {
        if !f.is_empty() {
            if f.len() > 2 {
                return Err(format!("Too many decimal places in \"{}\" (max 2)", s));
            }
            if !f.chars().all(|c| c.is_ascii_digit()) {
                return Err(format!("Invalid amount \"{}\"", s));
            }
            let p: i64 = match f.len() {
                1 => {
                    f.parse::<i64>()
                        .map_err(|_| format!("Invalid amount \"{}\"", s))?
                        * 10
                }
                2 => f
                    .parse::<i64>()
                    .map_err(|_| format!("Invalid amount \"{}\"", s))?,
                _ => unreachable!(),
            };
            cents += p;
        }
    }
    Ok(cents)
}

/// Validate and normalize a YYYY-MM-DD date.
pub fn parse_date(s: &str) -> Result<String, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Date is required".into());
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|d| d.format("%Y-%m-%d").to_string())
        .map_err(|_| format!("Invalid date \"{}\" (use YYYY-MM-DD)", s))
}

/// Validate and normalize a YYYY-MM month.
pub fn parse_month(s: &str) -> Result<String, String> {
    let s = s.trim();
    if s.len() != 7 || s.as_bytes().get(4) != Some(&b'-') {
        return Err("Invalid month (use YYYY-MM)".into());
    }
    let y: i32 = s[0..4]
        .parse()
        .map_err(|_| "Invalid month (use YYYY-MM)".to_string())?;
    let m: u32 = s[5..7]
        .parse()
        .map_err(|_| "Invalid month (use YYYY-MM)".to_string())?;
    if !(1..=12).contains(&m) {
        return Err("Invalid month (MM must be 01-12)".into());
    }
    Ok(format!("{:04}-{:02}", y, m))
}

pub fn month_str(y: i32, m: u32) -> String {
    format!("{:04}-{:02}", y, m)
}

/// Shift a (year, month) tuple by `delta` months.
pub fn shift_month(y: i32, m: u32, delta: i32) -> (i32, u32) {
    let total = y * 12 + (m as i32 - 1) + delta;
    let ny = total.div_euclid(12);
    let nm = total.rem_euclid(12) + 1;
    (ny, nm as u32)
}

pub fn current_month() -> (i32, u32) {
    let n = Local::now();
    (n.year(), n.month())
}

pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_cents_fixed_two_decimals() {
        assert_eq!(fmt_cents(123456), "1234.56");
        assert_eq!(fmt_cents(80005), "800.05");
        assert_eq!(fmt_cents(0), "0.00");
        assert_eq!(fmt_cents(5), "0.05");
        assert_eq!(fmt_cents(-71491), "-714.91");
        assert_eq!(fmt_cents(358842), "3588.42");
    }

    #[test]
    fn parse_money_roundtrip() {
        assert_eq!(parse_money("1234.56").unwrap(), 123456);
        assert_eq!(parse_money("800.05").unwrap(), 80005);
        assert_eq!(parse_money("42").unwrap(), 4200);
        assert_eq!(parse_money("42.5").unwrap(), 4250);
        assert_eq!(parse_money("0").unwrap(), 0);
        assert!(parse_money("").is_err());
        assert!(parse_money("abc").is_err());
        assert!(parse_money("-5").is_err());
        assert!(parse_money("1.234").is_err());
    }

    #[test]
    fn parse_date_validates() {
        assert_eq!(parse_date("2025-01-15").unwrap(), "2025-01-15");
        assert!(parse_date("2025-13-01").is_err());
        assert!(parse_date("01-15-2025").is_err());
        assert!(parse_date("").is_err());
    }

    #[test]
    fn parse_month_validates() {
        assert_eq!(parse_month("2025-01").unwrap(), "2025-01");
        assert_eq!(parse_month("2025-12").unwrap(), "2025-12");
        assert!(parse_month("2025-13").is_err());
        assert!(parse_month("2025-1").is_err());
        assert!(parse_month("2025/01").is_err());
    }

    #[test]
    fn month_shift_crosses_year() {
        assert_eq!(shift_month(2025, 1, -1), (2024, 12));
        assert_eq!(shift_month(2025, 12, 1), (2026, 1));
        assert_eq!(shift_month(2025, 6, 3), (2025, 9));
    }
}
