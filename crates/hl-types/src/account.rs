use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A position held on Hyperliquid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
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

impl HlPosition {
    /// Creates a new `HlPosition`.
    pub fn new(
        coin: String,
        size: Decimal,
        entry_px: Decimal,
        unrealized_pnl: Decimal,
        leverage: Decimal,
        liquidation_px: Option<Decimal>,
    ) -> Self {
        Self {
            coin,
            size,
            entry_px,
            unrealized_pnl,
            leverage,
            liquidation_px,
        }
    }
}

/// A trade fill on Hyperliquid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
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

impl HlFill {
    /// Creates a new `HlFill`.
    pub fn new(
        coin: String,
        px: Decimal,
        sz: Decimal,
        is_buy: bool,
        timestamp: u64,
        fee: Decimal,
        closed_pnl: Decimal,
    ) -> Self {
        Self {
            coin,
            px,
            sz,
            is_buy,
            timestamp,
            fee,
            closed_pnl,
        }
    }
}

/// Snapshot of an account's state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlAccountState {
    /// Account equity.
    pub equity: Decimal,
    /// Available margin.
    pub margin_available: Decimal,
    /// Open positions.
    pub positions: Vec<HlPosition>,
}

impl HlAccountState {
    /// Creates a new `HlAccountState`.
    pub fn new(equity: Decimal, margin_available: Decimal, positions: Vec<HlPosition>) -> Self {
        Self {
            equity,
            margin_available,
            positions,
        }
    }
}

/// Summary of a vault the user participates in.
///
/// Returned by the `vaultSummaries` info endpoint. Fields that the API may
/// add in the future are captured in `extra`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// User fee information including maker/taker rates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlUserFees {
    /// Fee tier level.
    pub fee_tier: String,
    /// Maker fee rate (e.g. "0.0002").
    pub maker_rate: Decimal,
    /// Taker fee rate (e.g. "0.0005").
    pub taker_rate: Decimal,
}

impl HlUserFees {
    /// Creates a new `HlUserFees`.
    pub fn new(fee_tier: String, maker_rate: Decimal, taker_rate: Decimal) -> Self {
        Self {
            fee_tier,
            maker_rate,
            taker_rate,
        }
    }
}

/// API rate limit status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlRateLimitStatus {
    /// Current number of requests used.
    pub used: u64,
    /// Maximum allowed requests in the window.
    pub limit: u64,
    /// Window duration in milliseconds.
    pub window_ms: u64,
}

impl HlRateLimitStatus {
    /// Creates a new `HlRateLimitStatus`.
    pub fn new(used: u64, limit: u64, window_ms: u64) -> Self {
        Self {
            used,
            limit,
            window_ms,
        }
    }
}

/// An extra (sub-)agent approval entry.
///
/// Returned by the `extraAgents` info endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// A staking delegation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlStakingDelegation {
    /// Validator address.
    pub validator: String,
    /// Amount delegated.
    pub amount: Decimal,
    /// Pending rewards.
    pub rewards: Decimal,
}

impl HlStakingDelegation {
    /// Creates a new `HlStakingDelegation`.
    pub fn new(validator: String, amount: Decimal, rewards: Decimal) -> Self {
        Self {
            validator,
            amount,
            rewards,
        }
    }
}

/// Borrow/lend position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlBorrowLendState {
    /// Token/coin name.
    pub coin: String,
    /// Amount supplied/lent.
    pub supply: Decimal,
    /// Amount borrowed.
    pub borrow: Decimal,
    /// Current APY rate.
    pub apy: Decimal,
}

impl HlBorrowLendState {
    /// Creates a new `HlBorrowLendState`.
    pub fn new(coin: String, supply: Decimal, borrow: Decimal, apy: Decimal) -> Self {
        Self {
            coin,
            supply,
            borrow,
            apy,
        }
    }
}

/// An open order on Hyperliquid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlOpenOrder {
    /// Order ID.
    pub oid: u64,
    /// The coin/asset symbol.
    pub coin: String,
    /// Order side.
    pub side: crate::market::TradeSide,
    /// Limit price.
    pub limit_px: Decimal,
    /// Order size.
    pub sz: Decimal,
    /// Timestamp in milliseconds.
    pub timestamp: u64,
    /// Order type (e.g. "Limit").
    pub order_type: String,
    /// Client order ID, if set.
    pub cloid: Option<String>,
}

