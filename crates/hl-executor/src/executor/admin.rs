use hl_types::{HlActionResponse, HlError};

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
        let chain = self.chain_name();
        let nonce = self.next_nonce();
        let mut action = serde_json::json!({
            "type": "approveAgent",
            "hyperliquidChain": chain,
            "signatureChainId": "0xa4b1",
            "agentAddress": agent_address,
            "nonce": nonce,
        });
        if let Some(name) = agent_name {
            action
                .as_object_mut()
                .ok_or_else(|| HlError::serialization("payload is not a JSON object"))?
                .insert(
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

        Self::check_and_parse_response(result, "approveAgent")
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

    /// Approve a builder fee for MEV protection.
    ///
    /// Uses EIP-712 user-signed-action signing.
    #[tracing::instrument(skip(self))]
    pub async fn approve_builder_fee(
        &self,
        builder: &str,
        max_fee_rate: &str,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let chain = self.chain_name();
        let nonce = self.next_nonce();
        let action = serde_json::json!({
            "type": "approveBuilderFee",
            "hyperliquidChain": chain,
            "signatureChainId": "0xa4b1",
            "maxFeeRate": max_fee_rate,
            "builder": builder,
            "nonce": nonce,
        });

        let types = vec![
            hl_signing::EIP712Field::new("hyperliquidChain", "string"),
            hl_signing::EIP712Field::new("maxFeeRate", "string"),
            hl_signing::EIP712Field::new("builder", "address"),
            hl_signing::EIP712Field::new("nonce", "uint64"),
        ];

        let signature = hl_signing::sign_user_signed_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            &types,
            "HyperliquidTransaction:ApproveBuilderFee",
            self.client.is_mainnet(),
        )?;

        let result = self
            .client
            .post_action(action, &signature, nonce, vault)
            .await?;

        Self::check_and_parse_response(result, "approveBuilderFee")
    }

    /// Modify EVM user configuration.
    ///
    /// The `modifications` parameter is a JSON object describing the changes
    /// to apply. Common fields include:
    ///
    /// - `"usingBigBlocks"` (`bool`) — opt into big-block mode for higher throughput
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use serde_json::json;
    /// # async fn example(executor: &hl_executor::OrderExecutor) -> Result<(), hl_types::HlError> {
    /// // Enable big-block mode
    /// executor.evm_user_modify(json!({"usingBigBlocks": true}), None).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Refer to the [Hyperliquid API documentation](https://hyperliquid.gitbook.io/hyperliquid-docs)
    /// for the full list of supported modification fields.
    #[tracing::instrument(skip(self))]
    pub async fn evm_user_modify(
        &self,
        modifications: serde_json::Value,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({
            "type": "evmUserModify",
            "modifications": modifications,
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("evm_user_modify response: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use async_trait::async_trait;
    use hl_client::HttpTransport;
    use hl_signing::PrivateKeySigner;
    use hl_types::Signature;
    use std::sync::{Arc, Mutex};

    use crate::meta_cache::AssetMetaCache;

    struct MockTransport {
        responses: Mutex<Vec<serde_json::Value>>,
        is_mainnet: bool,
    }

    impl MockTransport {
        fn new(responses: Vec<serde_json::Value>) -> Self {
            Self {
                responses: Mutex::new(responses),
                is_mainnet: true,
            }
        }
    }

    #[async_trait]
    impl HttpTransport for MockTransport {
        async fn post_info(&self, _req: serde_json::Value) -> Result<serde_json::Value, HlError> {
            let mut q = self.responses.lock().unwrap();
            if q.is_empty() {
                return Err(HlError::http("no mock responses"));
            }
            Ok(q.remove(0))
        }

        async fn post_action(
            &self,
            _action: serde_json::Value,
            _sig: &Signature,
            _nonce: u64,
            _vault: Option<&str>,
        ) -> Result<serde_json::Value, HlError> {
            let mut q = self.responses.lock().unwrap();
            if q.is_empty() {
                return Err(HlError::http("no mock responses"));
            }
            Ok(q.remove(0))
        }

        fn is_mainnet(&self) -> bool {
            self.is_mainnet
        }
    }

    fn test_signer() -> Box<dyn hl_signing::Signer> {
        Box::new(
            PrivateKeySigner::from_hex(
                "0x0000000000000000000000000000000000000000000000000000000000000001",
            )
            .unwrap(),
        )
    }

    fn test_executor(responses: Vec<serde_json::Value>) -> OrderExecutor {
        let mut name_to_idx = std::collections::HashMap::new();
        name_to_idx.insert("BTC".to_string(), 0u32);
        name_to_idx.insert("ETH".to_string(), 1u32);
        let cache = AssetMetaCache::from_maps(name_to_idx, Default::default());
        OrderExecutor::with_meta_cache(
            Arc::new(MockTransport::new(responses)),
            test_signer(),
            "0x0000000000000000000000000000000000000001".to_string(),
            cache,
        )
    }

    fn ok_response() -> serde_json::Value {
        serde_json::json!({"status": "ok", "response": {"type": "default"}})
    }

    #[tokio::test]
    async fn approve_agent_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .approve_agent(
                "0x0000000000000000000000000000000000000099",
                Some("my-bot"),
                None,
            )
            .await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn approve_agent_without_name() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .approve_agent("0x0000000000000000000000000000000000000099", None, None)
            .await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn approve_builder_fee_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .approve_builder_fee("0x0000000000000000000000000000000000000077", "0.001", None)
            .await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn set_referrer_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor.set_referrer("MYCODE", None).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn evm_user_modify_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .evm_user_modify(serde_json::json!({"usingBigBlocks": true}), None)
            .await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }
}
