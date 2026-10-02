//! Money is stored and computed as integer cents (`i64`) so that no rounding
//! error can ever creep into a ledger total. Yuan strings are only produced at
//! the edge, always with exactly two zero-padded decimal places.

/// Format cents as yuan with a fixed two decimal places, e.g. `1234.56`,
/// `800.05`, `0.00`. Negative values are prefixed with `-` (`-42.10`).
pub fn format_cents(cents: i64) -> String {
    // `unsigned_abs` so that i64::MIN does not overflow on negation.
    let abs = cents.unsigned_abs();
    let sign = if cents < 0 { "-" } else { "" };
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// Like [`format_cents`] but always carries an explicit sign, used where a
/// delta reads better with one (`+3588.42` / `-714.91`). Zero stays unsigned.
pub fn format_cents_signed(cents: i64) -> String {
    if cents > 0 {
        format!("+{}", format_cents(cents))
    } else {
        format_cents(cents)
    }
}

/// Parse a user-entered yuan amount into cents.
///
/// Accepts an optional currency symbol, digit grouping commas and up to two
/// decimal places: `1234.56`, `¥1,234.56`, `800.05`, `12`, `.5`, `12.`.
/// Rejects anything with more precision than a cent rather than silently
/// rounding a ledger entry.
pub fn parse_cents(input: &str) -> Result<i64, String> {
    let mut s = input.trim();
    if s.is_empty() {
        return Err("amount is required".to_string());
    }

    for symbol in ["¥", "￥", "CNY", "cny", "$"] {
        if let Some(rest) = s.strip_prefix(symbol) {
            s = rest.trim_start();
            break;
        }
    }

    let negative = match s.as_bytes().first() {
        Some(b'-') => {
            s = s[1..].trim_start();
            true
        }
        Some(b'+') => {
            s = s[1..].trim_start();
            false
        }
        _ => false,
    };

    let cleaned: String = s.chars().filter(|c| *c != ',' && *c != '_').collect();
    if cleaned.is_empty() {
        return Err("amount is required".to_string());
    }

    let (whole, frac) = match cleaned.split_once('.') {
        Some((w, f)) => (w, f),
        None => (cleaned.as_str(), ""),
    };

    if whole.is_empty() && frac.is_empty() {
        return Err("'.' is not an amount".to_string());
    }
    if let Some(bad) = whole.chars().chain(frac.chars()).find(|c| !c.is_ascii_digit()) {
        return Err(format!("invalid character '{bad}' in amount"));
    }
    if frac.len() > 2 {
        return Err("at most 2 decimal places (cents) are allowed".to_string());
    }

    let yuan: i64 = if whole.is_empty() {
        0
    } else {
        whole
            .parse::<i64>()
            .map_err(|_| "amount is too large".to_string())?
    };

    // Right-pad so "5" -> 50 cents and "05" -> 5 cents.
    let cents: i64 = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().unwrap() * 10,
        _ => frac.parse::<i64>().unwrap(),
    };

    let total = yuan
        .checked_mul(100)
        .and_then(|v| v.checked_add(cents))
        .ok_or_else(|| "amount is too large".to_string())?;

    Ok(if negative { -total } else { total })
}

/// Parse an amount that must be strictly positive, which is what every
/// transaction and budget entry requires (direction comes from the record kind).
pub fn parse_positive_cents(input: &str) -> Result<i64, String> {
    let cents = parse_cents(input)?;
    if cents <= 0 {
        return Err("amount must be greater than 0.00".to_string());
    }
    Ok(cents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_with_fixed_two_decimals() {
        assert_eq!(format_cents(123_456), "1234.56");
        assert_eq!(format_cents(80_005), "800.05");
        assert_eq!(format_cents(358_842), "3588.42");
        assert_eq!(format_cents(71_491), "714.91");
        assert_eq!(format_cents(120_000), "1200.00");
        assert_eq!(format_cents(15_678), "156.78");
        assert_eq!(format_cents(104_322), "1043.22");
        assert_eq!(format_cents(0), "0.00");
        assert_eq!(format_cents(5), "0.05");
        assert_eq!(format_cents(50), "0.50");
        assert_eq!(format_cents(-4210), "-42.10");
        assert_eq!(format_cents(i64::MIN), "-92233720368547758.08");
    }

    #[test]
    fn formats_signed() {
        assert_eq!(format_cents_signed(358_842), "+3588.42");
        assert_eq!(format_cents_signed(-71_491), "-714.91");
        assert_eq!(format_cents_signed(0), "0.00");
    }

    #[test]
    fn parses_plain_amounts() {
        assert_eq!(parse_cents("1234.56"), Ok(123_456));
        assert_eq!(parse_cents("800.05"), Ok(80_005));
        assert_eq!(parse_cents("12"), Ok(1200));
        assert_eq!(parse_cents("12."), Ok(1200));
        assert_eq!(parse_cents(".5"), Ok(50));
        assert_eq!(parse_cents("0.5"), Ok(50));
        assert_eq!(parse_cents("0.05"), Ok(5));
        assert_eq!(parse_cents("  42.10  "), Ok(4210));
    }

    #[test]
    fn parses_decorated_amounts() {
        assert_eq!(parse_cents("¥1,234.56"), Ok(123_456));
        assert_eq!(parse_cents("￥ 99"), Ok(9900));
        assert_eq!(parse_cents("CNY 1_000.00"), Ok(100_000));
        assert_eq!(parse_cents("-8.40"), Ok(-840));
        assert_eq!(parse_cents("+8.40"), Ok(840));
    }

    #[test]
    fn rejects_bad_amounts() {
        assert!(parse_cents("").is_err());
        assert!(parse_cents("abc").is_err());
        assert!(parse_cents("1.234").is_err());
        assert!(parse_cents(".").is_err());
        assert!(parse_cents("1.2.3").is_err());
        assert!(parse_positive_cents("0").is_err());
        assert!(parse_positive_cents("-1.00").is_err());
        assert!(parse_positive_cents("0.01").is_ok());
    }

    #[test]
    fn round_trips() {
        for cents in [0i64, 1, 9, 10, 99, 100, 101, 999_999, 123_456_789] {
            let text = format_cents(cents);
            assert_eq!(parse_cents(&text), Ok(cents), "round trip {text}");
        }
    }
}
