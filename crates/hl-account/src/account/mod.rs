mod parse;

pub(crate) use parse::{
    parse_account_state, parse_borrow_lend_state, parse_fills, parse_funding_history,
    parse_historical_orders, parse_open_orders, parse_order_status, parse_rate_limit_status,
    parse_spot_state, parse_staking_delegations, parse_user_fees, parse_user_funding,
};

use std::sync::Arc;

use hl_client::{HttpTransport, HyperliquidClient};
use hl_types::{
    HlAccountState, HlBorrowLendState, HlError, HlExtraAgent, HlFill, HlFundingEntry,
    HlHistoricalOrder, HlOpenOrder, HlOrderDetail, HlPosition, HlRateLimitStatus, HlSpotBalance,
    HlStakingDelegation, HlUserFees, HlUserFundingEntry, HlVaultDetails, HlVaultSummary,
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
    pub async fn open_orders(&self, address: &str) -> Result<Vec<HlOpenOrder>, HlError> {
        let payload = serde_json::json!({"type": "openOrders", "user": address});
        let resp = self.client.post_info(payload).await?;
        parse_open_orders(&resp)
    }

    /// Fetch the status of a specific order.
    #[tracing::instrument(skip(self))]
    pub async fn order_status(&self, address: &str, oid: u64) -> Result<HlOrderDetail, HlError> {
        let payload = serde_json::json!({"type": "orderStatus", "user": address, "oid": oid});
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
