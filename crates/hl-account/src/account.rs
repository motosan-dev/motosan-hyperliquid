use std::str::FromStr;
use std::sync::Arc;

use rust_decimal::Decimal;

use hl_client::{HttpTransport, HyperliquidClient};
use hl_types::{
    HlAccountState, HlBorrowLendState, HlError, HlExtraAgent, HlFill, HlPosition,
    HlRateLimitStatus, HlSpotBalance, HlStakingDelegation, HlUserFees, HlVaultDetails,
    HlVaultSummary,
};

/// Typed interface for Hyperliquid account state queries.
///
/// Wraps an [`HttpTransport`] and provides methods to fetch positions,
/// fills, vault information, and agent approvals for any public address.
pub struct Account {
    client: Arc<dyn HttpTransport>,
}

impl Account {
    /// Create a new `Account` instance wrapping an [`HttpTransport`].
    pub fn new(client: Arc<dyn HttpTransport>) -> Self {
        Self { client }
    }

    /// Convenience constructor that wraps a [`HyperliquidClient`] in an `Arc`.
    pub fn from_client(client: HyperliquidClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// Fetch the full clearinghouse state for an address.
    #[tracing::instrument(skip(self))]
    pub async fn state(&self, address: &str) -> Result<HlAccountState, HlError> {
        let payload = serde_json::json!({
            "type": "clearinghouseState",
            "user": address,
        });
        let resp = self.client.post_info(payload).await?;
        parse_account_state(&resp)
    }

    /// Fetch spot token balances for an address.
    #[tracing::instrument(skip(self))]
    pub async fn spot_state(&self, address: &str) -> Result<Vec<HlSpotBalance>, HlError> {
        let payload = serde_json::json!({
            "type": "spotClearinghouseState",
            "user": address,
        });
        let resp = self.client.post_info(payload).await?;
        parse_spot_state(&resp)
    }

    /// Fetch only the open positions for an address.
    #[tracing::instrument(skip(self))]
    pub async fn positions(&self, address: &str) -> Result<Vec<HlPosition>, HlError> {
        let state = self.state(address).await?;
        Ok(state.positions)
    }

    /// Fetch all fills (trade history) for an address.
    #[tracing::instrument(skip(self))]
    pub async fn fills(&self, address: &str) -> Result<Vec<HlFill>, HlError> {
        let payload = serde_json::json!({ "type": "userFills", "user": address });
        let resp = self.client.post_info(payload).await?;
        parse_fills(&resp)
    }

    /// Fetch vault summaries for an address.
    #[tracing::instrument(skip(self))]
    pub async fn vault_summaries(&self, address: &str) -> Result<Vec<HlVaultSummary>, HlError> {
        let payload = serde_json::json!({ "type": "vaultSummaries", "user": address });
        let resp = self.client.post_info(payload).await?;
        let arr = resp
            .as_array()
            .ok_or_else(|| HlError::Parse("expected array for vaultSummaries".into()))?;
        arr.iter()
            .map(|v| {
                serde_json::from_value(v.clone())
                    .map_err(|e| HlError::Parse(format!("vaultSummary: {e}")))
            })
            .collect()
    }

    /// Fetch details for a specific vault.
    #[tracing::instrument(skip(self))]
    pub async fn vault_details(
        &self,
        address: &str,
        vault: &str,
    ) -> Result<HlVaultDetails, HlError> {
        let payload = serde_json::json!({
            "type": "vaultDetails",
            "user": address,
            "vaultAddress": vault,
        });
        let resp = self.client.post_info(payload).await?;
        serde_json::from_value(resp).map_err(|e| HlError::Parse(format!("vaultDetails: {e}")))
    }

    /// Fetch extra (sub-)agent approvals for an address.
    #[tracing::instrument(skip(self))]
    pub async fn extra_agents(&self, address: &str) -> Result<Vec<HlExtraAgent>, HlError> {
        let payload = serde_json::json!({ "type": "extraAgents", "user": address });
        let resp = self.client.post_info(payload).await?;
        let arr = resp
            .as_array()
            .ok_or_else(|| HlError::Parse("expected array for extraAgents".into()))?;
        arr.iter()
            .map(|v| {
                serde_json::from_value(v.clone())
                    .map_err(|e| HlError::Parse(format!("extraAgent: {e}")))
            })
            .collect()
    }

    /// Fetch clearinghouse state across all DEXes for an address (HIP-3).
    ///
    /// Returns the raw JSON response since the multi-DEX structure is complex
    /// and varies. Callers can parse the fields they need.
    #[tracing::instrument(skip(self))]
    pub async fn all_dexs_state(&self, address: &str) -> Result<serde_json::Value, HlError> {
        let payload = serde_json::json!({
            "type": "allDexsClearinghouseState",
            "user": address,
        });
        self.client.post_info(payload).await
    }

    /// Fetch open orders for an address.
    #[tracing::instrument(skip(self))]
    pub async fn open_orders(&self, address: &str) -> Result<Vec<serde_json::Value>, HlError> {
        let payload = serde_json::json!({"type": "openOrders", "user": address});
        let resp = self.client.post_info(payload).await?;
        resp.as_array()
            .cloned()
            .ok_or_else(|| HlError::Parse("expected array for openOrders".into()))
    }

    /// Fetch the status of a specific order.
    #[tracing::instrument(skip(self))]
    pub async fn order_status(
        &self,
        address: &str,
        oid: u64,
    ) -> Result<serde_json::Value, HlError> {
        let payload = serde_json::json!({"type": "orderStatus", "user": address, "oid": oid});
        self.client.post_info(payload).await
    }

    /// Fetch funding history for a coin.
    #[tracing::instrument(skip(self))]
    pub async fn funding_history(
        &self,
        coin: &str,
        start_time: u64,
        end_time: Option<u64>,
    ) -> Result<Vec<serde_json::Value>, HlError> {
        let mut payload = serde_json::json!({
            "type": "fundingHistory",
            "coin": coin,
            "startTime": start_time,
        });
        if let Some(et) = end_time {
            payload
                .as_object_mut()
                .unwrap()
                .insert("endTime".to_string(), serde_json::Value::Number(et.into()));
        }
        let resp = self.client.post_info(payload).await?;
        resp.as_array()
            .cloned()
            .ok_or_else(|| HlError::Parse("expected array for fundingHistory".into()))
    }

    /// Fetch user funding history for an address.
    #[tracing::instrument(skip(self))]
    pub async fn user_funding(
        &self,
        address: &str,
        start_time: u64,
        end_time: Option<u64>,
    ) -> Result<Vec<serde_json::Value>, HlError> {
        let mut payload = serde_json::json!({
            "type": "userFunding",
            "user": address,
            "startTime": start_time,
        });
        if let Some(et) = end_time {
            payload
                .as_object_mut()
                .unwrap()
                .insert("endTime".to_string(), serde_json::Value::Number(et.into()));
        }
        let resp = self.client.post_info(payload).await?;
        resp.as_array()
            .cloned()
            .ok_or_else(|| HlError::Parse("expected array for userFunding".into()))
    }

    /// Fetch historical orders for an address.
    #[tracing::instrument(skip(self))]
    pub async fn historical_orders(
        &self,
        address: &str,
    ) -> Result<Vec<serde_json::Value>, HlError> {
        let payload = serde_json::json!({"type": "historicalOrders", "user": address});
        let resp = self.client.post_info(payload).await?;
        resp.as_array()
            .cloned()
            .ok_or_else(|| HlError::Parse("expected array for historicalOrders".into()))
    }

    /// Fetch staking delegations for an address.
    #[tracing::instrument(skip(self))]
    pub async fn staking_delegations(
        &self,
        address: &str,
    ) -> Result<Vec<HlStakingDelegation>, HlError> {
        let payload = serde_json::json!({ "type": "stakingDelegations", "user": address });
        let resp = self.client.post_info(payload).await?;
        parse_staking_delegations(&resp)
    }

    /// Fetch borrow/lend state for an address.
    #[tracing::instrument(skip(self))]
    pub async fn borrow_lend_state(
        &self,
        address: &str,
    ) -> Result<Vec<HlBorrowLendState>, HlError> {
        let payload = serde_json::json!({ "type": "spotClearinghouseState", "user": address });
        let resp = self.client.post_info(payload).await?;
        parse_borrow_lend_state(&resp)
    }

    /// Fetch fee tier and maker/taker rates for an address.
    #[tracing::instrument(skip(self))]
    pub async fn user_fees(&self, address: &str) -> Result<HlUserFees, HlError> {
        let payload = serde_json::json!({ "type": "userFees", "user": address });
        let resp = self.client.post_info(payload).await?;
        parse_user_fees(&resp)
    }

    /// Fetch current API rate limit status for an address.
    #[tracing::instrument(skip(self))]
    pub async fn rate_limit_status(&self, address: &str) -> Result<HlRateLimitStatus, HlError> {
        let payload = serde_json::json!({ "type": "userRateLimit", "user": address });
        let resp = self.client.post_info(payload).await?;
        parse_rate_limit_status(&resp)
    }
}

/// Parse a string-encoded decimal from a JSON value, returning an error on failure.
fn parse_str_decimal(val: &serde_json::Value, field: &str) -> Result<Decimal, HlError> {
    match val {
        serde_json::Value::String(s) => Decimal::from_str(s).map_err(|_| {
            HlError::Parse(format!("cannot parse '{field}' value \"{s}\" as Decimal"))
        }),
        serde_json::Value::Number(n) => {
            let s = n.to_string();
            Decimal::from_str(&s)
                .map_err(|_| HlError::Parse(format!("cannot convert '{field}' number to Decimal")))
        }
        serde_json::Value::Null => Err(HlError::Parse(format!("field '{field}' is null"))),
        v => Err(HlError::Parse(format!(
            "unexpected type for '{field}': expected string or number, got {v}"
        ))),
    }
}

/// A small threshold used to detect zero-size (closed) positions.
const ZERO_SIZE_THRESHOLD: Decimal = Decimal::from_parts(1, 0, 0, false, 12); // 1e-12

/// Parse a `clearinghouseState` JSON response into an [`HlAccountState`].
///
/// Hyperliquid returns numeric fields as quoted strings, e.g. `"szi": "0.001"`.
/// Zero-size positions (|szi| < 1e-12) are skipped.
pub fn parse_account_state(resp: &serde_json::Value) -> Result<HlAccountState, HlError> {
    let margin_summary = resp
        .get("marginSummary")
        .ok_or_else(|| HlError::Parse("missing 'marginSummary' in clearinghouseState".into()))?;

    let equity: Decimal = parse_str_decimal(
        margin_summary
            .get("accountValue")
            .ok_or_else(|| HlError::Parse("missing 'accountValue' in marginSummary".into()))?,
        "accountValue",
    )?;

    let margin_available: Decimal = {
        let raw = margin_summary
            .get("totalRawUsd")
            .or_else(|| margin_summary.get("availableMargin"))
            .ok_or_else(|| {
                HlError::Parse(
                    "missing 'totalRawUsd' and 'availableMargin' in marginSummary".into(),
                )
            })?;
        parse_str_decimal(raw, "totalRawUsd/availableMargin")?
    };

    let mut positions = Vec::new();

    if let Some(asset_positions) = resp["assetPositions"].as_array() {
        for pos in asset_positions {
            let p = &pos["position"];

            // Size: parse with error propagation. A size of 0.0 is valid
            // (means the position is closed), so we skip it rather than error.
            let size: Decimal = parse_str_decimal(
                p.get("szi")
                    .ok_or_else(|| HlError::Parse("missing 'szi' in position".into()))?,
                "szi",
            )?;
            if size.abs() < ZERO_SIZE_THRESHOLD {
                continue;
            }

            let coin = match p.get("coin").and_then(|v| v.as_str()) {
                Some(c) if !c.is_empty() => c.to_string(),
                _ => {
                    tracing::warn!("Skipping position with missing or empty coin field");
                    continue;
                }
            };

            let entry_px: Decimal = parse_str_decimal(
                p.get("entryPx")
                    .ok_or_else(|| HlError::Parse("missing 'entryPx' in position".into()))?,
                "entryPx",
            )?;
            let unrealized_pnl: Decimal = parse_str_decimal(
                p.get("unrealizedPnl")
                    .ok_or_else(|| HlError::Parse("missing 'unrealizedPnl' in position".into()))?,
                "unrealizedPnl",
            )?;
            let leverage: Decimal = match p.get("leverage").and_then(|l| l.get("value")) {
                Some(v) => parse_str_decimal(v, "leverage.value")
                    // Leverage defaults to 1.0 if unparseable (cross-margin mode).
                    .unwrap_or(Decimal::ONE),
                None => Decimal::ONE,
            };
            let liquidation_px: Option<Decimal> = match p.get("liquidationPx") {
                Some(serde_json::Value::Null) | None => None,
                Some(v) => Some(parse_str_decimal(v, "liquidationPx")?),
            };

            positions.push(HlPosition::new(
                coin,
                size,
                entry_px,
                unrealized_pnl,
                leverage,
                liquidation_px,
            ));
        }
    }

    Ok(HlAccountState::new(equity, margin_available, positions))
}

/// Parse a `userFills` JSON response into a [`Vec<HlFill>`].
///
/// Hyperliquid returns numeric fields as quoted strings.
/// The `side` field is `"B"` (buy) or `"A"` (ask/sell).
pub fn parse_fills(resp: &serde_json::Value) -> Result<Vec<HlFill>, HlError> {
    let arr = resp
        .as_array()
        .ok_or_else(|| HlError::Parse("expected array for userFills".into()))?;

    let mut fills = Vec::with_capacity(arr.len());

    for fill in arr {
        let coin = match fill["coin"].as_str() {
            Some(c) if !c.is_empty() => c.to_string(),
            _ => {
                tracing::warn!("Skipping fill with missing or empty coin field");
                continue;
            }
        };

        let px: Decimal = parse_str_decimal(
            fill.get("px")
                .ok_or_else(|| HlError::Parse("missing 'px' in fill".into()))?,
            "px",
        )?;
        let sz: Decimal = parse_str_decimal(
            fill.get("sz")
                .ok_or_else(|| HlError::Parse("missing 'sz' in fill".into()))?,
            "sz",
        )?;
        let is_buy = fill
            .get("side")
            .and_then(|v| v.as_str())
            .ok_or_else(|| HlError::Parse("missing 'side' in fill".into()))?
            == "B";
        let timestamp: u64 = fill
            .get("time")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| HlError::Parse("missing or invalid 'time' in fill".into()))?;
        let fee: Decimal = parse_str_decimal(
            fill.get("fee")
                .ok_or_else(|| HlError::Parse("missing 'fee' in fill".into()))?,
            "fee",
        )?;
        // closedPnl may be 0.0 legitimately (no realized PnL), default to 0.0 if missing.
        let closed_pnl: Decimal = match fill.get("closedPnl") {
            Some(v) => parse_str_decimal(v, "closedPnl")?,
            None => Decimal::ZERO,
        };

        fills.push(HlFill::new(
            coin, px, sz, is_buy, timestamp, fee, closed_pnl,
        ));
    }

