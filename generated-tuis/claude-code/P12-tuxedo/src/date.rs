//! Minimal proleptic-Gregorian date helpers.
//!
//! `tooll` only needs three things from a calendar: today's date (to highlight
//! overdue tasks and to stamp completion dates), validation of `due:YYYY-MM-DD`
//! values typed by the user, and an ordering key for due dates. Pulling a full
//! date/time crate in for that is overkill, so the two classic
//! days-since-epoch conversions are implemented here.

use std::time::{SystemTime, UNIX_EPOCH};

/// A calendar date as `(year, month, day)`.
pub type Ymd = (i32, u32, u32);

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m as i64 - 3 } else { m as i64 + 9 };
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
pub fn civil_from_days(z: i64) -> Ymd {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

/// Number of days in `month` of `year`.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Today's date (UTC). The wall clock is only used for relative due-date hints,
/// so a timezone database is deliberately not required.
pub fn today() -> Ymd {
    let secs = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    // Floor division so pre-1970 clocks still land on the right day.
    let days = secs.div_euclid(86_400);
    civil_from_days(days)
}

/// Format a date as `YYYY-MM-DD`.
pub fn format_ymd((y, m, d): Ymd) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

/// Parse a strict `YYYY-MM-DD` date, rejecting impossible days such as
/// `2026-02-30`. Returns `None` when the text is not a valid date.
pub fn parse_ymd(s: &str) -> Option<Ymd> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    if !b.iter().enumerate().all(|(i, c)| {
        if i == 4 || i == 7 {
            true
        } else {
            c.is_ascii_digit()
        }
    }) {
        return None;
    }
    let y: i32 = s[0..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    if m == 0 || m > 12 || d == 0 || d > days_in_month(y, m) {
        return None;
    }
    Some((y, m, d))
}

/// True when `s` looks like a todo.txt date field (`YYYY-MM-DD`).
pub fn is_date(s: &str) -> bool {
    parse_ymd(s).is_some()
}

/// Signed day difference `date - today`. Negative means overdue.
pub fn days_until(date: Ymd, today: Ymd) -> i64 {
    days_from_civil(date.0, date.1, date.2) - days_from_civil(today.0, today.1, today.2)
}

/// Human wording for a due date relative to today, e.g. `overdue by 3d`.
pub fn relative_wording(date: Ymd, today: Ymd) -> String {
    let delta = days_until(date, today);
    match delta {
        0 => "due today".to_string(),
        1 => "due tomorrow".to_string(),
        -1 => "overdue by 1 day".to_string(),
        d if d < 0 => format!("overdue by {} days", -d),
        d => format!("due in {d} days"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_roundtrip_across_centuries() {
        for &(y, m, d) in &[
            (1970, 1, 1),
            (1969, 12, 31),
            (2000, 2, 29),
            (2026, 3, 15),
            (2029, 12, 12),
            (2100, 3, 1),
            (1899, 7, 4),
        ] {
            let days = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(days), (y, m, d), "{y}-{m}-{d}");
        }
    }

    #[test]
    fn epoch_is_day_zero() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn parses_only_real_dates() {
        assert_eq!(parse_ymd("2026-03-15"), Some((2026, 3, 15)));
        assert_eq!(parse_ymd("2024-02-29"), Some((2024, 2, 29)));
        assert_eq!(parse_ymd("2023-02-29"), None);
        assert_eq!(parse_ymd("2026-13-01"), None);
        assert_eq!(parse_ymd("2026-00-10"), None);
        assert_eq!(parse_ymd("2026-3-15"), None);
        assert_eq!(parse_ymd("not-a-date"), None);
        assert_eq!(parse_ymd(""), None);
        assert_eq!(parse_ymd("2026-03-15x"), None);
    }

    #[test]
    fn leap_years() {
        assert!(is_leap_year(2024));
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2026));
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
    }

    #[test]
    fn relative_days() {
        let today = (2026, 3, 15);
        assert_eq!(days_until((2026, 3, 15), today), 0);
        assert_eq!(days_until((2026, 3, 18), today), 3);
        assert_eq!(days_until((2026, 3, 10), today), -5);
        assert!(relative_wording((2026, 3, 10), today).contains("overdue"));
    }
}
