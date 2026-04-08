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
