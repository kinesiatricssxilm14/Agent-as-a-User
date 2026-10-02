//! Date handling. Dates are stored as `YYYY-MM-DD` text and months as
//! `YYYY-MM`, which sort lexicographically and let SQLite do range filtering
//! with plain string comparisons.

use chrono::{Local, NaiveDate};

/// Today in the local timezone, as `YYYY-MM-DD`.
pub fn today() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

/// The current month, as `YYYY-MM`.
pub fn current_month() -> String {
    Local::now().date_naive().format("%Y-%m").to_string()
}

/// The `YYYY-MM` month a `YYYY-MM-DD` date falls in.
pub fn month_of(date: &str) -> String {
    if date.len() >= 7 {
        date[..7].to_string()
    } else {
        date.to_string()
    }
}

/// Parse and canonicalise a user-entered date.
///
/// Accepts `YYYY-MM-DD`, `YYYY/MM/DD`, `YYYY.MM.DD`, and the shorthands
/// `today`, `yesterday`, `tomorrow`. Single-digit month/day components are
/// zero-padded (`2026-8-3` -> `2026-08-03`). An empty input means today, which
/// keeps the common case a single keystroke.
pub fn parse_date(input: &str) -> Result<String, String> {
    let s = input.trim();
    if s.is_empty() {
        return Ok(today());
    }

    match s.to_ascii_lowercase().as_str() {
        "today" | "t" | "now" => return Ok(today()),
        "yesterday" | "y" => {
            let d = Local::now().date_naive().pred_opt().ok_or("date out of range")?;
            return Ok(d.format("%Y-%m-%d").to_string());
        }
        "tomorrow" => {
            let d = Local::now().date_naive().succ_opt().ok_or("date out of range")?;
            return Ok(d.format("%Y-%m-%d").to_string());
        }
        _ => {}
    }

    let normalized = s.replace(['/', '.'], "-");
    let parts: Vec<&str> = normalized.split('-').collect();
    if parts.len() != 3 {
        return Err("expected a date like 2026-08-13".to_string());
    }

    let year: i32 = parts[0]
        .parse()
        .map_err(|_| "invalid year in date".to_string())?;
    let month: u32 = parts[1]
        .parse()
        .map_err(|_| "invalid month in date".to_string())?;
    let day: u32 = parts[2]
        .parse()
        .map_err(|_| "invalid day in date".to_string())?;

    // from_ymd_opt rejects impossible dates such as 2026-02-30 outright.
    let date = NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| format!("{s} is not a real calendar date"))?;
    Ok(date.format("%Y-%m-%d").to_string())
}

/// Parse and canonicalise a user-entered month into `YYYY-MM`.
///
/// Accepts `YYYY-MM`, `YYYY/MM`, a full date (whose month is taken), and an
/// empty input meaning the current month.
pub fn parse_month(input: &str) -> Result<String, String> {
    let s = input.trim();
    if s.is_empty() {
        return Ok(current_month());
    }
    if matches!(s.to_ascii_lowercase().as_str(), "this" | "current" | "now") {
        return Ok(current_month());
    }

    let normalized = s.replace(['/', '.'], "-");
    let parts: Vec<&str> = normalized.split('-').collect();
    if parts.len() < 2 {
        return Err("expected a month like 2026-08".to_string());
    }

    let year: i32 = parts[0]
        .parse()
        .map_err(|_| "invalid year in month".to_string())?;
    let month: u32 = parts[1]
        .parse()
        .map_err(|_| "invalid month".to_string())?;
    if !(1..=12).contains(&month) {
        return Err("month must be between 01 and 12".to_string());
    }
    if !(1..=9999).contains(&year) {
        return Err("year is out of range".to_string());
    }
    Ok(format!("{year:04}-{month:02}"))
}

/// Step a `YYYY-MM-DD` date by `delta` days, used by `+`/`-` in date fields.
/// Returns `None` if the input is not a valid date or the result would leave
/// the representable range.
pub fn shift_day(date: &str, delta: i32) -> Option<String> {
    let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let shifted = parsed.checked_add_signed(chrono::Duration::days(delta as i64))?;
    Some(shifted.format("%Y-%m-%d").to_string())
}