impl HlOpenOrder {
    /// Creates a new `HlOpenOrder`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        oid: u64,
        coin: String,
        side: crate::market::TradeSide,
        limit_px: Decimal,
        sz: Decimal,
        timestamp: u64,
        order_type: String,
        cloid: Option<String>,
    ) -> Self {
        Self {
            oid,
            coin,
            side,
            limit_px,
            sz,
            timestamp,
            order_type,
            cloid,
        }
    }
}

/// Detailed status of a single order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlOrderDetail {
    /// Order ID.
    pub oid: u64,
    /// The coin/asset symbol.
    pub coin: String,
    /// Order side.
    pub side: crate::market::TradeSide,
    /// Limit price.
    pub limit_px: Decimal,
    /// Order size.
    pub sz: Decimal,
    /// Timestamp in milliseconds.
    pub timestamp: u64,
    /// Order type (e.g. "Limit").
    pub order_type: String,
    /// Client order ID, if set.
    pub cloid: Option<String>,
    /// Order status (e.g. "open", "filled", "canceled").
    pub status: String,
}

impl HlOrderDetail {
    /// Creates a new `HlOrderDetail`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        oid: u64,
        coin: String,
        side: crate::market::TradeSide,
        limit_px: Decimal,
        sz: Decimal,
        timestamp: u64,
        order_type: String,
        cloid: Option<String>,
        status: String,
    ) -> Self {
        Self {
            oid,
            coin,
            side,
            limit_px,
            sz,
            timestamp,
            order_type,
            cloid,
            status,
        }
    }
}

/// A funding rate entry for a coin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlFundingEntry {
    /// The coin/asset symbol.
    pub coin: String,
    /// Funding rate.
    pub funding_rate: Decimal,
    /// Premium.
    pub premium: Decimal,
    /// Timestamp in milliseconds.
    pub time: u64,
}

impl HlFundingEntry {
    /// Creates a new `HlFundingEntry`.
    pub fn new(coin: String, funding_rate: Decimal, premium: Decimal, time: u64) -> Self {
        Self {
            coin,
            funding_rate,
            premium,
            time,
        }
    }
}

/// A user-specific funding entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlUserFundingEntry {
    /// The coin/asset symbol.
    pub coin: String,
    /// USDC amount.
    pub usdc: Decimal,
    /// Size (signed).
    pub szi: Decimal,
    /// Funding rate.
    pub funding_rate: Decimal,
    /// Timestamp in milliseconds.
    pub time: u64,
}

impl HlUserFundingEntry {
    /// Creates a new `HlUserFundingEntry`.
    pub fn new(
        coin: String,
        usdc: Decimal,
        szi: Decimal,
        funding_rate: Decimal,
        time: u64,
    ) -> Self {
        Self {
            coin,
            usdc,
            szi,
            funding_rate,
            time,
        }
    }
}

/// A historical order with status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlHistoricalOrder {
    /// Order ID.
    pub oid: u64,
    /// The coin/asset symbol.
    pub coin: String,
    /// Order side.
    pub side: crate::market::TradeSide,
    /// Limit price.
    pub limit_px: Decimal,
    /// Order size.
    pub sz: Decimal,
    /// Timestamp in milliseconds.
    pub timestamp: u64,
    /// Order type (e.g. "Limit").
    pub order_type: String,
    /// Client order ID, if set.
    pub cloid: Option<String>,
    /// Order status (e.g. "filled", "canceled").
    pub status: String,
}

impl HlHistoricalOrder {
    /// Creates a new `HlHistoricalOrder`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        oid: u64,
        coin: String,
        side: crate::market::TradeSide,
        limit_px: Decimal,
        sz: Decimal,
        timestamp: u64,
        order_type: String,
        cloid: Option<String>,
        status: String,
    ) -> Self {
        Self {
            oid,
            coin,
            side,
            limit_px,
            sz,
            timestamp,
            order_type,
            cloid,
            status,
        }
    }
}

/// Referral state for a Hyperliquid account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlReferralState {
    /// The address of the referrer, if any.
    pub referrer: Option<String>,
    /// The user's own referral code, if any.
    pub referral_code: Option<String>,
    /// Cumulative volume traded.
    pub cum_vlm: Decimal,
    /// Referral rewards earned.
    pub rewards: Decimal,
}

impl HlReferralState {
    /// Creates a new `HlReferralState`.
    pub fn new(
        referrer: Option<String>,
        referral_code: Option<String>,
        cum_vlm: Decimal,
        rewards: Decimal,
    ) -> Self {
        Self {
            referrer,
            referral_code,
            cum_vlm,
            rewards,
        }
    }
}

