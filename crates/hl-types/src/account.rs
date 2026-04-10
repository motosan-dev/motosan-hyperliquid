use std::collections::HashMap;

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

/// Summary of a vault the user participates in.
///
/// Returned by the `vaultSummaries` info endpoint. Fields that the API may
/// add in the future are captured in `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlVaultSummary {
    /// On-chain vault address.
    pub vault_address: String,
    /// Human-readable vault name.
    pub name: String,
    /// Vault leader's equity (USDC).
    #[serde(default)]
    pub leader_equity: Option<Decimal>,
    /// Total follower equity (USDC).
    #[serde(default)]
    pub follower_equity: Option<Decimal>,
    /// Vault's all-time PnL.
    #[serde(default)]
    pub all_time_pnl: Option<Decimal>,
    /// Any additional fields returned by the API.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Detailed information about a specific vault.
///
/// Returned by the `vaultDetails` info endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlVaultDetails {
    /// Vault name.
    pub name: String,
    /// On-chain vault address.
    pub vault_address: String,
    /// Vault leader address.
    #[serde(default)]
    pub leader: Option<String>,
    /// Portfolio state of the vault (positions, equity, etc.).
    #[serde(default)]
    pub portfolio: Option<serde_json::Value>,
    /// Number of followers.
    #[serde(default)]
    pub follower_count: Option<u64>,
    /// Any additional fields returned by the API.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// An extra (sub-)agent approval entry.
///
/// Returned by the `extraAgents` info endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlExtraAgent {
    /// Address of the approved agent.
    pub address: String,
    /// Human-readable agent name, if set.
    #[serde(default)]
    pub name: Option<String>,
    /// Any additional fields returned by the API.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
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

    #[test]
    fn vault_summary_serde_roundtrip() {
        let json = serde_json::json!({
            "vaultAddress": "0xabc123",
            "name": "My Vault",
            "leaderEquity": "10000.0",
            "followerEquity": "50000.0",
            "allTimePnl": "2500.0"
        });
        let parsed: HlVaultSummary = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.vault_address, "0xabc123");
        assert_eq!(parsed.name, "My Vault");
        assert_eq!(
            parsed.leader_equity,
            Some(Decimal::from_str("10000.0").unwrap())
        );
        assert_eq!(
            parsed.follower_equity,
            Some(Decimal::from_str("50000.0").unwrap())
        );
        assert_eq!(
            parsed.all_time_pnl,
            Some(Decimal::from_str("2500.0").unwrap())
        );
    }

    #[test]
    fn vault_summary_minimal_fields() {
        let json = serde_json::json!({
            "vaultAddress": "0xdef456",
            "name": "Minimal Vault"
        });
        let parsed: HlVaultSummary = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.vault_address, "0xdef456");
        assert_eq!(parsed.name, "Minimal Vault");
        assert!(parsed.leader_equity.is_none());
        assert!(parsed.follower_equity.is_none());
        assert!(parsed.all_time_pnl.is_none());
    }

    #[test]
    fn vault_summary_extra_fields_captured() {
        let json = serde_json::json!({
            "vaultAddress": "0x111",
            "name": "V",
            "someNewField": 42
        });
        let parsed: HlVaultSummary = serde_json::from_value(json).unwrap();
        assert_eq!(
            parsed.extra.get("someNewField").unwrap(),
            &serde_json::json!(42)
        );
    }

    #[test]
    fn vault_summary_camel_case_keys() {
        let summary = HlVaultSummary {
            vault_address: "0x1".into(),
            name: "V".into(),
            leader_equity: Some(Decimal::ONE),
            follower_equity: None,
            all_time_pnl: None,
            extra: HashMap::new(),
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("vaultAddress"));
        assert!(json.contains("leaderEquity"));
    }

    #[test]
    fn vault_details_serde_roundtrip() {
        let json = serde_json::json!({
            "name": "Alpha Vault",
            "vaultAddress": "0xvault",
            "leader": "0xleader",
            "portfolio": {"equity": "100000"},
            "followerCount": 25
        });
        let parsed: HlVaultDetails = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.name, "Alpha Vault");
        assert_eq!(parsed.vault_address, "0xvault");
        assert_eq!(parsed.leader.as_deref(), Some("0xleader"));
        assert!(parsed.portfolio.is_some());
        assert_eq!(parsed.follower_count, Some(25));
    }

    #[test]
    fn vault_details_minimal_fields() {
        let json = serde_json::json!({
            "name": "Min",
            "vaultAddress": "0xmin"
        });
        let parsed: HlVaultDetails = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.name, "Min");
        assert!(parsed.leader.is_none());
        assert!(parsed.portfolio.is_none());
        assert!(parsed.follower_count.is_none());
    }

    #[test]
    fn vault_details_extra_fields_captured() {
        let json = serde_json::json!({
            "name": "V",
            "vaultAddress": "0x1",
            "customMetric": "hello"
        });
        let parsed: HlVaultDetails = serde_json::from_value(json).unwrap();
        assert_eq!(
            parsed.extra.get("customMetric").unwrap(),
            &serde_json::json!("hello")
        );
    }

    #[test]
    fn extra_agent_serde_roundtrip() {
        let json = serde_json::json!({
            "address": "0xagent1",
            "name": "Trading Bot"
        });
        let parsed: HlExtraAgent = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.address, "0xagent1");
        assert_eq!(parsed.name.as_deref(), Some("Trading Bot"));
    }

    #[test]
    fn extra_agent_minimal_fields() {
        let json = serde_json::json!({
            "address": "0xagent2"
        });
        let parsed: HlExtraAgent = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.address, "0xagent2");
        assert!(parsed.name.is_none());
    }

    #[test]
    fn extra_agent_extra_fields_captured() {
        let json = serde_json::json!({
            "address": "0xagent3",
            "permissions": ["trade", "withdraw"]
        });
        let parsed: HlExtraAgent = serde_json::from_value(json).unwrap();
        assert!(parsed.extra.contains_key("permissions"));
    }

    #[test]
    fn extra_agent_camel_case_keys() {
        let agent = HlExtraAgent {
            address: "0x1".into(),
            name: Some("Bot".into()),
            extra: HashMap::new(),
        };
        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("address"));
        assert!(json.contains("name"));
    }
}
