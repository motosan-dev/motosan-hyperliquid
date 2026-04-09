use serde::{Deserialize, Serialize};

/// An OHLCV candlestick bar from Hyperliquid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HlCandle {
    /// Candle open time in milliseconds since the UNIX epoch.
    pub timestamp: u64,
    /// Opening price.
    pub open: f64,
    /// Highest price during the interval.
    pub high: f64,
    /// Lowest price during the interval.
    pub low: f64,
    /// Closing price.
    pub close: f64,
    /// Trade volume during the interval.
    pub volume: f64,
}
