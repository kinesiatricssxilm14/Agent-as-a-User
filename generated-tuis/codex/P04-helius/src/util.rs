use anyhow::{bail, Result};
use chrono::NaiveDate;

pub fn money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let n = cents.unsigned_abs();
    format!("{}{:.2}", sign, n as f64 / 100.0)
}

pub fn parse_money(s: &str) -> Result<i64> {
    let s = s.trim();
    if s.is_empty() {
        bail!("amount is required")
    }
    if s.starts_with('-') {
        bail!("amount must be positive")
    }
    let parts: Vec<_> = s.split('.').collect();
    if parts.len() > 2 || parts[0].is_empty() {
        bail!("enter an amount such as 123.45")
    }
    let whole: i64 = parts[0]
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid amount"))?;
    let frac = match parts.get(1).copied().unwrap_or("") {
        "" => 0,
        x if x.len() == 1 => {
            x.parse::<i64>()
                .map_err(|_| anyhow::anyhow!("invalid amount"))?
                * 10
        }
        x if x.len() == 2 => x
            .parse::<i64>()
            .map_err(|_| anyhow::anyhow!("invalid amount"))?,
        _ => bail!("use no more than two decimal places"),
    };
    let cents = whole
        .checked_mul(100)
        .and_then(|x| x.checked_add(frac))
        .ok_or_else(|| anyhow::anyhow!("amount is too large"))?;
    if cents <= 0 {
        bail!("amount must be greater than zero")
    };
    Ok(cents)
}

pub fn valid_date(s: &str) -> bool {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}
pub fn valid_month(s: &str) -> bool {
    NaiveDate::parse_from_str(&format!("{}-01", s), "%Y-%m-%d").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_money() {
        assert_eq!(money(80005), "800.05");
        assert_eq!(money(-71491), "-714.91");
    }
    #[test]
    fn parses_money() {
        assert_eq!(parse_money("12").unwrap(), 1200);
        assert_eq!(parse_money("12.3").unwrap(), 1230);
        assert!(parse_money("1.234").is_err());
    }
}