    Ok(fills)
}

/// Parse a `spotClearinghouseState` JSON response into a [`Vec<HlSpotBalance>`].
///
/// The response contains a `balances` array:
///   `{ "balances": [{ "coin": "PURR", "token": 1, "hold": "0", "total": "1000.0" }, ...] }`
pub fn parse_spot_state(resp: &serde_json::Value) -> Result<Vec<HlSpotBalance>, HlError> {
    let balances_arr = resp
        .get("balances")
        .and_then(|v| v.as_array())
        .ok_or_else(|| HlError::Parse("missing 'balances' in spotClearinghouseState".into()))?;

    let mut balances = Vec::with_capacity(balances_arr.len());
    for item in balances_arr {
        let coin = match item.get("coin").and_then(|v| v.as_str()) {
            Some(c) if !c.is_empty() => c.to_string(),
            _ => {
                tracing::warn!("Skipping spot balance with missing or empty coin field");
                continue;
            }
        };

        let token = item
            .get("token")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| HlError::Parse("missing 'token' in spot balance".into()))?
            as u32;

        let hold = parse_str_decimal(
            item.get("hold")
                .ok_or_else(|| HlError::Parse("missing 'hold' in spot balance".into()))?,
            "hold",
        )?;

        let total = parse_str_decimal(
            item.get("total")
                .ok_or_else(|| HlError::Parse("missing 'total' in spot balance".into()))?,
            "total",
        )?;

        balances.push(HlSpotBalance::new(coin, token, hold, total));
    }

    Ok(balances)
}

