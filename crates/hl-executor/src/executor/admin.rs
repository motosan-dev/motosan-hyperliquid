use hl_types::*;

use super::OrderExecutor;

impl OrderExecutor {
    /// Approve a trading agent for this account.
    #[tracing::instrument(skip(self))]
    pub async fn approve_agent(
        &self,
        agent_address: &str,
        agent_name: Option<&str>,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let chain = if self.client.is_mainnet() { "Mainnet" } else { "Testnet" };
        let mut action = serde_json::json!({
            "type": "approveAgent",
            "hyperliquidChain": chain,
            "signatureChainId": "0xa4b1",
            "agentAddress": agent_address,
            "nonce": self.next_nonce(),
        });
        if let Some(name) = agent_name {
            action.as_object_mut().unwrap().insert(
                "agentName".to_string(),
                serde_json::Value::String(name.to_string()),
            );
        }
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("approve_agent response: {e}")))
    }

    /// Schedule cancellation of all open orders at a future time.
    /// Pass `None` to clear a previously scheduled cancellation.
    #[tracing::instrument(skip(self))]
    pub async fn schedule_cancel(
        &self,
        time: Option<u64>,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let action = if let Some(t) = time {
            serde_json::json!({"type": "scheduleCancel", "time": t})
        } else {
            serde_json::json!({"type": "scheduleCancel", "time": null})
        };
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("schedule_cancel response: {e}")))
    }

    /// Claim earned trading rewards.
    #[tracing::instrument(skip(self))]
    pub async fn claim_rewards(
        &self,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({"type": "claimRewards"});
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("claim_rewards response: {e}")))
    }
}
