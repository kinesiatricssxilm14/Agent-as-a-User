//! Human readable size formatting and parsing.
//!
//! All formats use two fixed decimal places with zero-padding (e.g. `21.56 GB`,
//! `149.00 MB`), matching the requirement that sizes be shown with at least two
//! decimal places.

const GB: f64 = 1_000_000_000.0;
const MB: f64 = 1_000_000.0;
const KB: f64 = 1_000.0;

/// Format a byte count choosing the unit by context (GB for large values, MB,
/// KB or B for smaller ones), always with two padded decimal places.
pub fn format_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{:.2} B", b)
    }
}

/// Format a byte count using only GB or MB. Directory totals and the largest
/// item are displayed with this helper so they read like `21.56 GB` or
/// `149.00 MB`.
pub fn format_gb_mb(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else {
        format!("{:.2} MB", b / MB)
    }
}

/// Parse a user supplied size threshold such as `100M`, `2G`, `500KB`,
/// `1.5GiB` or plain byte counts (case-insensitive). Returns `None` for input
/// that cannot be interpreted.
pub fn parse_size(input: &str) -> Option<u64> {
    let s = input.trim();
    if s.is_empty() {
        return Some(0);
    }
    let upper: String = s.chars().map(|c| c.to_ascii_uppercase()).collect();
    let split = upper.find(|c: char| !(c.is_ascii_digit() || c == '.'));
    let (num_part, unit_part) = match split {
        Some(i) => (&upper[..i], upper[i..].trim()),
        None => (upper.as_str(), ""),
    };

    let value: f64 = num_part.trim().parse().ok()?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }

    let multiplier: f64 = match unit_part {
        "" | "B" => 1.0,
        "K" | "KB" => 1_000.0,
        "KI" | "KIB" => 1_024.0,
        "M" | "MB" => 1_000_000.0,
        "MI" | "MIB" => 1_048_576.0,
        "G" | "GB" => 1_000_000_000.0,
        "GI" | "GIB" => 1_073_741_824.0,
        "T" | "TB" => 1_000_000_000_000.0,
        "TI" | "TIB" => 1_099_511_627_776.0,
        _ => return None,
    };

    Some((value * multiplier) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_with_two_decimals() {
        assert_eq!(format_gb_mb(21_560_000_000), "21.56 GB");
        assert_eq!(format_gb_mb(149_000_000), "149.00 MB");
        assert_eq!(format_size(1234_560_000), "1.23 GB");
    }

    #[test]
    fn parses_suffixes() {
        assert_eq!(parse_size("100M"), Some(100_000_000));
        assert_eq!(parse_size("2G"), Some(2_000_000_000));
        assert_eq!(parse_size("500KB"), Some(500_000));
        assert_eq!(parse_size("1.5GiB"), Some(1_610_612_736));
        assert_eq!(parse_size("12345"), Some(12345));
        assert_eq!(parse_size(""), Some(0));
        assert_eq!(parse_size("nope"), None);
    }
}