/// Parse a `stakingDelegations` JSON response into a [`Vec<HlStakingDelegation>`].
///
/// Hyperliquid returns: `[{"validator": "0x...", "amount": "1000.0", "rewards": "5.0"}, ...]`
pub fn parse_staking_delegations(
    resp: &serde_json::Value,
) -> Result<Vec<HlStakingDelegation>, HlError> {
    let arr = resp
        .as_array()
        .ok_or_else(|| HlError::Parse("expected array for stakingDelegations".into()))?;

    let mut delegations = Vec::with_capacity(arr.len());
    for item in arr {
        let validator = match item.get("validator").and_then(|v| v.as_str()) {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => {
                tracing::warn!("Skipping delegation with missing or empty validator");
                continue;
            }
        };
        let amount = parse_str_decimal(
            item.get("amount")
                .ok_or_else(|| HlError::Parse("missing 'amount' in delegation".into()))?,
            "amount",
        )?;
        let rewards = parse_str_decimal(
            item.get("rewards")
                .ok_or_else(|| HlError::Parse("missing 'rewards' in delegation".into()))?,
            "rewards",
        )?;
        delegations.push(HlStakingDelegation::new(validator, amount, rewards));
    }

    Ok(delegations)
}

/// Parse borrow/lend state from a `spotClearinghouseState` JSON response.
///
/// Extracts entries from the `balances` array that have `supply`, `borrow`, and
/// `apy` fields. Entries without these fields are skipped (they are plain spot
/// balances, not borrow/lend positions).
pub fn parse_borrow_lend_state(
    resp: &serde_json::Value,
) -> Result<Vec<HlBorrowLendState>, HlError> {
    let balances_arr = resp
        .get("balances")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            HlError::Parse("missing 'balances' in spotClearinghouseState for borrow/lend".into())
        })?;

    let mut states = Vec::new();
    for item in balances_arr {
        // Only process entries that have borrow/lend fields.
        let (supply_val, borrow_val, apy_val) =
            match (item.get("supply"), item.get("borrow"), item.get("apy")) {
                (Some(s), Some(b), Some(a)) => (s, b, a),
                _ => continue, // Not a borrow/lend entry, skip.
            };

        let coin = match item.get("coin").and_then(|v| v.as_str()) {
            Some(c) if !c.is_empty() => c.to_string(),
            _ => {
                tracing::warn!("Skipping borrow/lend entry with missing or empty coin");
                continue;
            }
        };

        let supply = parse_str_decimal(supply_val, "supply")?;
        let borrow = parse_str_decimal(borrow_val, "borrow")?;
        let apy = parse_str_decimal(apy_val, "apy")?;

        states.push(HlBorrowLendState::new(coin, supply, borrow, apy));
    }

    Ok(states)
}

