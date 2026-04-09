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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candle_serde_roundtrip() {
        let candle = HlCandle {
            timestamp: 1700000000000,
            open: 50000.0,
            high: 51000.0,
            low: 49500.0,
            close: 50500.0,
            volume: 1234.56,
        };
        let json = serde_json::to_string(&candle).unwrap();
        let parsed: HlCandle = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.timestamp, 1700000000000);
        assert!((parsed.open - 50000.0).abs() < f64::EPSILON);
        assert!((parsed.high - 51000.0).abs() < f64::EPSILON);
        assert!((parsed.low - 49500.0).abs() < f64::EPSILON);
        assert!((parsed.close - 50500.0).abs() < f64::EPSILON);
        assert!((parsed.volume - 1234.56).abs() < f64::EPSILON);
    }
}
