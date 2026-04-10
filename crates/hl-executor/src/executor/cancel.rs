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
}
