use hl_types::*;

use super::OrderExecutor;

impl OrderExecutor {
    /// Cancel an order by asset index and exchange order ID.
    #[tracing::instrument(skip(self), fields(asset, oid))]
    pub async fn cancel_order(
        &self,
        asset: u32,
        oid: u64,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({
            "type": "cancel",
            "cancels": [{"a": asset, "o": oid}]
        });

        let result = self.send_signed_action(action, vault).await?;

        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("cancel_order response: {e}")))
    }

    /// Cancel multiple orders in a single request.
    #[tracing::instrument(skip(self, cancels), fields(count = cancels.len()))]
    pub async fn bulk_cancel(
        &self,
        cancels: Vec<CancelRequest>,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let cancel_entries: Vec<serde_json::Value> = cancels
            .iter()
            .map(|c| serde_json::json!({"a": c.asset, "o": c.oid}))
            .collect();
        let action = serde_json::json!({
            "type": "cancel",
            "cancels": cancel_entries
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("bulk_cancel response: {e}")))
    }

    /// Cancel an order by client order ID (cloid).
    #[tracing::instrument(skip(self))]
    pub async fn cancel_by_cloid(
        &self,
        symbol: &str,
        cloid: &str,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let asset = self.resolve_asset(symbol)?;
        let action = serde_json::json!({
            "type": "cancelByCloid",
            "cancels": [{"asset": asset, "cloid": cloid}]
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("cancel_by_cloid response: {e}")))
    }

    /// Cancel multiple orders by client order ID (cloid) in a single request.
    #[tracing::instrument(skip(self, cancels), fields(count = cancels.len()))]
    pub async fn bulk_cancel_by_cloid(
        &self,
        cancels: Vec<CancelByCloidRequest>,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let cancel_entries: Vec<serde_json::Value> = cancels
            .iter()
            .map(|c| serde_json::json!({"asset": c.asset, "cloid": c.cloid}))
            .collect();
        let action = serde_json::json!({
            "type": "cancelByCloid",
            "cancels": cancel_entries
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("bulk_cancel_by_cloid response: {e}")))
    }
}
