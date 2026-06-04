use hl_types::{HlActionResponse, HlError};

use super::{validate_eth_address, OrderExecutor, SIGNATURE_CHAIN_ID};

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
        validate_eth_address(agent_address)?;
        let chain = self.chain_name();
        let nonce = self.next_nonce();
        let mut action = serde_json::json!({
            "type": "approveAgent",
            "hyperliquidChain": chain,
            "signatureChainId": SIGNATURE_CHAIN_ID,
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
            .post_action(action, &signature, nonce, vault, None)
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
        validate_eth_address(builder)?;
        let chain = self.chain_name();
        let nonce = self.next_nonce();
        let action = serde_json::json!({
            "type": "approveBuilderFee",
            "hyperliquidChain": chain,
            "signatureChainId": SIGNATURE_CHAIN_ID,
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
            .post_action(action, &signature, nonce, vault, None)
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

    /// Delegate (stake) or undelegate (unstake) native tokens to a validator.
    ///
    /// `wei` is the **raw base-unit** amount (a `uint64`, not scaled to
    /// micro-units). `is_undelegate = true` unstakes, `false` stakes. Uses
    /// EIP-712 user-signed-action signing (`HyperliquidTransaction:TokenDelegate`).
    #[tracing::instrument(skip(self))]
    pub async fn token_delegate(
        &self,
        validator: &str,
        wei: u64,
        is_undelegate: bool,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        validate_eth_address(validator)?;
        let chain = self.chain_name();
        let nonce = self.next_nonce();
        let action = serde_json::json!({
            "type": "tokenDelegate",
            "hyperliquidChain": chain,
            "signatureChainId": SIGNATURE_CHAIN_ID,
            "validator": validator,
            "wei": wei,
            "isUndelegate": is_undelegate,
            "nonce": nonce,
        });

        // Field order must match the canonical TOKEN_DELEGATE_TYPES.
        let types = vec![
            hl_signing::EIP712Field::new("hyperliquidChain", "string"),
            hl_signing::EIP712Field::new("validator", "address"),
            hl_signing::EIP712Field::new("wei", "uint64"),
            hl_signing::EIP712Field::new("isUndelegate", "bool"),
            hl_signing::EIP712Field::new("nonce", "uint64"),
        ];

        let signature = hl_signing::sign_user_signed_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            &types,
            "HyperliquidTransaction:TokenDelegate",
            self.client.is_mainnet(),
        )?;

        let result = self
            .client
            .post_action(action, &signature, nonce, vault, None)
            .await?;

        Self::check_and_parse_response(result, "tokenDelegate")
    }
}

#[cfg(test)]
mod tests {
    use hl_test_utils::{ok_response, test_executor, test_executor_capturing};
    use hl_types::HlError;

    #[tokio::test]
    async fn token_delegate_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .token_delegate(
                "0x0000000000000000000000000000000000000099",
                1_000_000_000_000_000_000u64,
                false,
                None,
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, "ok");
    }

    #[tokio::test]
    async fn token_undelegate_success() {
        let (executor, transport) = test_executor_capturing(vec![ok_response()]);
        let result = executor
            .token_delegate(
                "0x0000000000000000000000000000000000000099",
                100,
                true,
                None,
            )
            .await;
        assert!(result.is_ok());
        // Lock in that the undelegate flag actually reaches the wire as `true`.
        assert_eq!(transport.last_request().unwrap()["isUndelegate"], true);
    }

    #[tokio::test]
    async fn token_delegate_rejects_invalid_validator() {
        let executor = test_executor(vec![]);
        let result = executor
            .token_delegate("not-an-address", 100, false, None)
            .await;
        assert!(matches!(result, Err(HlError::InvalidAddress(_))));
    }

    #[tokio::test]
    async fn token_delegate_wire_format() {
        let (executor, transport) = test_executor_capturing(vec![ok_response()]);
        executor
            .token_delegate(
                "0x0000000000000000000000000000000000000099",
                42,
                false,
                None,
            )
            .await
            .unwrap();
        let req = transport.last_request().unwrap();
        assert_eq!(req["type"], "tokenDelegate");
        assert_eq!(
            req["validator"],
            "0x0000000000000000000000000000000000000099"
        );
        // wei is a JSON integer (uint64), not a string.
        assert_eq!(req["wei"].as_u64(), Some(42));
        assert!(req["wei"].as_str().is_none());
        assert_eq!(req["isUndelegate"], false);
        assert_eq!(req["signatureChainId"], "0x66eee");
        // test_executor's MockTransport defaults to mainnet.
        assert_eq!(req["hyperliquidChain"], "Mainnet");
        // The action `nonce` field is a u64 (matches the post nonce).
        assert!(req["nonce"].as_u64().is_some());
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
