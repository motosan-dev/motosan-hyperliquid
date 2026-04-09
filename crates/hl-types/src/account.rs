use serde::{Deserialize, Serialize};

/// A position held on Hyperliquid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlPosition {
    /// The coin/asset symbol.
    pub coin: String,
    /// Position size (negative for short).
    pub size: f64,
    /// Average entry price.
    pub entry_px: f64,
    /// Unrealised PnL.
    pub unrealized_pnl: f64,
    /// Leverage used.
    pub leverage: f64,
    /// Liquidation price, if applicable.
    pub liquidation_px: Option<f64>,
}

/// A trade fill on Hyperliquid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlFill {
    /// The coin/asset symbol.
    pub coin: String,
    /// Fill price.
    pub px: f64,
    /// Fill size.
    pub sz: f64,
    /// Whether the fill was on the buy side.
    pub is_buy: bool,
    /// Timestamp in milliseconds.
    pub timestamp: u64,
    /// Fee paid.
    pub fee: f64,
    /// Realized PnL from closing a position (0.0 if this fill opened a position).
    pub closed_pnl: f64,
}

/// Snapshot of an account's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlAccountState {
    /// Account equity.
    pub equity: f64,
    /// Available margin.
    pub margin_available: f64,
    /// Open positions.
    pub positions: Vec<HlPosition>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_serde_roundtrip() {
        let pos = HlPosition {
            coin: "BTC".into(),
            size: 0.5,
            entry_px: 60000.0,
            unrealized_pnl: 150.0,
            leverage: 10.0,
            liquidation_px: Some(54000.0),
        };
        let json = serde_json::to_string(&pos).unwrap();
        let parsed: HlPosition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coin, "BTC");
        assert!((parsed.size - 0.5).abs() < f64::EPSILON);
        assert!((parsed.entry_px - 60000.0).abs() < f64::EPSILON);
        assert!((parsed.unrealized_pnl - 150.0).abs() < f64::EPSILON);
        assert!((parsed.leverage - 10.0).abs() < f64::EPSILON);
        assert!((parsed.liquidation_px.unwrap() - 54000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn position_no_liquidation_px_roundtrip() {
        let pos = HlPosition {
            coin: "ETH".into(),
            size: -2.0,
            entry_px: 3000.0,
            unrealized_pnl: -50.0,
            leverage: 5.0,
            liquidation_px: None,
        };
        let json = serde_json::to_string(&pos).unwrap();
        let parsed: HlPosition = serde_json::from_str(&json).unwrap();
        assert!(parsed.liquidation_px.is_none());
        assert!(parsed.size < 0.0);
    }

    #[test]
    fn position_camel_case_keys() {
        let pos = HlPosition {
            coin: "X".into(),
            size: 1.0,
            entry_px: 1.0,
            unrealized_pnl: 0.0,
            leverage: 1.0,
            liquidation_px: None,
        };
        let json = serde_json::to_string(&pos).unwrap();
        assert!(json.contains("entryPx"));
        assert!(json.contains("unrealizedPnl"));
        assert!(json.contains("liquidationPx"));
    }

    #[test]
    fn fill_serde_roundtrip() {
        let fill = HlFill {
            coin: "ETH".into(),
            px: 3000.0,
            sz: 1.5,
            is_buy: true,
            timestamp: 1700000000000,
            fee: 0.75,
            closed_pnl: 0.0,
        };
        let json = serde_json::to_string(&fill).unwrap();
        let parsed: HlFill = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coin, "ETH");
        assert!((parsed.px - 3000.0).abs() < f64::EPSILON);
        assert!((parsed.sz - 1.5).abs() < f64::EPSILON);
        assert!(parsed.is_buy);
        assert_eq!(parsed.timestamp, 1700000000000);
        assert!((parsed.fee - 0.75).abs() < f64::EPSILON);
        assert!((parsed.closed_pnl - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn fill_camel_case_keys() {
        let fill = HlFill {
            coin: "X".into(),
            px: 1.0,
            sz: 1.0,
            is_buy: false,
            timestamp: 0,
            fee: 0.0,
            closed_pnl: 100.0,
        };
        let json = serde_json::to_string(&fill).unwrap();
        assert!(json.contains("isBuy"));
        assert!(json.contains("closedPnl"));
    }

    #[test]
    fn account_state_serde_roundtrip() {
        let state = HlAccountState {
            equity: 100000.0,
            margin_available: 50000.0,
            positions: vec![
                HlPosition {
                    coin: "BTC".into(),
                    size: 0.1,
                    entry_px: 60000.0,
                    unrealized_pnl: 0.0,
                    leverage: 10.0,
                    liquidation_px: None,
                },
            ],
        };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: HlAccountState = serde_json::from_str(&json).unwrap();
        assert!((parsed.equity - 100000.0).abs() < f64::EPSILON);
        assert!((parsed.margin_available - 50000.0).abs() < f64::EPSILON);
        assert_eq!(parsed.positions.len(), 1);
        assert_eq!(parsed.positions[0].coin, "BTC");
    }

    #[test]
    fn account_state_empty_positions_roundtrip() {
        let state = HlAccountState {
            equity: 0.0,
            margin_available: 0.0,
            positions: vec![],
        };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: HlAccountState = serde_json::from_str(&json).unwrap();
        assert!(parsed.positions.is_empty());
    }

    #[test]
    fn account_state_camel_case_keys() {
        let state = HlAccountState {
            equity: 1.0,
            margin_available: 1.0,
            positions: vec![],
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("marginAvailable"));
    }
}
