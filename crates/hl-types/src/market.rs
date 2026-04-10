use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Level-2 orderbook snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HlOrderbook {
    /// Coin/asset symbol.
    pub coin: String,
    /// Bid levels (price, size).
    pub bids: Vec<(Decimal, Decimal)>,
    /// Ask levels (price, size).
    pub asks: Vec<(Decimal, Decimal)>,
    /// Timestamp in milliseconds.
    pub timestamp: u64,
}

impl HlOrderbook {
    /// Creates a new `HlOrderbook`.
    pub fn new(
        coin: String,
        bids: Vec<(Decimal, Decimal)>,
        asks: Vec<(Decimal, Decimal)>,
        timestamp: u64,
    ) -> Self {
        Self {
            coin,
            bids,
            asks,
            timestamp,
        }
    }
}

/// Static metadata for an asset listed on Hyperliquid.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlAssetInfo {
    /// Asset symbol (e.g. "BTC").
    pub coin: String,
    /// Asset index used in wire messages.
    pub asset_id: u32,
    /// Minimum order size.
    pub min_size: Decimal,
    /// Size decimal places.
    pub sz_decimals: u32,
    /// Price decimal places.
    pub px_decimals: u32,
}

impl HlAssetInfo {
    /// Creates a new `HlAssetInfo`.
    pub fn new(
        coin: String,
        asset_id: u32,
        min_size: Decimal,
        sz_decimals: u32,
        px_decimals: u32,
    ) -> Self {
        Self {
            coin,
            asset_id,
            min_size,
            sz_decimals,
            px_decimals,
        }
    }
}

/// Current funding rate for a perpetual.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlFundingRate {
    /// Coin/asset symbol.
    pub coin: String,
    /// Current funding rate.
    pub funding_rate: Decimal,
    /// Next funding time (ms since epoch).
    pub next_funding_time: u64,
}

impl HlFundingRate {
    /// Creates a new `HlFundingRate`.
    pub fn new(coin: String, funding_rate: Decimal, next_funding_time: u64) -> Self {
        Self {
            coin,
            funding_rate,
            next_funding_time,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn orderbook_serde_roundtrip() {
        let ob = HlOrderbook {
            coin: "BTC".into(),
            bids: vec![
                (
                    Decimal::from_str("50000.0").unwrap(),
                    Decimal::from_str("1.5").unwrap(),
                ),
                (
                    Decimal::from_str("49999.0").unwrap(),
                    Decimal::from_str("2.0").unwrap(),
                ),
            ],
            asks: vec![
                (
                    Decimal::from_str("50001.0").unwrap(),
                    Decimal::from_str("0.5").unwrap(),
                ),
                (
                    Decimal::from_str("50002.0").unwrap(),
                    Decimal::from_str("3.0").unwrap(),
                ),
            ],
            timestamp: 1700000000000,
        };
        let json = serde_json::to_string(&ob).unwrap();
        let parsed: HlOrderbook = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coin, "BTC");
        assert_eq!(parsed.bids.len(), 2);
        assert_eq!(parsed.asks.len(), 2);
        assert_eq!(parsed.bids[0].0, Decimal::from_str("50000.0").unwrap());
        assert_eq!(parsed.bids[0].1, Decimal::from_str("1.5").unwrap());
        assert_eq!(parsed.timestamp, 1700000000000);
    }

    #[test]
    fn orderbook_empty_levels_roundtrip() {
        let ob = HlOrderbook {
            coin: "SOL".into(),
            bids: vec![],
            asks: vec![],
            timestamp: 0,
        };
        let json = serde_json::to_string(&ob).unwrap();
        let parsed: HlOrderbook = serde_json::from_str(&json).unwrap();
        assert!(parsed.bids.is_empty());
        assert!(parsed.asks.is_empty());
    }

    #[test]
    fn asset_info_serde_roundtrip() {
        let info = HlAssetInfo {
            coin: "BTC".into(),
            asset_id: 0,
            min_size: Decimal::from_str("0.001").unwrap(),
            sz_decimals: 5,
            px_decimals: 1,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: HlAssetInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coin, "BTC");
        assert_eq!(parsed.asset_id, 0);
        assert_eq!(parsed.min_size, Decimal::from_str("0.001").unwrap());
        assert_eq!(parsed.sz_decimals, 5);
        assert_eq!(parsed.px_decimals, 1);
    }

    #[test]
    fn asset_info_camel_case_keys() {
        let info = HlAssetInfo {
            coin: "X".into(),
            asset_id: 0,
            min_size: Decimal::ZERO,
            sz_decimals: 0,
            px_decimals: 0,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("assetId"));
        assert!(json.contains("minSize"));
        assert!(json.contains("szDecimals"));
        assert!(json.contains("pxDecimals"));
    }

    #[test]
    fn funding_rate_serde_roundtrip() {
        let fr = HlFundingRate {
            coin: "ETH".into(),
            funding_rate: Decimal::from_str("0.0001").unwrap(),
            next_funding_time: 1700003600000,
        };
        let json = serde_json::to_string(&fr).unwrap();
        let parsed: HlFundingRate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coin, "ETH");
        assert_eq!(parsed.funding_rate, Decimal::from_str("0.0001").unwrap());
        assert_eq!(parsed.next_funding_time, 1700003600000);
    }

    #[test]
    fn funding_rate_camel_case_keys() {
        let fr = HlFundingRate {
            coin: "X".into(),
            funding_rate: Decimal::ZERO,
            next_funding_time: 0,
        };
        let json = serde_json::to_string(&fr).unwrap();
        assert!(json.contains("fundingRate"));
        assert!(json.contains("nextFundingTime"));
    }
}