/// Step a `YYYY-MM` month by `delta` months, used by the month pickers in the
/// SUMMARY and BUDGETS views.
pub fn shift_month(month: &str, delta: i32) -> String {
    let (year, mon) = match split_month(month) {
        Some(v) => v,
        None => return current_month(),
    };

    // Work in absolute months so that wrapping across a year boundary in
    // either direction is a single division.
    let total = year as i64 * 12 + (mon as i64 - 1) + delta as i64;
    let new_year = total.div_euclid(12);
    let new_mon = total.rem_euclid(12) + 1;
    if !(1..=9999).contains(&new_year) {
        return month.to_string();
    }
    format!("{new_year:04}-{new_mon:02}")
}

/// A human-friendly month label, e.g. `August 2026`.
pub fn month_label(month: &str) -> String {
    const NAMES: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September",
        "October", "November", "December",
    ];
    match split_month(month) {
        Some((year, mon)) => format!("{} {year}", NAMES[(mon - 1) as usize]),
        None => month.to_string(),
    }
}

fn split_month(month: &str) -> Option<(i32, u32)> {
    let (y, m) = month.split_once('-')?;
    let year: i32 = y.parse().ok()?;
    let mon: u32 = m.parse().ok()?;
    if (1..=12).contains(&mon) { Some((year, mon)) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso_dates() {
        assert_eq!(parse_date("2026-08-13").unwrap(), "2026-08-13");
        assert_eq!(parse_date("2026/08/13").unwrap(), "2026-08-13");
        assert_eq!(parse_date("2026.8.3").unwrap(), "2026-08-03");
        assert_eq!(parse_date("2026-8-3").unwrap(), "2026-08-03");
    }

    #[test]
    fn rejects_impossible_dates() {
        assert!(parse_date("2026-02-30").is_err());
        assert!(parse_date("2026-13-01").is_err());
        assert!(parse_date("not-a-date").is_err());
        assert!(parse_date("2026-08").is_err());
        // 2024 is a leap year, 2026 is not.
        assert!(parse_date("2024-02-29").is_ok());
        assert!(parse_date("2026-02-29").is_err());
    }

    #[test]
    fn parses_months() {
        assert_eq!(parse_month("2026-08").unwrap(), "2026-08");
        assert_eq!(parse_month("2026/8").unwrap(), "2026-08");
        assert_eq!(parse_month("2026-08-13").unwrap(), "2026-08");
        assert!(parse_month("2026-00").is_err());
        assert!(parse_month("2026").is_err());
    }

    #[test]
    fn shifts_months_across_year_boundaries() {
        assert_eq!(shift_month("2026-08", 1), "2026-09");
        assert_eq!(shift_month("2026-12", 1), "2027-01");
        assert_eq!(shift_month("2026-01", -1), "2025-12");
        assert_eq!(shift_month("2026-08", -8), "2025-12");
        assert_eq!(shift_month("2026-08", 12), "2027-08");
        assert_eq!(shift_month("2026-08", -12), "2025-08");
    }

    #[test]
    fn shifts_days_across_month_and_year_boundaries() {
        assert_eq!(shift_day("2026-08-13", 1).unwrap(), "2026-08-14");
        assert_eq!(shift_day("2026-08-13", -1).unwrap(), "2026-08-12");
        assert_eq!(shift_day("2026-08-31", 1).unwrap(), "2026-09-01");
        assert_eq!(shift_day("2026-01-01", -1).unwrap(), "2025-12-31");
        assert_eq!(shift_day("2026-02-28", 1).unwrap(), "2026-03-01");
        assert_eq!(shift_day("2024-02-28", 1).unwrap(), "2024-02-29", "leap year");
        assert!(shift_day("not-a-date", 1).is_none());
    }

    #[test]
    fn labels_and_month_of() {
        assert_eq!(month_label("2026-08"), "August 2026");
        assert_eq!(month_label("2026-01"), "January 2026");
        assert_eq!(month_of("2026-08-13"), "2026-08");
    }
}
