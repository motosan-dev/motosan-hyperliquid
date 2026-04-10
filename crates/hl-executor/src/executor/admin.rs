use hl_types::*;

use super::OrderExecutor;

impl OrderExecutor {
    /// Approve a trading agent for this account.
    ///
    /// Uses EIP-712 user-signed-action signing (not L1 action signing).
    #[tracing::instrument(skip(self))]
    pub async fn approve_agent(
        &self,
        agent_address: &str,
        agent_name: Option<&str>,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let chain = if self.client.is_mainnet() {
            "Mainnet"
        } else {
            "Testnet"
        };
        let nonce = self.next_nonce();
        let mut action = serde_json::json!({
            "type": "approveAgent",
            "hyperliquidChain": chain,
            "signatureChainId": "0xa4b1",
            "agentAddress": agent_address,
            "nonce": nonce,
        });
        if let Some(name) = agent_name {
            action.as_object_mut().unwrap().insert(
                "agentName".to_string(),
                serde_json::Value::String(name.to_string()),
            );
        }

        let mut types = vec![
            hl_signing::EIP712Field::new("hyperliquidChain", "string"),
            hl_signing::EIP712Field::new("agentAddress", "address"),
            hl_signing::EIP712Field::new("agentName", "string"),
            hl_signing::EIP712Field::new("nonce", "uint64"),
        ];

        // If no agent name, remove it from the types
        if agent_name.is_none() {
            types.retain(|f| f.name != "agentName");
        }

        let signature = hl_signing::sign_user_signed_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            &types,
            "HyperliquidTransaction:ApproveAgent",
            self.client.is_mainnet(),
        )?;

        let result = self
            .client
            .post_action(action, &signature, nonce, vault)
            .await?;

        let api_status = result
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");
        if api_status != "ok" {
            return Err(HlError::Rejected {
                reason: format!("Exchange rejected approveAgent: {}", result),
            });
        }

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
    pub async fn claim_rewards(&self, vault: Option<&str>) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({"type": "claimRewards"});
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("claim_rewards response: {e}")))
    }

    /// Set a referrer code for this account.
    ///
    /// This is a one-time action per account. Once a referrer code is set it
    /// cannot be changed.
    #[tracing::instrument(skip(self))]
    pub async fn set_referrer(
        &self,
        code: &str,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({"type": "setReferrer", "code": code});
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("set_referrer response: {e}")))
    }
}