/// Parse a `userFees` JSON response into an [`HlUserFees`].
///
/// The API returns something like:
///   `{"userCrossRate": "0.0002", "userAddRate": "0.0005", ...}`
/// We map `userCrossRate` → `maker_rate` and `userAddRate` → `taker_rate`.
pub fn parse_user_fees(resp: &serde_json::Value) -> Result<HlUserFees, HlError> {
    let fee_tier = resp
        .get("feeTier")
        .or_else(|| resp.get("userFeeTier"))
        .and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .unwrap_or_default();

    let maker_rate = parse_str_decimal(
        resp.get("userCrossRate")
            .ok_or_else(|| HlError::Parse("missing 'userCrossRate' in userFees".into()))?,
        "userCrossRate",
    )?;

    let taker_rate = parse_str_decimal(
        resp.get("userAddRate")
            .ok_or_else(|| HlError::Parse("missing 'userAddRate' in userFees".into()))?,
        "userAddRate",
    )?;

    Ok(HlUserFees::new(fee_tier, maker_rate, taker_rate))
}

/// Parse a `userRateLimit` JSON response into an [`HlRateLimitStatus`].
///
/// The API returns something like:
///   `{"cumVlm": "...", "nRequestsUsed": 42, "nRequestsCap": 1200, ...}`
pub fn parse_rate_limit_status(resp: &serde_json::Value) -> Result<HlRateLimitStatus, HlError> {
    let used = resp
        .get("nRequestsUsed")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            HlError::Parse("missing or invalid 'nRequestsUsed' in userRateLimit".into())
        })?;

    let limit = resp
        .get("nRequestsCap")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            HlError::Parse("missing or invalid 'nRequestsCap' in userRateLimit".into())
        })?;

    let window_ms = resp
        .get("windowMs")
        .and_then(|v| v.as_u64())
        .unwrap_or(60_000); // default 60s window if not provided

    Ok(HlRateLimitStatus::new(used, limit, window_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_clearinghouse_resp() -> serde_json::Value {
        serde_json::json!({
            "marginSummary": {
                "accountValue": "50000.00",
                "totalMarginUsed": "10000.00",
                "totalRawUsd": "40000.00"
            },
            "assetPositions": [
                {
                    "position": {
                        "coin": "BTC",
                        "szi": "0.5",
                        "entryPx": "60000.0",
                        "unrealizedPnl": "-200.0",
                        "leverage": { "type": "cross", "value": 5 },
                        "liquidationPx": "55000.0"
                    }
                },
                {
                    "position": {
                        "coin": "ETH",
                        "szi": "-2.0",
                        "entryPx": "3000.0",
                        "unrealizedPnl": "100.0",
                        "leverage": { "type": "cross", "value": 3 },
                        "liquidationPx": null
                    }
                },
                {
                    "position": {
                        "coin": "DOGE",
                        "szi": "0.0",
                        "entryPx": "0.1",
                        "unrealizedPnl": "0.0",
                        "leverage": { "type": "cross", "value": 1 }
                    }
                }
            ]
        })
    }

    #[test]
    fn parse_account_state_equity() {
        let resp = make_clearinghouse_resp();
        let state = parse_account_state(&resp).unwrap();
        assert_eq!(state.equity, Decimal::from_str("50000.00").unwrap());
    }

    #[test]
    fn parse_account_state_skips_zero_size() {
        let resp = make_clearinghouse_resp();
        let state = parse_account_state(&resp).unwrap();
        // DOGE has szi=0.0 and should be skipped
        assert_eq!(state.positions.len(), 2);
        assert!(!state.positions.iter().any(|p| p.coin == "DOGE"));
    }

    #[test]
    fn parse_account_state_btc_position() {
        let resp = make_clearinghouse_resp();
        let state = parse_account_state(&resp).unwrap();
        let btc = state.positions.iter().find(|p| p.coin == "BTC").unwrap();
        assert_eq!(btc.size, Decimal::from_str("0.5").unwrap());
        assert_eq!(btc.entry_px, Decimal::from_str("60000.0").unwrap());
        assert_eq!(btc.unrealized_pnl, Decimal::from_str("-200.0").unwrap());
        assert_eq!(btc.leverage, Decimal::from_str("5").unwrap());
        assert_eq!(
            btc.liquidation_px,
            Some(Decimal::from_str("55000.0").unwrap())
        );
    }

    #[test]
    fn parse_account_state_eth_position_no_liquidation() {
        let resp = make_clearinghouse_resp();
        let state = parse_account_state(&resp).unwrap();
        let eth = state.positions.iter().find(|p| p.coin == "ETH").unwrap();
        assert_eq!(eth.size, Decimal::from_str("-2.0").unwrap());
        assert!(eth.liquidation_px.is_none());
    }

    #[test]
    fn parse_fills_basic() {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let resp = serde_json::json!([
            {
                "coin": "BTC",
                "px": "60100.5",
                "sz": "0.1",
                "side": "B",
                "time": now_ms,
                "fee": "1.50",
                "closedPnl": "0.0"
            },
            {
                "coin": "ETH",
                "px": "3010.0",
                "sz": "1.0",
                "side": "A",
                "time": now_ms,
                "fee": "0.75",
                "closedPnl": "-50.0"
            }
        ]);

        let fills = parse_fills(&resp).unwrap();
        assert_eq!(fills.len(), 2);

        let btc = &fills[0];
        assert_eq!(btc.coin, "BTC");
        assert_eq!(btc.px, Decimal::from_str("60100.5").unwrap());
        assert_eq!(btc.sz, Decimal::from_str("0.1").unwrap());
        assert!(btc.is_buy);
        assert_eq!(btc.timestamp, now_ms);
        assert_eq!(btc.fee, Decimal::from_str("1.50").unwrap());
        assert_eq!(btc.closed_pnl, Decimal::ZERO);

        let eth = &fills[1];
        assert_eq!(eth.coin, "ETH");
        assert!(!eth.is_buy);
        assert_eq!(eth.closed_pnl, Decimal::from_str("-50.0").unwrap());
    }

    #[test]
    fn parse_fills_expects_array() {
        let resp = serde_json::json!({"not": "an array"});
        assert!(parse_fills(&resp).is_err());
    }

    #[test]
    fn parse_fills_skips_missing_coin() {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let resp = serde_json::json!([
            { "coin": "", "px": "100.0", "sz": "1.0", "side": "B", "time": now_ms, "fee": "0", "closedPnl": "0" },
            { "coin": "SOL", "px": "150.0", "sz": "2.0", "side": "A", "time": now_ms, "fee": "0", "closedPnl": "10.0" }
        ]);
        let fills = parse_fills(&resp).unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].coin, "SOL");
    }

    #[test]
    fn parse_account_state_missing_margin_summary_errors() {
        let resp = serde_json::json!({"assetPositions": []});
        let err = parse_account_state(&resp).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("marginSummary"),
            "should mention missing field: {msg}"
        );
    }

    #[test]
    fn parse_account_state_unparseable_equity_errors() {
        let resp = serde_json::json!({
            "marginSummary": {
                "accountValue": "not_a_number",
                "totalRawUsd": "100.0"
            },
            "assetPositions": []
        });
        assert!(parse_account_state(&resp).is_err());
    }

    #[test]
    fn parse_account_state_unparseable_entry_px_errors() {
        let resp = serde_json::json!({
            "marginSummary": {
                "accountValue": "1000.0",
                "totalRawUsd": "500.0"
            },
            "assetPositions": [{
                "position": {
                    "coin": "BTC",
                    "szi": "1.0",
                    "entryPx": "garbage",
                    "unrealizedPnl": "0.0",
                    "leverage": {"type": "cross", "value": 1}
                }
            }]
        });
        assert!(parse_account_state(&resp).is_err());
    }

    #[test]
    fn parse_fills_unparseable_price_errors() {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let resp = serde_json::json!([{
            "coin": "BTC",
            "px": "not_valid",
            "sz": "1.0",
            "side": "B",
            "time": now_ms,
            "fee": "0",
            "closedPnl": "0"
        }]);
        assert!(parse_fills(&resp).is_err());
    }

    #[test]
    fn parse_fills_missing_time_errors() {
        let resp = serde_json::json!([{
            "coin": "BTC",
            "px": "100.0",
            "sz": "1.0",
            "side": "B",
            "fee": "0",
            "closedPnl": "0"
            // "time" missing
        }]);
        assert!(parse_fills(&resp).is_err());
    }

    #[test]
    fn parse_spot_state_valid() {
        let resp = serde_json::json!({
            "balances": [
                { "coin": "PURR", "token": 1, "hold": "0", "total": "1000.0" },
                { "coin": "USDC", "token": 2, "hold": "50.0", "total": "500.0" }
            ]
        });
        let balances = parse_spot_state(&resp).unwrap();
        assert_eq!(balances.len(), 2);
        assert_eq!(balances[0].coin, "PURR");
        assert_eq!(balances[0].token, 1);
        assert_eq!(balances[0].hold, Decimal::ZERO);
        assert_eq!(balances[0].total, Decimal::from_str("1000.0").unwrap());
        assert_eq!(balances[1].coin, "USDC");
        assert_eq!(balances[1].token, 2);
        assert_eq!(balances[1].hold, Decimal::from_str("50.0").unwrap());
        assert_eq!(balances[1].total, Decimal::from_str("500.0").unwrap());
    }

    #[test]
    fn parse_spot_state_empty_balances() {
        let resp = serde_json::json!({ "balances": [] });
        let balances = parse_spot_state(&resp).unwrap();
        assert!(balances.is_empty());
    }

    #[test]
    fn parse_spot_state_missing_balances_errors() {
        let resp = serde_json::json!({});
        assert!(parse_spot_state(&resp).is_err());
    }

    #[test]
    fn parse_spot_state_skips_empty_coin() {
        let resp = serde_json::json!({
            "balances": [
                { "coin": "", "token": 0, "hold": "0", "total": "0" },
                { "coin": "PURR", "token": 1, "hold": "0", "total": "100.0" }
            ]
        });
        let balances = parse_spot_state(&resp).unwrap();
        assert_eq!(balances.len(), 1);
        assert_eq!(balances[0].coin, "PURR");
    }

    #[test]
    fn parse_spot_state_missing_token_errors() {
        let resp = serde_json::json!({
            "balances": [
                { "coin": "PURR", "hold": "0", "total": "100.0" }
            ]
        });
        assert!(parse_spot_state(&resp).is_err());
    }

    #[test]
    fn parse_staking_delegations_basic() {
        let resp = serde_json::json!([
            { "validator": "0xval1", "amount": "1000.0", "rewards": "5.0" },
            { "validator": "0xval2", "amount": "2000.0", "rewards": "10.5" }
        ]);
        let delegations = parse_staking_delegations(&resp).unwrap();
        assert_eq!(delegations.len(), 2);
        assert_eq!(delegations[0].validator, "0xval1");
        assert_eq!(delegations[0].amount, Decimal::from_str("1000.0").unwrap());
        assert_eq!(delegations[0].rewards, Decimal::from_str("5.0").unwrap());
        assert_eq!(delegations[1].validator, "0xval2");
        assert_eq!(delegations[1].amount, Decimal::from_str("2000.0").unwrap());
        assert_eq!(delegations[1].rewards, Decimal::from_str("10.5").unwrap());
    }

    #[test]
    fn parse_staking_delegations_empty() {
        let resp = serde_json::json!([]);
        let delegations = parse_staking_delegations(&resp).unwrap();
        assert!(delegations.is_empty());
    }

    #[test]
    fn parse_staking_delegations_expects_array() {
        let resp = serde_json::json!({"not": "an array"});
        assert!(parse_staking_delegations(&resp).is_err());
    }

    #[test]
    fn parse_staking_delegations_skips_empty_validator() {
        let resp = serde_json::json!([
            { "validator": "", "amount": "100.0", "rewards": "1.0" },
            { "validator": "0xval1", "amount": "200.0", "rewards": "2.0" }
        ]);
        let delegations = parse_staking_delegations(&resp).unwrap();
        assert_eq!(delegations.len(), 1);
        assert_eq!(delegations[0].validator, "0xval1");
    }

    #[test]
    fn parse_staking_delegations_missing_amount_errors() {
        let resp = serde_json::json!([
            { "validator": "0xval1", "rewards": "1.0" }
        ]);
        assert!(parse_staking_delegations(&resp).is_err());
    }

    #[test]
    fn parse_staking_delegations_missing_rewards_errors() {
        let resp = serde_json::json!([
            { "validator": "0xval1", "amount": "100.0" }
        ]);
        assert!(parse_staking_delegations(&resp).is_err());
    }

    #[test]
    fn parse_borrow_lend_state_basic() {
        let resp = serde_json::json!({
            "balances": [
                {
                    "coin": "USDC",
                    "supply": "10000.0",
                    "borrow": "0.0",
                    "apy": "0.05"
                },
                {
                    "coin": "ETH",
                    "supply": "0.0",
                    "borrow": "5.0",
                    "apy": "0.08"
                }
            ]
        });
        let states = parse_borrow_lend_state(&resp).unwrap();
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].coin, "USDC");
        assert_eq!(states[0].supply, Decimal::from_str("10000.0").unwrap());
        assert_eq!(states[0].borrow, Decimal::ZERO);
        assert_eq!(states[0].apy, Decimal::from_str("0.05").unwrap());
        assert_eq!(states[1].coin, "ETH");
        assert_eq!(states[1].supply, Decimal::ZERO);
        assert_eq!(states[1].borrow, Decimal::from_str("5.0").unwrap());
        assert_eq!(states[1].apy, Decimal::from_str("0.08").unwrap());
    }

    #[test]
    fn parse_borrow_lend_state_skips_plain_balances() {
        let resp = serde_json::json!({
            "balances": [
                { "coin": "PURR", "token": 1, "hold": "0", "total": "1000.0" },
                {
                    "coin": "USDC",
                    "supply": "500.0",
                    "borrow": "0.0",
                    "apy": "0.03"
                }
            ]
        });
        let states = parse_borrow_lend_state(&resp).unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].coin, "USDC");
    }

    #[test]
    fn parse_borrow_lend_state_empty_balances() {
        let resp = serde_json::json!({ "balances": [] });
        let states = parse_borrow_lend_state(&resp).unwrap();
        assert!(states.is_empty());
    }

    #[test]
    fn parse_borrow_lend_state_missing_balances_errors() {
        let resp = serde_json::json!({});
        assert!(parse_borrow_lend_state(&resp).is_err());
    }

    #[test]
    fn parse_borrow_lend_state_skips_empty_coin() {
        let resp = serde_json::json!({
            "balances": [
                { "coin": "", "supply": "100.0", "borrow": "0.0", "apy": "0.01" },
                { "coin": "BTC", "supply": "1.0", "borrow": "0.0", "apy": "0.02" }
            ]
        });
        let states = parse_borrow_lend_state(&resp).unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].coin, "BTC");
    }

    #[test]
    fn parse_fills_missing_closed_pnl_defaults_to_zero() {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let resp = serde_json::json!([{
            "coin": "BTC",
            "px": "100.0",
            "sz": "1.0",
            "side": "B",
            "time": now_ms,
            "fee": "0.5"
            // "closedPnl" missing — should default to 0.0
        }]);
        let fills = parse_fills(&resp).unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].closed_pnl, Decimal::ZERO);
    }

    #[test]
    fn parse_user_fees_basic() {
        let resp = serde_json::json!({
            "userCrossRate": "0.0002",
            "userAddRate": "0.0005",
            "feeTier": "VIP1"
        });
        let fees = parse_user_fees(&resp).unwrap();
        assert_eq!(fees.fee_tier, "VIP1");
        assert_eq!(fees.maker_rate, Decimal::from_str("0.0002").unwrap());
        assert_eq!(fees.taker_rate, Decimal::from_str("0.0005").unwrap());
    }

    #[test]
    fn parse_user_fees_numeric_tier() {
        let resp = serde_json::json!({
            "userCrossRate": "0.0001",
            "userAddRate": "0.00035",
            "feeTier": 3
        });
        let fees = parse_user_fees(&resp).unwrap();
        assert_eq!(fees.fee_tier, "3");
        assert_eq!(fees.maker_rate, Decimal::from_str("0.0001").unwrap());
        assert_eq!(fees.taker_rate, Decimal::from_str("0.00035").unwrap());
    }

    #[test]
    fn parse_user_fees_missing_tier_defaults_empty() {
        let resp = serde_json::json!({
            "userCrossRate": "0.0002",
            "userAddRate": "0.0005"
        });
        let fees = parse_user_fees(&resp).unwrap();
        assert_eq!(fees.fee_tier, "");
    }

    #[test]
    fn parse_user_fees_missing_cross_rate_errors() {
        let resp = serde_json::json!({
            "userAddRate": "0.0005",
            "feeTier": "VIP1"
        });
        assert!(parse_user_fees(&resp).is_err());
    }

    #[test]
    fn parse_user_fees_missing_add_rate_errors() {
        let resp = serde_json::json!({
            "userCrossRate": "0.0002",
            "feeTier": "VIP1"
        });
        assert!(parse_user_fees(&resp).is_err());
    }

    #[test]
    fn parse_rate_limit_status_basic() {
        let resp = serde_json::json!({
            "cumVlm": "500000.0",
            "nRequestsUsed": 42,
            "nRequestsCap": 1200,
            "windowMs": 60000
        });
        let status = parse_rate_limit_status(&resp).unwrap();
        assert_eq!(status.used, 42);
        assert_eq!(status.limit, 1200);
        assert_eq!(status.window_ms, 60000);
    }

    #[test]
    fn parse_rate_limit_status_default_window() {
        let resp = serde_json::json!({
            "cumVlm": "100.0",
            "nRequestsUsed": 10,
            "nRequestsCap": 500
        });
        let status = parse_rate_limit_status(&resp).unwrap();
        assert_eq!(status.used, 10);
        assert_eq!(status.limit, 500);
        assert_eq!(status.window_ms, 60_000);
    }

    #[test]
    fn parse_rate_limit_status_missing_used_errors() {
        let resp = serde_json::json!({
            "nRequestsCap": 1200,
            "windowMs": 60000
        });
        assert!(parse_rate_limit_status(&resp).is_err());
    }

    #[test]
    fn parse_rate_limit_status_missing_cap_errors() {
        let resp = serde_json::json!({
            "nRequestsUsed": 42,
            "windowMs": 60000
        });
        assert!(parse_rate_limit_status(&resp).is_err());
    }
}
