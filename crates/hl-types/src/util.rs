use rust_decimal::Decimal;

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

/// Format a [`Decimal`] into Hyperliquid canonical wire form.
///
/// Approximates the Python SDK's `float_to_wire`: at most 8 decimal places,
/// trailing zeros stripped, plain decimal (never scientific notation). Two
/// deliberate divergences: this rounds to 8 dp where Python *raises* if rounding
/// would lose precision, and `rust_decimal` rounds exact decimals where Python
/// rounds binary floats, so midpoint behaviour can differ.
pub fn normalize_wire(value: Decimal) -> String {
    // round_dp uses banker's rounding (MidpointNearestEven), matching the
    // Python SDK's float_to_wire (`f"{x:.8f}"`). normalize() strips trailing
    // zeros and converts -0 to 0; Decimal's Display never uses sci-notation.
    value.round_dp(8).normalize().to_string()
}

#[cfg(test)]
mod wire_tests {
    use super::normalize_wire;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn strips_trailing_zeros() {
        assert_eq!(normalize_wire(d("100.0")), "100");
        assert_eq!(normalize_wire(d("94500.000")), "94500");
        assert_eq!(normalize_wire(d("0.10")), "0.1");
        assert_eq!(normalize_wire(d("100.150")), "100.15");
    }

    #[test]
    fn caps_at_eight_decimals() {
        // round_dp(8) is banker's rounding (MidpointNearestEven); 0.000000005 is
        // an exact midpoint that rounds to even => "0". Use NON-midpoint inputs.
        assert_eq!(normalize_wire(d("0.000000006")), "0.00000001");
        assert_eq!(normalize_wire(d("0.123456789")), "0.12345679");
        assert_eq!(normalize_wire(d("0.000000005")), "0");
    }

    #[test]
    fn integer_is_plain() {
        assert_eq!(normalize_wire(d("42")), "42");
        assert_eq!(normalize_wire(Decimal::ZERO), "0");
    }
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
