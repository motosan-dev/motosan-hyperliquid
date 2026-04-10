use std::str::FromStr;

use rust_decimal::Decimal;

use crate::HlError;

/// Parse a `serde_json::Value` as a `Decimal`.
///
/// Accepts string-encoded decimals, JSON numbers, and handles `None`/`Null`.
pub fn parse_str_decimal(val: Option<&serde_json::Value>, field: &str) -> Result<Decimal, HlError> {
    match val {
        Some(serde_json::Value::String(s)) => Decimal::from_str(s).map_err(|_| {
            HlError::Parse(format!("cannot parse '{field}' value \"{s}\" as Decimal"))
        }),
        Some(serde_json::Value::Number(n)) => {
            let s = n.to_string();
            Decimal::from_str(&s)
                .map_err(|_| HlError::Parse(format!("cannot convert '{field}' number to Decimal")))
        }
        Some(serde_json::Value::Null) | None => {
            Err(HlError::Parse(format!("missing field '{field}'")))
        }
        Some(v) => Err(HlError::Parse(format!(
            "unexpected type for '{field}': expected string or number, got {v}"
        ))),
    }
}

/// Normalize a coin symbol by stripping common suffixes.
///
/// Removes `-PERP`, `-USDC`, and `-USD` suffixes so that e.g.
/// `"BTC-PERP"` becomes `"BTC"`.
pub fn normalize_coin(coin: &str) -> String {
    let s = coin.trim();
    for suffix in &["-PERP", "-USDC", "-USD"] {
        if let Some(stripped) = s.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_perp() {
        assert_eq!(normalize_coin("BTC-PERP"), "BTC");
    }

    #[test]
    fn strip_usdc() {
        assert_eq!(normalize_coin("ETH-USDC"), "ETH");
    }

    #[test]
    fn strip_usd() {
        assert_eq!(normalize_coin("SOL-USD"), "SOL");
    }

    #[test]
    fn no_suffix() {
        assert_eq!(normalize_coin("BTC"), "BTC");
    }

    #[test]
    fn handles_whitespace() {
        assert_eq!(normalize_coin("  BTC-PERP  "), "BTC");
    }

    #[test]
    fn longest_suffix_wins() {
        // "-USDC" should be stripped, not just "-USD"
        assert_eq!(normalize_coin("ETH-USDC"), "ETH");
    }
}
