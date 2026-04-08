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
