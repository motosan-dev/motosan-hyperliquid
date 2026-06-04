mod parse;

pub(crate) use parse::{
    parse_account_state, parse_active_asset_data, parse_borrow_lend_state, parse_fills,
    parse_frontend_open_orders, parse_funding_history, parse_historical_orders, parse_open_orders,
    parse_order_status, parse_rate_limit_status, parse_referral_state, parse_spot_state,
    parse_staking_delegations, parse_user_fees, parse_user_funding, parse_user_staking_rewards,
    parse_user_staking_summary,
};

use std::sync::Arc;

use hl_client::{HttpTransport, HyperliquidClient};
use hl_types::{
    HlAccountState, HlActiveAssetData, HlBorrowLendState, HlError, HlExtraAgent, HlFill,
    HlFrontendOpenOrder, HlFundingEntry, HlHistoricalOrder, HlOpenOrder, HlOrderDetail, HlPosition,
    HlRateLimitStatus, HlReferralState, HlSpotBalance, HlStakingDelegation, HlStakingReward,
    HlStakingSummary, HlUserFees, HlUserFundingEntry, HlVaultDetails, HlVaultSummary,
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

    /// Fetch clearinghouse states for multiple addresses in a single request.
    ///
    /// Sends `clearinghouseStates` with an array of users and parses each
    /// entry using the same logic as [`state`](Self::state).
    #[tracing::instrument(skip(self))]
    pub async fn states(&self, addresses: &[&str]) -> Result<Vec<HlAccountState>, HlError> {
        let payload = serde_json::json!({
            "type": "clearinghouseStates",
            "users": addresses,
        });
        let resp = self.client.post_info(payload).await?;
        let arr = resp
            .as_array()
            .ok_or_else(|| HlError::Parse("expected array for clearinghouseStates".into()))?;
        arr.iter().map(parse_account_state).collect()
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

    /// Fetch fills within a time range, optionally aggregating partial fills
    /// that share a timestamp.
    ///
    /// `start_ms`/`end_ms` are Unix epoch **milliseconds**; `end_ms = None`
    /// leaves the upper bound open. Mirrors the Python SDK's `userFillsByTime`:
    /// `startTime` and `aggregateByTime` are always sent, and `endTime` is sent
    /// as JSON `null` when not provided.
    #[tracing::instrument(skip(self))]
    pub async fn fills_by_time(
        &self,
        address: &str,
        start_ms: u64,
        end_ms: Option<u64>,
        aggregate_by_time: bool,
    ) -> Result<Vec<HlFill>, HlError> {
        let payload = serde_json::json!({
            "type": "userFillsByTime",
            "user": address,
            "startTime": start_ms,
            "endTime": end_ms,
            "aggregateByTime": aggregate_by_time,
        });
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

    /// List the sub-accounts under an address.
    ///
    /// Returns the raw JSON response — an array of sub-account objects
    /// (`name`, `subAccountUser`, `master`, nested `clearinghouseState` /
    /// `spotState`), or JSON `null` when the user has none. The nested state is
    /// deeply nested and variable, so it is returned unparsed.
    #[tracing::instrument(skip(self))]
    pub async fn query_sub_accounts(&self, address: &str) -> Result<serde_json::Value, HlError> {
        let payload = serde_json::json!({ "type": "subAccounts", "user": address });
        self.client.post_info(payload).await
    }

    /// Fetch the account value / PnL / volume history for an address.
    ///
    /// Returns the raw JSON response — a heterogeneous array of
    /// `[period_label, data]` pairs (e.g. `"day"`, `"week"`, `"allTime"`) whose
    /// inner shape varies, so it is returned unparsed.
    #[tracing::instrument(skip(self))]
    pub async fn portfolio(&self, address: &str) -> Result<serde_json::Value, HlError> {
        let payload = serde_json::json!({ "type": "portfolio", "user": address });
        self.client.post_info(payload).await
    }

    /// Fetch open orders for an address.
    #[tracing::instrument(skip(self))]
    pub async fn open_orders(&self, address: &str) -> Result<Vec<HlOpenOrder>, HlError> {
        let payload = serde_json::json!({"type": "openOrders", "user": address});
        let resp = self.client.post_info(payload).await?;
        parse_open_orders(&resp)
    }

    /// Fetch open orders for an address with the richer "frontend" view —
    /// trigger conditions, TP/SL bracket metadata, original size, and child
    /// orders — as needed to manage existing stop/take-profit orders.
    ///
    /// `dex` scopes the query to a specific builder/HIP-3 DEX; `None` queries the
    /// primary perp DEX (the empty-string default).
    #[tracing::instrument(skip(self))]
    pub async fn frontend_open_orders(
        &self,
        address: &str,
        dex: Option<&str>,
    ) -> Result<Vec<HlFrontendOpenOrder>, HlError> {
        let payload = serde_json::json!({
            "type": "frontendOpenOrders",
            "user": address,
            "dex": dex.unwrap_or(""),
        });
        let resp = self.client.post_info(payload).await?;
        parse_frontend_open_orders(&resp)
    }

    /// Fetch the status of a specific order by its exchange order id (oid).
    #[tracing::instrument(skip(self))]
    pub async fn order_status(&self, address: &str, oid: u64) -> Result<HlOrderDetail, HlError> {
        let payload = serde_json::json!({"type": "orderStatus", "user": address, "oid": oid});
        let resp = self.client.post_info(payload).await?;
        parse_order_status(&resp)
    }

    /// Fetch the status of a specific order by its client order id (cloid).
    ///
    /// The `cloid` should be the canonical `0x`-prefixed, 32-hex-char client
    /// order id. This uses the same `orderStatus` query as [`Self::order_status`]
    /// — the cloid is sent under the same `oid` field but as a JSON string, which
    /// is how the exchange distinguishes a cloid lookup from an oid lookup.
    #[tracing::instrument(skip(self))]
    pub async fn order_status_by_cloid(
        &self,
        address: &str,
        cloid: &str,
    ) -> Result<HlOrderDetail, HlError> {
        let payload = serde_json::json!({"type": "orderStatus", "user": address, "oid": cloid});
        let resp = self.client.post_info(payload).await?;
        parse_order_status(&resp)
    }

    /// Fetch funding history for a coin.
    #[tracing::instrument(skip(self))]
    pub async fn funding_history(
        &self,
        coin: &str,
        start_time: u64,
        end_time: Option<u64>,
    ) -> Result<Vec<HlFundingEntry>, HlError> {
        let mut payload = serde_json::json!({
            "type": "fundingHistory",
            "coin": coin,
            "startTime": start_time,
        });
        if let Some(et) = end_time {
            payload
                .as_object_mut()
                .ok_or_else(|| HlError::Parse("payload is not a JSON object".into()))?
                .insert("endTime".to_string(), serde_json::Value::Number(et.into()));
        }
        let resp = self.client.post_info(payload).await?;
        parse_funding_history(&resp)
    }

    /// Fetch user funding history for an address.
    #[tracing::instrument(skip(self))]
    pub async fn user_funding(
        &self,
        address: &str,
        start_time: u64,
        end_time: Option<u64>,
    ) -> Result<Vec<HlUserFundingEntry>, HlError> {
        let mut payload = serde_json::json!({
            "type": "userFunding",
            "user": address,
            "startTime": start_time,
        });
        if let Some(et) = end_time {
            payload
                .as_object_mut()
                .ok_or_else(|| HlError::Parse("payload is not a JSON object".into()))?
                .insert("endTime".to_string(), serde_json::Value::Number(et.into()));
        }
        let resp = self.client.post_info(payload).await?;
        parse_user_funding(&resp)
    }

    /// Fetch historical orders for an address.
    #[tracing::instrument(skip(self))]
    pub async fn historical_orders(
        &self,
        address: &str,
    ) -> Result<Vec<HlHistoricalOrder>, HlError> {
        let payload = serde_json::json!({"type": "historicalOrders", "user": address});
        let resp = self.client.post_info(payload).await?;
        parse_historical_orders(&resp)
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

    /// Fetch the staking summary for an address (`delegatorSummary`):
    /// total delegated/undelegated and pending withdrawals.
    #[tracing::instrument(skip(self))]
    pub async fn user_staking_summary(&self, address: &str) -> Result<HlStakingSummary, HlError> {
        let payload = serde_json::json!({ "type": "delegatorSummary", "user": address });
        let resp = self.client.post_info(payload).await?;
        parse_user_staking_summary(&resp)
    }

    /// Fetch the staking reward history for an address (`delegatorRewards`).
    #[tracing::instrument(skip(self))]
    pub async fn user_staking_rewards(
        &self,
        address: &str,
    ) -> Result<Vec<HlStakingReward>, HlError> {
        let payload = serde_json::json!({ "type": "delegatorRewards", "user": address });
        let resp = self.client.post_info(payload).await?;
        parse_user_staking_rewards(&resp)
    }

    /// Fetch the full delegation/undelegation history for an address
    /// (`delegatorHistory`).
    ///
    /// Returns the raw JSON response — the event schema (timestamps, tx hashes,
    /// delta details) is not yet stabilized, so it is returned unparsed.
    #[tracing::instrument(skip(self))]
    pub async fn delegator_history(&self, address: &str) -> Result<serde_json::Value, HlError> {
        let payload = serde_json::json!({ "type": "delegatorHistory", "user": address });
        self.client.post_info(payload).await
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

    /// Fetch referral state for an address.
    #[tracing::instrument(skip(self))]
    pub async fn referral_state(&self, address: &str) -> Result<HlReferralState, HlError> {
        let payload = serde_json::json!({ "type": "referral", "user": address });
        let resp = self.client.post_info(payload).await?;
        parse_referral_state(&resp)
    }

    /// Fetch active asset data for a user's position in a specific coin.
    #[tracing::instrument(skip(self))]
    pub async fn active_asset_data(
        &self,
        user: &str,
        coin: &str,
    ) -> Result<HlActiveAssetData, HlError> {
        let payload = serde_json::json!({
            "type": "activeAssetData",
            "user": user,
            "coin": coin,
        });
        let resp = self.client.post_info(payload).await?;
        parse_active_asset_data(&resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hl_test_utils::MockTransport;

    fn mock_clearinghouse_state() -> serde_json::Value {
        serde_json::json!({
            "marginSummary": {
                "accountValue": "10000.0",
                "totalRawUsd": "5000.0"
            },
            "assetPositions": [
                {
                    "position": {
                        "coin": "ETH",
                        "szi": "1.5",
                        "entryPx": "2000.0",
                        "unrealizedPnl": "100.0",
                        "leverage": { "value": "5.0" },
                        "liquidationPx": "1600.0"
                    }
                }
            ]
        })
    }

    #[tokio::test]
    async fn states_parses_batch_response() {
        let mock_resp = serde_json::json!([mock_clearinghouse_state(), mock_clearinghouse_state()]);
        let transport = Arc::new(MockTransport::new(vec![mock_resp]));
        let account = Account::new(transport);

        let result = account
            .states(&["0xabc", "0xdef"])
            .await
            .expect("batch states should succeed");

        assert_eq!(result.len(), 2);
        for state in &result {
            assert_eq!(state.equity, rust_decimal::Decimal::from(10000));
            assert_eq!(state.positions.len(), 1);
            assert_eq!(state.positions[0].coin, "ETH");
        }
    }

    #[tokio::test]
    async fn states_empty_array() {
        let mock_resp = serde_json::json!([]);
        let transport = Arc::new(MockTransport::new(vec![mock_resp]));
        let account = Account::new(transport);

        let result = account
            .states(&[])
            .await
            .expect("empty batch should succeed");

        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn states_rejects_non_array() {
        let mock_resp = serde_json::json!({"not": "an array"});
        let transport = Arc::new(MockTransport::new(vec![mock_resp]));
        let account = Account::new(transport);

        let result = account.states(&["0xabc"]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fills_by_time_parses_response() {
        let mock_resp = serde_json::json!([
            {"coin": "BTC", "px": "90000.0", "sz": "0.5", "side": "B", "time": 1_700_000_000_000u64, "fee": "1.2", "closedPnl": "0"},
            {"coin": "ETH", "px": "3000.0", "sz": "2.0", "side": "A", "time": 1_700_000_050_000u64, "fee": "0.4", "closedPnl": "5"}
        ]);
        let transport = Arc::new(MockTransport::new(vec![mock_resp]));
        let account = Account::new(transport);

        let result = account
            .fills_by_time("0xabc", 1_700_000_000_000, Some(1_700_000_100_000), false)
            .await
            .expect("fills_by_time should parse");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].coin, "BTC");
        assert!(result[0].is_buy);
        assert_eq!(result[1].coin, "ETH");
        assert!(!result[1].is_buy);
    }

    #[tokio::test]
    async fn fills_by_time_open_ended_and_empty() {
        let transport = Arc::new(MockTransport::new(vec![serde_json::json!([])]));
        let account = Account::new(transport);
        let result = account
            .fills_by_time("0xabc", 1_700_000_000_000, None, true)
            .await
            .expect("empty array should parse");
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn fills_by_time_rejects_non_array() {
        let transport = Arc::new(MockTransport::new(vec![serde_json::json!({"x": 1})]));
        let account = Account::new(transport);
        assert!(account
            .fills_by_time("0xabc", 1, None, false)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn order_status_by_cloid_parses() {
        let mock_resp = serde_json::json!({
            "order": {
                "oid": 555, "coin": "SOL", "side": "B", "limitPx": "150.0", "sz": "10.0",
                "timestamp": 1_700_000_000_000u64, "orderType": "Limit",
                "cloid": "0x00000000000000000000000000000001"
            },
            "status": "filled"
        });
        let transport = Arc::new(MockTransport::new(vec![mock_resp]));
        let account = Account::new(transport);
        let detail = account
            .order_status_by_cloid("0xabc", "0x00000000000000000000000000000001")
            .await
            .expect("order_status_by_cloid should parse");
        assert_eq!(detail.oid, 555);
        assert_eq!(detail.coin, "SOL");
    }

    #[tokio::test]
    async fn frontend_open_orders_parses_trigger_and_children() {
        let mock_resp = serde_json::json!([
            {
                "oid": 1, "coin": "BTC", "side": "A", "limitPx": "0.0", "sz": "0.2",
                "origSz": "0.2", "timestamp": 1_700_000_000_000u64,
                "orderType": "Stop Market", "tif": null, "reduceOnly": true,
                "isTrigger": true, "isPositionTpsl": false,
                "triggerCondition": "Price below 60000", "triggerPx": "60000.0",
                "children": []
            },
            {
                "oid": 2, "coin": "ETH", "side": "B", "limitPx": "3000.0", "sz": "1.0",
                "origSz": "1.0", "timestamp": 1_700_000_001_000u64,
                "orderType": "Limit", "tif": "Gtc", "reduceOnly": false,
                "isTrigger": false, "isPositionTpsl": true,
                "triggerCondition": "N/A", "triggerPx": "0.0",
                "children": [
                    {
                        "oid": 3, "coin": "ETH", "side": "A", "limitPx": "0.0", "sz": "1.0",
                        "origSz": "1.0", "timestamp": 1_700_000_001_000u64,
                        "orderType": "Take Profit Market", "tif": null, "reduceOnly": true,
                        "isTrigger": true, "isPositionTpsl": true,
                        "triggerCondition": "Price above 3500", "triggerPx": "3500.0"
                    }
                ]
            }
        ]);
        let transport = Arc::new(MockTransport::new(vec![mock_resp]));
        let account = Account::new(transport);
        let orders = account
            .frontend_open_orders("0xabc", None)
            .await
            .expect("frontend_open_orders should parse");
        assert_eq!(orders.len(), 2);
        // First: a trigger order with no children.
        assert!(orders[0].is_trigger);
        assert!(orders[0].reduce_only);
        assert_eq!(orders[0].trigger_px, rust_decimal::Decimal::from(60000));
        assert_eq!(orders[0].order_type, "Stop Market");
        assert!(orders[0].tif.is_none());
        assert!(orders[0].children.is_empty());
        // Second: a position-TPSL parent with one recursive child.
        assert!(!orders[1].is_trigger);
        assert!(orders[1].is_position_tpsl);
        assert_eq!(orders[1].tif.as_deref(), Some("Gtc"));
        assert_eq!(orders[1].children.len(), 1);
        assert!(orders[1].children[0].is_trigger);
    }

    #[tokio::test]
    async fn frontend_open_orders_rejects_non_array() {
        let transport = Arc::new(MockTransport::new(vec![serde_json::json!({"x": 1})]));
        let account = Account::new(transport);
        assert!(account.frontend_open_orders("0xabc", None).await.is_err());
    }

    // ── Outbound wire-format regression guards ─────────────────

    #[tokio::test]
    async fn order_status_wire_oid_is_number() {
        let transport = Arc::new(MockTransport::new(vec![serde_json::json!({"status": "x"})]));
        let account = Account::new(transport.clone());
        let _ = account.order_status("0xabc", 555).await;
        let req = transport.last_request().unwrap();
        assert_eq!(req["type"], "orderStatus");
        assert_eq!(req["oid"].as_u64(), Some(555));
        assert!(req["oid"].as_str().is_none());
    }

    #[tokio::test]
    async fn order_status_by_cloid_wire_oid_is_string() {
        let transport = Arc::new(MockTransport::new(vec![serde_json::json!({"status": "x"})]));
        let account = Account::new(transport.clone());
        let _ = account
            .order_status_by_cloid("0xabc", "0x00000000000000000000000000000001")
            .await;
        let req = transport.last_request().unwrap();
        assert_eq!(req["type"], "orderStatus");
        // cloid travels under the SAME `oid` key but as a JSON string, with no
        // separate `cloid` key — that's how the exchange disambiguates.
        assert_eq!(req["oid"], "0x00000000000000000000000000000001");
        assert!(req["oid"].as_str().is_some());
        assert!(req.get("cloid").is_none());
    }

    #[tokio::test]
    async fn fills_by_time_wire_endtime_null_when_none() {
        let transport = Arc::new(MockTransport::new(vec![serde_json::json!([])]));
        let account = Account::new(transport.clone());
        account
            .fills_by_time("0xabc", 1_700_000_000_000, None, true)
            .await
            .unwrap();
        let req = transport.last_request().unwrap();
        assert_eq!(req["type"], "userFillsByTime");
        assert_eq!(req["startTime"].as_u64(), Some(1_700_000_000_000));
        // endTime is sent as null (not omitted); aggregateByTime always present.
        assert!(req["endTime"].is_null());
        assert_eq!(req["aggregateByTime"], true);
    }

    #[tokio::test]
    async fn fills_by_time_wire_endtime_present_when_some() {
        let transport = Arc::new(MockTransport::new(vec![serde_json::json!([])]));
        let account = Account::new(transport.clone());
        account
            .fills_by_time("0xabc", 1, Some(2), false)
            .await
            .unwrap();
        let req = transport.last_request().unwrap();
        assert_eq!(req["endTime"].as_u64(), Some(2));
        assert_eq!(req["aggregateByTime"], false);
    }

    #[tokio::test]
    async fn frontend_open_orders_wire_keys() {
        let transport = Arc::new(MockTransport::new(vec![serde_json::json!([])]));
        let account = Account::new(transport.clone());
        account.frontend_open_orders("0xabc", None).await.unwrap();
        let req = transport.last_request().unwrap();
        assert_eq!(req["type"], "frontendOpenOrders");
        assert_eq!(req["user"], "0xabc");
        assert_eq!(req["dex"], "");
    }

    // ── P2 info queries ────────────────────────────────────────

    #[tokio::test]
    async fn query_sub_accounts_returns_raw_array_and_wire() {
        let resp = serde_json::json!([
            {"name": "Test", "subAccountUser": "0x0356", "master": "0x8c96",
             "clearinghouseState": {}, "spotState": {"balances": []}}
        ]);
        let transport = Arc::new(MockTransport::new(vec![resp]));
        let account = Account::new(transport.clone());
        let v = account.query_sub_accounts("0xabc").await.unwrap();
        assert!(v.is_array());
        assert_eq!(v[0]["subAccountUser"], "0x0356");
        let req = transport.last_request().unwrap();
        assert_eq!(req["type"], "subAccounts");
        assert_eq!(req["user"], "0xabc");
    }

    #[tokio::test]
    async fn query_sub_accounts_handles_null() {
        let transport = Arc::new(MockTransport::new(vec![serde_json::Value::Null]));
        let account = Account::new(transport);
        let v = account.query_sub_accounts("0xabc").await.unwrap();
        assert!(v.is_null());
    }

    #[tokio::test]
    async fn portfolio_returns_raw_value_and_wire() {
        let resp = serde_json::json!([
            ["day", {"accountValueHistory": [[1_700_000_000_000u64, "100.0"]], "pnlHistory": [], "vlm": "250.0"}]
        ]);
        let transport = Arc::new(MockTransport::new(vec![resp]));
        let account = Account::new(transport.clone());
        let v = account.portfolio("0xabc").await.unwrap();
        assert_eq!(v[0][0], "day");
        assert_eq!(v[0][1]["vlm"], "250.0");
        let req = transport.last_request().unwrap();
        assert_eq!(req["type"], "portfolio");
        assert_eq!(req["user"], "0xabc");
        assert_eq!(req.as_object().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn user_staking_summary_parses() {
        let resp = serde_json::json!({
            "delegated": "1000.0", "undelegated": "50.0",
            "totalPendingWithdrawal": "10.0", "nPendingWithdrawals": 2
        });
        let transport = Arc::new(MockTransport::new(vec![resp]));
        let account = Account::new(transport.clone());
        let s = account.user_staking_summary("0xabc").await.unwrap();
        assert_eq!(s.delegated, rust_decimal::Decimal::from(1000));
        assert_eq!(s.undelegated, rust_decimal::Decimal::from(50));
        assert_eq!(s.n_pending_withdrawals, 2);
        let req = transport.last_request().unwrap();
        assert_eq!(req["type"], "delegatorSummary");
    }

    #[tokio::test]
    async fn user_staking_rewards_parses_array() {
        let resp = serde_json::json!([
            {"time": 1_700_000_000_000u64, "source": "delegation", "totalAmount": "5.0"},
            {"time": 1_700_000_050_000u64, "source": "commission", "totalAmount": "1.25"}
        ]);
        let transport = Arc::new(MockTransport::new(vec![resp]));
        let account = Account::new(transport.clone());
        let r = account.user_staking_rewards("0xabc").await.unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].source, "delegation");
        assert_eq!(r[0].total_amount, rust_decimal::Decimal::from(5));
        assert_eq!(r[1].time, 1_700_000_050_000);
        assert_eq!(
            transport.last_request().unwrap()["type"],
            "delegatorRewards"
        );
    }

    #[tokio::test]
    async fn user_staking_rewards_rejects_non_array() {
        let transport = Arc::new(MockTransport::new(vec![serde_json::json!({"x": 1})]));
        let account = Account::new(transport);
        assert!(account.user_staking_rewards("0xabc").await.is_err());
    }

    #[tokio::test]
    async fn user_staking_rewards_empty_array() {
        let transport = Arc::new(MockTransport::new(vec![serde_json::json!([])]));
        let account = Account::new(transport);
        assert!(account
            .user_staking_rewards("0xabc")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn user_staking_summary_missing_field_errors() {
        // No `delegated` field -> parse error.
        let transport = Arc::new(MockTransport::new(vec![serde_json::json!({
            "undelegated": "1.0", "totalPendingWithdrawal": "0.0", "nPendingWithdrawals": 0
        })]));
        let account = Account::new(transport);
        assert!(account.user_staking_summary("0xabc").await.is_err());
    }

    #[tokio::test]
    async fn delegator_history_returns_raw_value() {
        let resp = serde_json::json!([{"time": 1, "hash": "0xabc", "delta": {}}]);
        let transport = Arc::new(MockTransport::new(vec![resp.clone()]));
        let account = Account::new(transport.clone());
        let v = account.delegator_history("0xabc").await.unwrap();
        assert_eq!(v, resp);
        assert_eq!(
            transport.last_request().unwrap()["type"],
            "delegatorHistory"
        );
    }
}
