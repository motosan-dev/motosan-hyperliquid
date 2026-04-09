use serde::{Deserialize, Serialize};

/// Level-2 orderbook snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlOrderbook {
    /// Coin/asset symbol.
    pub coin: String,
    /// Bid levels (price, size).
    pub bids: Vec<(f64, f64)>,
    /// Ask levels (price, size).
    pub asks: Vec<(f64, f64)>,
    /// Timestamp in milliseconds.
    pub timestamp: u64,
}

/// Static metadata for an asset listed on Hyperliquid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlAssetInfo {
    /// Asset symbol (e.g. "BTC").
    pub coin: String,
    /// Asset index used in wire messages.
    pub asset_id: u32,
    /// Minimum order size.
    pub min_size: f64,
    /// Size decimal places.
    pub sz_decimals: u32,
    /// Price decimal places.
    pub px_decimals: u32,
}

/// Current funding rate for a perpetual.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlFundingRate {
    /// Coin/asset symbol.
    pub coin: String,
    /// Current funding rate.
    pub funding_rate: f64,
    /// Next funding time (ms since epoch).
    pub next_funding_time: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orderbook_serde_roundtrip() {
        let ob = HlOrderbook {
            coin: "BTC".into(),
            bids: vec![(50000.0, 1.5), (49999.0, 2.0)],
            asks: vec![(50001.0, 0.5), (50002.0, 3.0)],
            timestamp: 1700000000000,
        };
        let json = serde_json::to_string(&ob).unwrap();
        let parsed: HlOrderbook = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coin, "BTC");
        assert_eq!(parsed.bids.len(), 2);
        assert_eq!(parsed.asks.len(), 2);
        assert!((parsed.bids[0].0 - 50000.0).abs() < f64::EPSILON);
        assert!((parsed.bids[0].1 - 1.5).abs() < f64::EPSILON);
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
            min_size: 0.001,
            sz_decimals: 5,
            px_decimals: 1,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: HlAssetInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coin, "BTC");
        assert_eq!(parsed.asset_id, 0);
        assert!((parsed.min_size - 0.001).abs() < f64::EPSILON);
        assert_eq!(parsed.sz_decimals, 5);
        assert_eq!(parsed.px_decimals, 1);
    }

    #[test]
    fn asset_info_camel_case_keys() {
        let info = HlAssetInfo {
            coin: "X".into(),
            asset_id: 0,
            min_size: 0.0,
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
            funding_rate: 0.0001,
            next_funding_time: 1700003600000,
        };
        let json = serde_json::to_string(&fr).unwrap();
        let parsed: HlFundingRate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coin, "ETH");
        assert!((parsed.funding_rate - 0.0001).abs() < f64::EPSILON);
        assert_eq!(parsed.next_funding_time, 1700003600000);
    }

    #[test]
    fn funding_rate_camel_case_keys() {
        let fr = HlFundingRate {
            coin: "X".into(),
            funding_rate: 0.0,
            next_funding_time: 0,
        };
        let json = serde_json::to_string(&fr).unwrap();
        assert!(json.contains("fundingRate"));
        assert!(json.contains("nextFundingTime"));
    }
}
