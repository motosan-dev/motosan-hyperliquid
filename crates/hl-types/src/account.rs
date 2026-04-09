use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A position held on Hyperliquid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlPosition {
    /// The coin/asset symbol.
    pub coin: String,
    /// Position size (negative for short).
    pub size: Decimal,
    /// Average entry price.
    pub entry_px: Decimal,
    /// Unrealised PnL.
    pub unrealized_pnl: Decimal,
    /// Leverage used.
    pub leverage: Decimal,
    /// Liquidation price, if applicable.
    pub liquidation_px: Option<Decimal>,
}

/// A trade fill on Hyperliquid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlFill {
    /// The coin/asset symbol.
    pub coin: String,
    /// Fill price.
    pub px: Decimal,
    /// Fill size.
    pub sz: Decimal,
    /// Whether the fill was on the buy side.
    pub is_buy: bool,
    /// Timestamp in milliseconds.
    pub timestamp: u64,
    /// Fee paid.
    pub fee: Decimal,
    /// Realized PnL from closing a position (0.0 if this fill opened a position).
    pub closed_pnl: Decimal,
}

/// Snapshot of an account's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlAccountState {
    /// Account equity.
    pub equity: Decimal,
    /// Available margin.
    pub margin_available: Decimal,
    /// Open positions.
    pub positions: Vec<HlPosition>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn position_serde_roundtrip() {
        let pos = HlPosition {
            coin: "BTC".into(),
            size: Decimal::from_str("0.5").unwrap(),
            entry_px: Decimal::from_str("60000.0").unwrap(),
            unrealized_pnl: Decimal::from_str("150.0").unwrap(),
            leverage: Decimal::from_str("10.0").unwrap(),
            liquidation_px: Some(Decimal::from_str("54000.0").unwrap()),
        };
        let json = serde_json::to_string(&pos).unwrap();
        let parsed: HlPosition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coin, "BTC");
        assert_eq!(parsed.size, Decimal::from_str("0.5").unwrap());
        assert_eq!(parsed.entry_px, Decimal::from_str("60000.0").unwrap());
        assert_eq!(parsed.unrealized_pnl, Decimal::from_str("150.0").unwrap());
        assert_eq!(parsed.leverage, Decimal::from_str("10.0").unwrap());
        assert_eq!(
            parsed.liquidation_px,
            Some(Decimal::from_str("54000.0").unwrap())
        );
    }

    #[test]
    fn position_no_liquidation_px_roundtrip() {
        let pos = HlPosition {
            coin: "ETH".into(),
            size: Decimal::from_str("-2.0").unwrap(),
            entry_px: Decimal::from_str("3000.0").unwrap(),
            unrealized_pnl: Decimal::from_str("-50.0").unwrap(),
            leverage: Decimal::from_str("5.0").unwrap(),
            liquidation_px: None,
        };
        let json = serde_json::to_string(&pos).unwrap();
        let parsed: HlPosition = serde_json::from_str(&json).unwrap();
        assert!(parsed.liquidation_px.is_none());
        assert!(parsed.size < Decimal::ZERO);
    }

    #[test]
    fn position_camel_case_keys() {
        let pos = HlPosition {
            coin: "X".into(),
            size: Decimal::ONE,
            entry_px: Decimal::ONE,
            unrealized_pnl: Decimal::ZERO,
            leverage: Decimal::ONE,
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
            px: Decimal::from_str("3000.0").unwrap(),
            sz: Decimal::from_str("1.5").unwrap(),
            is_buy: true,
            timestamp: 1700000000000,
            fee: Decimal::from_str("0.75").unwrap(),
            closed_pnl: Decimal::ZERO,
        };
        let json = serde_json::to_string(&fill).unwrap();
        let parsed: HlFill = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coin, "ETH");
        assert_eq!(parsed.px, Decimal::from_str("3000.0").unwrap());
        assert_eq!(parsed.sz, Decimal::from_str("1.5").unwrap());
        assert!(parsed.is_buy);
        assert_eq!(parsed.timestamp, 1700000000000);
        assert_eq!(parsed.fee, Decimal::from_str("0.75").unwrap());
        assert_eq!(parsed.closed_pnl, Decimal::ZERO);
    }

    #[test]
    fn fill_camel_case_keys() {
        let fill = HlFill {
            coin: "X".into(),
            px: Decimal::ONE,
            sz: Decimal::ONE,
            is_buy: false,
            timestamp: 0,
            fee: Decimal::ZERO,
            closed_pnl: Decimal::from_str("100.0").unwrap(),
        };
        let json = serde_json::to_string(&fill).unwrap();
        assert!(json.contains("isBuy"));
        assert!(json.contains("closedPnl"));
    }

    #[test]
    fn account_state_serde_roundtrip() {
        let state = HlAccountState {
            equity: Decimal::from_str("100000.0").unwrap(),
            margin_available: Decimal::from_str("50000.0").unwrap(),
            positions: vec![HlPosition {
                coin: "BTC".into(),
                size: Decimal::from_str("0.1").unwrap(),
                entry_px: Decimal::from_str("60000.0").unwrap(),
                unrealized_pnl: Decimal::ZERO,
                leverage: Decimal::from_str("10.0").unwrap(),
                liquidation_px: None,
            }],
        };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: HlAccountState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.equity, Decimal::from_str("100000.0").unwrap());
        assert_eq!(
            parsed.margin_available,
            Decimal::from_str("50000.0").unwrap()
        );
        assert_eq!(parsed.positions.len(), 1);
        assert_eq!(parsed.positions[0].coin, "BTC");
    }

    #[test]
    fn account_state_empty_positions_roundtrip() {
        let state = HlAccountState {
            equity: Decimal::ZERO,
            margin_available: Decimal::ZERO,
            positions: vec![],
        };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: HlAccountState = serde_json::from_str(&json).unwrap();
        assert!(parsed.positions.is_empty());
    }

    #[test]
    fn account_state_camel_case_keys() {
        let state = HlAccountState {
            equity: Decimal::ONE,
            margin_available: Decimal::ONE,
            positions: vec![],
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("marginAvailable"));
    }
}
