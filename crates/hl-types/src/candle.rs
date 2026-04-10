use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// An OHLCV candlestick bar from Hyperliquid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HlCandle {
    /// Candle open time in milliseconds since the UNIX epoch.
    pub timestamp: u64,
    /// Opening price.
    pub open: Decimal,
    /// Highest price during the interval.
    pub high: Decimal,
    /// Lowest price during the interval.
    pub low: Decimal,
    /// Closing price.
    pub close: Decimal,
    /// Trade volume during the interval.
    pub volume: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn candle_serde_roundtrip() {
        let candle = HlCandle {
            timestamp: 1700000000000,
            open: Decimal::from_str("50000.0").unwrap(),
            high: Decimal::from_str("51000.0").unwrap(),
            low: Decimal::from_str("49500.0").unwrap(),
            close: Decimal::from_str("50500.0").unwrap(),
            volume: Decimal::from_str("1234.56").unwrap(),
        };
        let json = serde_json::to_string(&candle).unwrap();
        let parsed: HlCandle = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.timestamp, 1700000000000);
        assert_eq!(parsed.open, Decimal::from_str("50000.0").unwrap());
        assert_eq!(parsed.high, Decimal::from_str("51000.0").unwrap());
        assert_eq!(parsed.low, Decimal::from_str("49500.0").unwrap());
        assert_eq!(parsed.close, Decimal::from_str("50500.0").unwrap());
        assert_eq!(parsed.volume, Decimal::from_str("1234.56").unwrap());
    }
}