/// Active asset data for a user's position in a specific coin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlActiveAssetData {
    /// The coin/asset symbol.
    pub coin: String,
    /// Current leverage for this asset.
    pub leverage: Decimal,
    /// Maximum trade sizes (buy/sell).
    pub max_trade_szs: Vec<Decimal>,
    /// Margin currently used for this asset.
    pub margin_used: Decimal,
}

impl HlActiveAssetData {
    /// Creates a new `HlActiveAssetData`.
    pub fn new(
        coin: String,
        leverage: Decimal,
        max_trade_szs: Vec<Decimal>,
        margin_used: Decimal,
    ) -> Self {
        Self {
            coin,
            leverage,
            max_trade_szs,
            margin_used,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::TradeSide;
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
    fn staking_delegation_serde_roundtrip() {
        let delegation = HlStakingDelegation {
            validator: "0xval1".into(),
            amount: Decimal::from_str("1000.0").unwrap(),
            rewards: Decimal::from_str("5.25").unwrap(),
        };
        let json = serde_json::to_string(&delegation).unwrap();
        let parsed: HlStakingDelegation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.validator, "0xval1");
        assert_eq!(parsed.amount, Decimal::from_str("1000.0").unwrap());
        assert_eq!(parsed.rewards, Decimal::from_str("5.25").unwrap());
    }

    #[test]
    fn staking_delegation_from_json() {
        let json = serde_json::json!({
            "validator": "0xabc",
            "amount": "500.0",
            "rewards": "2.5"
        });
        let parsed: HlStakingDelegation = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.validator, "0xabc");
        assert_eq!(parsed.amount, Decimal::from_str("500.0").unwrap());
        assert_eq!(parsed.rewards, Decimal::from_str("2.5").unwrap());
    }

