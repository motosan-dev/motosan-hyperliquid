use std::str::FromStr;

use rust_decimal::Decimal;

use crate::order::Side;
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

/// Parse the mid price from an `l2Book` JSON response.
///
/// Extracts the best bid and best ask from the `levels` array and returns
/// `(best_bid + best_ask) / 2`.
pub fn parse_mid_price_from_l2book(resp: &serde_json::Value) -> Result<Decimal, HlError> {
    let levels = resp
        .get("levels")
        .and_then(|v| v.as_array())
        .ok_or_else(|| HlError::Parse("l2Book response missing 'levels' array".into()))?;

    if levels.len() < 2 {
        return Err(HlError::Parse(
            "l2Book 'levels' array has fewer than 2 entries".into(),
        ));
    }

    let best_bid = levels[0]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|e| e.get("px"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| HlError::Parse("l2Book: missing best bid price".into()))?;

    let best_ask = levels[1]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|e| e.get("px"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| HlError::Parse("l2Book: missing best ask price".into()))?;

    let bid: Decimal = Decimal::from_str(best_bid)
        .map_err(|e| HlError::Parse(format!("l2Book: invalid bid price '{}': {}", best_bid, e)))?;
    let ask: Decimal = Decimal::from_str(best_ask)
        .map_err(|e| HlError::Parse(format!("l2Book: invalid ask price '{}': {}", best_ask, e)))?;

    Ok((bid + ask) / Decimal::from(2))
}

/// Parse a position's size and side from a `clearinghouseState` JSON response.
///
/// Searches the `assetPositions` array for a matching coin and returns
/// `(side, abs_size)` where side is `Buy` (long) for positive szi and
/// `Sell` (short) for negative szi.
pub fn parse_position_szi(
    resp: &serde_json::Value,
    coin: &str,
) -> Result<(Side, Decimal), HlError> {
    let positions = resp
        .get("assetPositions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| HlError::Parse("clearinghouseState: missing 'assetPositions'".into()))?;

    for pos in positions {
        let position = &pos["position"];
        let pos_coin = position.get("coin").and_then(|v| v.as_str()).unwrap_or("");
        if pos_coin.to_uppercase() != coin.to_uppercase() {
            continue;
        }
        let szi_str = position
            .get("szi")
            .and_then(|v| v.as_str())
            .ok_or_else(|| HlError::Parse("clearinghouseState: missing 'szi' field".into()))?;
        let szi: Decimal = Decimal::from_str(szi_str).map_err(|e| {
            HlError::Parse(format!(
                "clearinghouseState: invalid szi '{}': {}",
                szi_str, e
            ))
        })?;

        if szi.is_zero() {
            return Err(HlError::Parse(format!(
                "market_close: position size for {} is zero",
                coin
            )));
        }

        let side = if szi > Decimal::ZERO {
            Side::Buy // long
        } else {
            Side::Sell // short
        };
        return Ok((side, szi.abs()));
    }

    Err(HlError::Parse(format!(
        "market_close: no open position found for {}",
        coin
    )))
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

    // -- parse_mid_price_from_l2book tests --

    fn l2book_json(bid: &str, ask: &str) -> serde_json::Value {
        serde_json::json!({
            "levels": [
                [{"px": bid, "sz": "1.0", "n": 1}],
                [{"px": ask, "sz": "1.0", "n": 1}]
            ]
        })
    }

    #[test]
    fn mid_price_basic() {
        let resp = l2book_json("90000", "90100");
        let mid = parse_mid_price_from_l2book(&resp).unwrap();
        assert_eq!(mid, Decimal::from_str("90050").unwrap());
    }

    #[test]
    fn mid_price_decimal_values() {
        let resp = l2book_json("1.50", "2.50");
        let mid = parse_mid_price_from_l2book(&resp).unwrap();
        assert_eq!(mid, Decimal::from(2));
    }

    #[test]
    fn mid_price_missing_levels() {
        let resp = serde_json::json!({});
        assert!(parse_mid_price_from_l2book(&resp).is_err());
    }

    #[test]
    fn mid_price_too_few_levels() {
        let resp = serde_json::json!({ "levels": [[{"px": "100", "sz": "1"}]] });
        assert!(parse_mid_price_from_l2book(&resp).is_err());
    }

    #[test]
    fn mid_price_empty_bid_level() {
        let resp = serde_json::json!({
            "levels": [
                [],
                [{"px": "100", "sz": "1"}]
            ]
        });
        assert!(parse_mid_price_from_l2book(&resp).is_err());
    }

    // -- parse_position_szi tests --

    fn clearinghouse_json(coin: &str, szi: &str) -> serde_json::Value {
        serde_json::json!({
            "assetPositions": [
                {
                    "position": {
                        "coin": coin,
                        "szi": szi
                    }
                }
            ]
        })
    }

    #[test]
    fn position_szi_long() {
        let resp = clearinghouse_json("BTC", "1.5");
        let (side, size) = parse_position_szi(&resp, "BTC").unwrap();
        assert_eq!(side, Side::Buy);
        assert_eq!(size, Decimal::from_str("1.5").unwrap());
    }

    #[test]
    fn position_szi_short() {
        let resp = clearinghouse_json("ETH", "-2.0");
        let (side, size) = parse_position_szi(&resp, "ETH").unwrap();
        assert_eq!(side, Side::Sell);
        assert_eq!(size, Decimal::from(2));
    }

    #[test]
    fn position_szi_case_insensitive() {
        let resp = clearinghouse_json("btc", "0.5");
        let (side, _) = parse_position_szi(&resp, "BTC").unwrap();
        assert_eq!(side, Side::Buy);
    }

    #[test]
    fn position_szi_zero_errors() {
        let resp = clearinghouse_json("BTC", "0");
        assert!(parse_position_szi(&resp, "BTC").is_err());
    }

    #[test]
    fn position_szi_not_found() {
        let resp = clearinghouse_json("BTC", "1.0");
        assert!(parse_position_szi(&resp, "ETH").is_err());
    }

    #[test]
    fn position_szi_missing_asset_positions() {
        let resp = serde_json::json!({});
        assert!(parse_position_szi(&resp, "BTC").is_err());
    }
}
