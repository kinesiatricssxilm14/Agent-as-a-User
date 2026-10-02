//! Human readable size formatting and parsing.
//!
//! Every formatter emits at least two fixed decimal places with zero padding,
//! e.g. `21.56 GB`, `149.00 MB`, `1234.56 MB`.

pub const KIB: u64 = 1024;
pub const MIB: u64 = 1024 * KIB;
pub const GIB: u64 = 1024 * MIB;
pub const TIB: u64 = 1024 * GIB;

/// Format as GB with two fixed decimals: `21.56 GB`.
pub fn gb(bytes: u64) -> String {
    format!("{:.2} GB", bytes as f64 / GIB as f64)
}

/// Format as MB with two fixed decimals: `149.00 MB`.
pub fn mb(bytes: u64) -> String {
    format!("{:.2} MB", bytes as f64 / MIB as f64)
}

/// Format as KB with two fixed decimals: `12.50 KB`.
pub fn kb(bytes: u64) -> String {
    format!("{:.2} KB", bytes as f64 / KIB as f64)
}

/// Totals for directories: GB once the value reaches 1 GB, MB below that.
///
/// This keeps directory totals and the largest item in GB (as required) while
/// smaller scopes stay readable in MB.
pub fn gb_or_mb(bytes: u64) -> String {
    if bytes >= GIB { gb(bytes) } else { mb(bytes) }
}

/// Both units at once: `21.56 GB (22077.44 MB)`.
///
/// Directory totals and the largest item are quoted in GB as required, while the
/// MB figure stays on the same screen so smaller scopes are still meaningful.
pub fn size_dual(bytes: u64) -> String {
    format!("{} ({})", gb(bytes), mb(bytes))
}

/// Pick the most readable unit, never losing the two decimal places.
pub fn human(bytes: u64) -> String {
    if bytes >= TIB {
        format!("{:.2} TB", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        gb(bytes)
    } else if bytes >= MIB {
        mb(bytes)
    } else if bytes >= KIB {
        kb(bytes)
    } else {
        format!("{:.2} B", bytes as f64)
    }
}

/// Percentage with two fixed decimals: `42.00 %`.
pub fn percent(part: u64, whole: u64) -> String {
    if whole == 0 {
        return "0.00 %".to_string();
    }
    format!("{:.2} %", part as f64 * 100.0 / whole as f64)
}

/// Percentage from an already-computed share, same two decimals.
pub fn percent_short(share: f64) -> String {
    let share = if share.is_finite() { share } else { 0.0 };
    format!("{:.2} %", share * 100.0)
}

/// Parse a user supplied size threshold.
///
/// Accepts a bare byte count (`1048576`) or a value with a unit suffix in any
/// case, with or without a space and with or without the `B`/`iB` tail:
/// `10K`, `10 kb`, `1.5MiB`, `2G`, `0.5 TB`.
pub fn parse_size(input: &str) -> Result<u64, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("empty size".to_string());
    }

    let digits_end = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(s.len());
    let (number, suffix) = s.split_at(digits_end);
    let number = number.replace(',', "");
    let value: f64 = number
        .parse()
        .map_err(|_| format!("`{}` is not a number", number))?;
    if value < 0.0 || !value.is_finite() {
        return Err("size must be a positive number".to_string());
    }

    let unit = suffix.trim().to_ascii_lowercase();
    let unit = unit.trim_end_matches("ib").trim_end_matches('b');
    let multiplier = match unit {
        "" => 1_u64,
        "k" => KIB,
        "m" => MIB,
        "g" => GIB,
        "t" => TIB,
        other => return Err(format!("unknown unit `{}` (use B/KB/MB/GB/TB)", other)),
    };

    Ok((value * multiplier as f64).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_fixed_decimals_are_always_present() {
        assert_eq!(gb(23_163_101_675), "21.57 GB");
        assert_eq!(mb(156_237_824), "149.00 MB");
        assert_eq!(mb(1_294_615_609), "1234.64 MB");
        assert_eq!(gb_or_mb(156_237_824), "149.00 MB");
        assert_eq!(gb_or_mb(2 * GIB), "2.00 GB");
        assert_eq!(human(0), "0.00 B");
        assert_eq!(human(1536), "1.50 KB");
        assert_eq!(size_dual(2 * GIB), "2.00 GB (2048.00 MB)");
    }

    #[test]
    fn parses_sizes_with_and_without_units() {
        assert_eq!(parse_size("1024"), Ok(1024));
        assert_eq!(parse_size("10K"), Ok(10 * KIB));
        assert_eq!(parse_size("10 kb"), Ok(10 * KIB));
        assert_eq!(parse_size("1.5MiB"), Ok(1024 * KIB + 512 * KIB));
        assert_eq!(parse_size("2g"), Ok(2 * GIB));
        assert_eq!(parse_size("0.5 TB"), Ok(TIB / 2));
        assert!(parse_size("").is_err());
        assert!(parse_size("abc").is_err());
        assert!(parse_size("12 zb").is_err());
    }

    #[test]
    fn percentages_are_zero_padded() {
        assert_eq!(percent(1, 2), "50.00 %");
        assert_eq!(percent(0, 0), "0.00 %");
        assert_eq!(percent_short(0.5), "50.00 %");
        assert_eq!(percent_short(f64::NAN), "0.00 %");
    }
}