    #[test]
    fn borrow_lend_state_serde_roundtrip() {
        let state = HlBorrowLendState {
            coin: "USDC".into(),
            supply: Decimal::from_str("10000.0").unwrap(),
            borrow: Decimal::from_str("5000.0").unwrap(),
            apy: Decimal::from_str("0.05").unwrap(),
        };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: HlBorrowLendState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coin, "USDC");
        assert_eq!(parsed.supply, Decimal::from_str("10000.0").unwrap());
        assert_eq!(parsed.borrow, Decimal::from_str("5000.0").unwrap());
        assert_eq!(parsed.apy, Decimal::from_str("0.05").unwrap());
    }

    #[test]
    fn borrow_lend_state_from_json() {
        let json = serde_json::json!({
            "coin": "ETH",
            "supply": "100.0",
            "borrow": "0.0",
            "apy": "0.03"
        });
        let parsed: HlBorrowLendState = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.coin, "ETH");
        assert_eq!(parsed.supply, Decimal::from_str("100.0").unwrap());
        assert_eq!(parsed.borrow, Decimal::ZERO);
        assert_eq!(parsed.apy, Decimal::from_str("0.03").unwrap());
    }

    #[test]
    fn borrow_lend_state_camel_case_keys() {
        let state = HlBorrowLendState {
            coin: "X".into(),
            supply: Decimal::ONE,
            borrow: Decimal::ZERO,
            apy: Decimal::ZERO,
        };
        let json = serde_json::to_string(&state).unwrap();
        // All fields are single-word or camelCase; just verify it serializes
        assert!(json.contains("supply"));
        assert!(json.contains("borrow"));
        assert!(json.contains("apy"));
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

    #[test]
    fn user_fees_serde_roundtrip() {
        let fees = HlUserFees {
            fee_tier: "VIP2".into(),
            maker_rate: Decimal::from_str("0.0001").unwrap(),
            taker_rate: Decimal::from_str("0.0003").unwrap(),
        };
        let json = serde_json::to_string(&fees).unwrap();
        let parsed: HlUserFees = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.fee_tier, "VIP2");
        assert_eq!(parsed.maker_rate, Decimal::from_str("0.0001").unwrap());
        assert_eq!(parsed.taker_rate, Decimal::from_str("0.0003").unwrap());
    }

    #[test]
    fn user_fees_camel_case_keys() {
        let fees = HlUserFees {
            fee_tier: "T1".into(),
            maker_rate: Decimal::ZERO,
            taker_rate: Decimal::ONE,
        };
        let json = serde_json::to_string(&fees).unwrap();
        assert!(json.contains("feeTier"));
        assert!(json.contains("makerRate"));
        assert!(json.contains("takerRate"));
    }

    #[test]
    fn user_fees_constructor() {
        let fees = HlUserFees::new(
            "VIP1".into(),
            Decimal::from_str("0.0002").unwrap(),
            Decimal::from_str("0.0005").unwrap(),
        );
        assert_eq!(fees.fee_tier, "VIP1");
        assert_eq!(fees.maker_rate, Decimal::from_str("0.0002").unwrap());
        assert_eq!(fees.taker_rate, Decimal::from_str("0.0005").unwrap());
    }

    #[test]
    fn rate_limit_status_serde_roundtrip() {
        let status = HlRateLimitStatus {
            used: 42,
            limit: 1200,
            window_ms: 60000,
        };
        let json = serde_json::to_string(&status).unwrap();
        let parsed: HlRateLimitStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.used, 42);
        assert_eq!(parsed.limit, 1200);
        assert_eq!(parsed.window_ms, 60000);
    }

    #[test]
    fn rate_limit_status_camel_case_keys() {
        let status = HlRateLimitStatus {
            used: 0,
            limit: 100,
            window_ms: 30000,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("windowMs"));
    }

    #[test]
    fn rate_limit_status_constructor() {
        let status = HlRateLimitStatus::new(10, 500, 60000);
        assert_eq!(status.used, 10);
        assert_eq!(status.limit, 500);
        assert_eq!(status.window_ms, 60000);
    }

    #[test]
    fn open_order_serde_roundtrip() {
        let order = HlOpenOrder {
            oid: 12345,
            coin: "BTC".into(),
            side: TradeSide::Buy,
            limit_px: Decimal::from_str("60000.0").unwrap(),
            sz: Decimal::from_str("0.5").unwrap(),
            timestamp: 1700000000000,
            order_type: "Limit".into(),
            cloid: Some("my-order-1".into()),
        };
        let json = serde_json::to_string(&order).unwrap();
        let parsed: HlOpenOrder = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.oid, 12345);
        assert_eq!(parsed.coin, "BTC");
        assert_eq!(parsed.side, TradeSide::Buy);
        assert_eq!(parsed.limit_px, Decimal::from_str("60000.0").unwrap());
        assert_eq!(parsed.sz, Decimal::from_str("0.5").unwrap());
        assert_eq!(parsed.timestamp, 1700000000000);
        assert_eq!(parsed.order_type, "Limit");
        assert_eq!(parsed.cloid.as_deref(), Some("my-order-1"));
    }

    #[test]
    fn open_order_no_cloid_roundtrip() {
        let order = HlOpenOrder {
            oid: 99,
            coin: "ETH".into(),
            side: TradeSide::Sell,
            limit_px: Decimal::from_str("3000.0").unwrap(),
            sz: Decimal::ONE,
            timestamp: 0,
            order_type: "Limit".into(),
            cloid: None,
        };
        let json = serde_json::to_string(&order).unwrap();
        let parsed: HlOpenOrder = serde_json::from_str(&json).unwrap();
        assert!(parsed.cloid.is_none());
    }

    #[test]
    fn open_order_camel_case_keys() {
        let order = HlOpenOrder {
            oid: 1,
            coin: "X".into(),
            side: TradeSide::Buy,
            limit_px: Decimal::ONE,
            sz: Decimal::ONE,
            timestamp: 0,
            order_type: "Limit".into(),
            cloid: None,
        };
        let json = serde_json::to_string(&order).unwrap();
        assert!(json.contains("limitPx"));
        assert!(json.contains("orderType"));
    }

    #[test]
    fn order_detail_serde_roundtrip() {
        let detail = HlOrderDetail {
            oid: 555,
            coin: "SOL".into(),
            side: TradeSide::Buy,
            limit_px: Decimal::from_str("150.0").unwrap(),
            sz: Decimal::from_str("10.0").unwrap(),
            timestamp: 1700000000000,
            order_type: "Limit".into(),
            cloid: None,
            status: "filled".into(),
        };
        let json = serde_json::to_string(&detail).unwrap();
        let parsed: HlOrderDetail = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.oid, 555);
        assert_eq!(parsed.status, "filled");
        assert_eq!(parsed.coin, "SOL");
    }

    #[test]
    fn order_detail_camel_case_keys() {
        let detail = HlOrderDetail {
            oid: 1,
            coin: "X".into(),
            side: TradeSide::Buy,
            limit_px: Decimal::ONE,
            sz: Decimal::ONE,
            timestamp: 0,
            order_type: "Limit".into(),
            cloid: Some("c1".into()),
            status: "open".into(),
        };
        let json = serde_json::to_string(&detail).unwrap();
        assert!(json.contains("limitPx"));
        assert!(json.contains("orderType"));
    }

    #[test]
    fn funding_entry_serde_roundtrip() {
        let entry = HlFundingEntry {
            coin: "BTC".into(),
            funding_rate: Decimal::from_str("0.0001").unwrap(),
            premium: Decimal::from_str("0.00005").unwrap(),
            time: 1700000000000,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: HlFundingEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coin, "BTC");
        assert_eq!(parsed.funding_rate, Decimal::from_str("0.0001").unwrap());
        assert_eq!(parsed.premium, Decimal::from_str("0.00005").unwrap());
        assert_eq!(parsed.time, 1700000000000);
    }

    #[test]
    fn funding_entry_camel_case_keys() {
        let entry = HlFundingEntry {
            coin: "X".into(),
            funding_rate: Decimal::ONE,
            premium: Decimal::ZERO,
            time: 0,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("fundingRate"));
    }

    #[test]
    fn user_funding_entry_serde_roundtrip() {
        let entry = HlUserFundingEntry {
            coin: "ETH".into(),
            usdc: Decimal::from_str("-1.5").unwrap(),
            szi: Decimal::from_str("2.0").unwrap(),
            funding_rate: Decimal::from_str("0.0002").unwrap(),
            time: 1700000000000,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: HlUserFundingEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coin, "ETH");
        assert_eq!(parsed.usdc, Decimal::from_str("-1.5").unwrap());
        assert_eq!(parsed.szi, Decimal::from_str("2.0").unwrap());
        assert_eq!(parsed.funding_rate, Decimal::from_str("0.0002").unwrap());
        assert_eq!(parsed.time, 1700000000000);
    }

    #[test]
    fn user_funding_entry_camel_case_keys() {
        let entry = HlUserFundingEntry {
            coin: "X".into(),
            usdc: Decimal::ONE,
            szi: Decimal::ONE,
            funding_rate: Decimal::ZERO,
            time: 0,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("fundingRate"));
    }

    #[test]
    fn historical_order_serde_roundtrip() {
        let order = HlHistoricalOrder {
            oid: 777,
            coin: "BTC".into(),
            side: TradeSide::Sell,
            limit_px: Decimal::from_str("65000.0").unwrap(),
            sz: Decimal::from_str("0.1").unwrap(),
            timestamp: 1700000000000,
            order_type: "Limit".into(),
            cloid: Some("hist-1".into()),
            status: "filled".into(),
        };
        let json = serde_json::to_string(&order).unwrap();
        let parsed: HlHistoricalOrder = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.oid, 777);
        assert_eq!(parsed.status, "filled");
        assert_eq!(parsed.coin, "BTC");
        assert_eq!(parsed.cloid.as_deref(), Some("hist-1"));
    }

    #[test]
    fn historical_order_camel_case_keys() {
        let order = HlHistoricalOrder {
            oid: 1,
            coin: "X".into(),
            side: TradeSide::Buy,
            limit_px: Decimal::ONE,
            sz: Decimal::ONE,
            timestamp: 0,
            order_type: "Limit".into(),
            cloid: None,
            status: "canceled".into(),
        };
        let json = serde_json::to_string(&order).unwrap();
        assert!(json.contains("limitPx"));
        assert!(json.contains("orderType"));
    }

    #[test]
    fn referral_state_serde_roundtrip() {
        let state = HlReferralState::new(
            Some("0xabc".into()),
            Some("CODE123".into()),
            Decimal::from_str("50000.0").unwrap(),
            Decimal::from_str("100.5").unwrap(),
        );
        let json = serde_json::to_string(&state).unwrap();
        let parsed: HlReferralState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.referrer.as_deref(), Some("0xabc"));
        assert_eq!(parsed.referral_code.as_deref(), Some("CODE123"));
        assert_eq!(parsed.cum_vlm, Decimal::from_str("50000.0").unwrap());
        assert_eq!(parsed.rewards, Decimal::from_str("100.5").unwrap());
    }

    #[test]
    fn referral_state_camel_case_keys() {
        let state = HlReferralState::new(None, None, Decimal::ZERO, Decimal::ZERO);
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("cumVlm"));
        assert!(json.contains("referralCode"));
    }
}
