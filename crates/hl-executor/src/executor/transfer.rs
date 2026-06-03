use rust_decimal::Decimal;

use hl_types::{HlActionResponse, HlError};

use super::{validate_eth_address, OrderExecutor, SIGNATURE_CHAIN_ID};

impl OrderExecutor {
    /// Deposit USDC into or withdraw USDC from a vault.
    ///
    /// `is_deposit = true` deposits into the vault; `false` withdraws from it.
    /// The `amount` is in USDC and is converted to an integer in micro-units
    /// (6 decimals) on the wire — e.g. `50` becomes `50_000_000` — matching the
    /// `vaultTransfer` action's `usd` field, which Hyperliquid expects as an
    /// integer, not a string.
    #[tracing::instrument(skip(self), fields(vault, is_deposit, amount = %amount))]
    pub async fn vault_transfer(
        &self,
        vault: &str,
        is_deposit: bool,
        amount: Decimal,
    ) -> Result<HlActionResponse, HlError> {
        validate_eth_address(vault)?;
        if amount <= Decimal::ZERO {
            return Err(HlError::Validation(
                "vault_transfer amount must be positive".into(),
            ));
        }
        // `usd` is an integer in micro-units (6 decimals): 50 USD -> 50_000_000.
        let micro = (amount * Decimal::from(1_000_000)).trunc();
        let micro_u64: u64 = micro.to_string().parse().map_err(|e| {
            HlError::Validation(format!(
                "vault_transfer: amount {} converts to invalid micro-units: {e}",
                amount
            ))
        })?;
        let action = serde_json::json!({
            "type": "vaultTransfer",
            "vaultAddress": vault,
            "isDeposit": is_deposit,
            "usd": micro_u64,
        });
        let resp = self.send_signed_action(action, None).await?;
        serde_json::from_value(resp)
            .map_err(|e| HlError::Parse(format!("vault_transfer response: {e}")))
    }

    /// Deposit USDC into a vault. Convenience wrapper over [`Self::vault_transfer`].
    #[tracing::instrument(skip(self), fields(vault, amount = %amount))]
    pub async fn deposit_to_vault(
        &self,
        vault: &str,
        amount: Decimal,
    ) -> Result<HlActionResponse, HlError> {
        self.vault_transfer(vault, true, amount).await
    }

    /// Withdraw USDC from a vault. Convenience wrapper over [`Self::vault_transfer`].
    #[tracing::instrument(skip(self), fields(vault, amount = %amount))]
    pub async fn withdraw_from_vault(
        &self,
        vault: &str,
        amount: Decimal,
    ) -> Result<HlActionResponse, HlError> {
        self.vault_transfer(vault, false, amount).await
    }

    /// Transfer USDC into a vault.
    ///
    /// Backward-compatible alias for [`Self::deposit_to_vault`]. Prefer
    /// [`Self::vault_transfer`] / [`Self::withdraw_from_vault`] for explicit
    /// direction.
    #[tracing::instrument(skip(self), fields(vault, amount = %amount))]
    pub async fn transfer_to_vault(
        &self,
        vault: &str,
        amount: Decimal,
    ) -> Result<HlActionResponse, HlError> {
        self.vault_transfer(vault, true, amount).await
    }

    /// Send USDC to another address on the Hyperliquid L1.
    ///
    /// Uses EIP-712 user-signed-action signing (not L1 action signing).
    #[tracing::instrument(skip(self))]
    pub async fn usdc_transfer(
        &self,
        destination: &str,
        amount: Decimal,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        validate_eth_address(destination)?;
        let chain = self.chain_name();
        let nonce = self.next_nonce();
        let action = serde_json::json!({
            "type": "usdSend",
            "hyperliquidChain": chain,
            "signatureChainId": SIGNATURE_CHAIN_ID,
            "destination": destination,
            "amount": amount.to_string(),
            "time": nonce,
        });

        let types = vec![
            hl_signing::EIP712Field::new("hyperliquidChain", "string"),
            hl_signing::EIP712Field::new("destination", "string"),
            hl_signing::EIP712Field::new("amount", "string"),
            hl_signing::EIP712Field::new("time", "uint64"),
        ];

        let signature = hl_signing::sign_user_signed_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            &types,
            "HyperliquidTransaction:UsdSend",
            self.client.is_mainnet(),
        )?;

        let result = self
            .client
            .post_action(action, &signature, nonce, vault)
            .await?;

        Self::check_and_parse_response(result, "usdSend")
    }

    /// Withdraw USDC from Hyperliquid to an EVM address.
    ///
    /// Uses EIP-712 user-signed-action signing (Withdraw3).
    #[tracing::instrument(skip(self))]
    pub async fn withdraw(
        &self,
        destination: &str,
        amount: Decimal,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        validate_eth_address(destination)?;
        let chain = self.chain_name();
        let nonce = self.next_nonce();
        let action = serde_json::json!({
            "type": "withdraw3",
            "hyperliquidChain": chain,
            "signatureChainId": SIGNATURE_CHAIN_ID,
            "destination": destination,
            "amount": amount.to_string(),
            "time": nonce,
        });

        let types = vec![
            hl_signing::EIP712Field::new("hyperliquidChain", "string"),
            hl_signing::EIP712Field::new("destination", "string"),
            hl_signing::EIP712Field::new("amount", "string"),
            hl_signing::EIP712Field::new("time", "uint64"),
        ];

        let signature = hl_signing::sign_user_signed_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            &types,
            "HyperliquidTransaction:Withdraw",
            self.client.is_mainnet(),
        )?;

        let result = self
            .client
            .post_action(action, &signature, nonce, vault)
            .await?;

        Self::check_and_parse_response(result, "withdraw3")
    }

    /// Send spot tokens to another address on the Hyperliquid L1.
    ///
    /// The `token` parameter uses the format `"<name>:<id>"` (e.g. `"PURR:0x..."`)
    /// as required by the Hyperliquid exchange.
    ///
    /// Uses EIP-712 user-signed-action signing (not L1 action signing).
    #[tracing::instrument(skip(self))]
    pub async fn spot_send(
        &self,
        destination: &str,
        token: &str,
        amount: Decimal,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        validate_eth_address(destination)?;
        let chain = self.chain_name();
        let nonce = self.next_nonce();
        let action = serde_json::json!({
            "type": "spotSend",
            "hyperliquidChain": chain,
            "signatureChainId": SIGNATURE_CHAIN_ID,
            "destination": destination,
            "token": token,
            "amount": amount.to_string(),
            "time": nonce,
        });

        let types = vec![
            hl_signing::EIP712Field::new("hyperliquidChain", "string"),
            hl_signing::EIP712Field::new("destination", "string"),
            hl_signing::EIP712Field::new("token", "string"),
            hl_signing::EIP712Field::new("amount", "string"),
            hl_signing::EIP712Field::new("time", "uint64"),
        ];

        let signature = hl_signing::sign_user_signed_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            &types,
            "HyperliquidTransaction:SpotSend",
            self.client.is_mainnet(),
        )?;

        let result = self
            .client
            .post_action(action, &signature, nonce, vault)
            .await?;

        Self::check_and_parse_response(result, "spotSend")
    }

    /// Send assets cross-chain with DEX routing.
    ///
    /// Unlike `spot_send` (L1-only) and `usdc_transfer` (USDC-only), this action
    /// routes assets across chains via the Hyperliquid bridge.
    ///
    /// Uses EIP-712 user-signed-action signing.
    #[tracing::instrument(skip(self))]
    pub async fn send_asset(
        &self,
        destination: &str,
        asset: &str,
        amount: Decimal,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        validate_eth_address(destination)?;
        let chain = self.chain_name();
        let nonce = self.next_nonce();
        let action = serde_json::json!({
            "type": "sendAsset",
            "hyperliquidChain": chain,
            "signatureChainId": SIGNATURE_CHAIN_ID,
            "destination": destination,
            "asset": asset,
            "amount": amount.to_string(),
            "time": nonce,
        });

        let types = vec![
            hl_signing::EIP712Field::new("hyperliquidChain", "string"),
            hl_signing::EIP712Field::new("destination", "string"),
            hl_signing::EIP712Field::new("asset", "string"),
            hl_signing::EIP712Field::new("amount", "string"),
            hl_signing::EIP712Field::new("time", "uint64"),
        ];

        let signature = hl_signing::sign_user_signed_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            &types,
            "HyperliquidTransaction:SendAsset",
            self.client.is_mainnet(),
        )?;

        let result = self
            .client
            .post_action(action, &signature, nonce, vault)
            .await?;

        Self::check_and_parse_response(result, "sendAsset")
    }

    /// Transfer funds between spot and perp accounts.
    ///
    /// When `to_perp` is `true`, funds move from spot to perp.
    /// When `to_perp` is `false`, funds move from perp to spot.
    /// The `amount` is in USDC (will be converted to micro-units internally).
    #[tracing::instrument(skip(self))]
    pub async fn class_transfer(
        &self,
        amount: Decimal,
        to_perp: bool,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        if amount <= Decimal::ZERO {
            return Err(HlError::Validation(
                "class_transfer amount must be positive".into(),
            ));
        }
        // Truncate to 6 decimal places (micro-units), then convert to integer
        let micro = (amount * Decimal::from(1_000_000)).trunc();
        let micro_u64: u64 = micro.to_string().parse().map_err(|e| {
            HlError::Validation(format!(
                "class_transfer: amount {} converts to invalid micro-units: {e}",
                amount
            ))
        })?;
        let action = serde_json::json!({
            "type": "spotUser",
            "classTransfer": {
                "usdc": micro_u64,
                "toPerp": to_perp,
            },
        });
        let resp = self.send_signed_action(action, vault).await?;
        serde_json::from_value(resp)
            .map_err(|e| HlError::Parse(format!("class_transfer response: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use hl_test_utils::{ok_response, test_executor, test_executor_capturing};

    #[tokio::test]
    async fn usdc_transfer_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .usdc_transfer(
                "0x0000000000000000000000000000000000000002",
                Decimal::from(100),
                None,
            )
            .await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn withdraw_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .withdraw(
                "0x0000000000000000000000000000000000000002",
                Decimal::from(50),
                None,
            )
            .await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn spot_send_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .spot_send(
                "0x0000000000000000000000000000000000000002",
                "PURR:0xabcdef",
                Decimal::from(10),
                None,
            )
            .await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn class_transfer_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .class_transfer(Decimal::from(100), true, None)
            .await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn class_transfer_rejects_zero_amount() {
        let executor = test_executor(vec![]);
        let result = executor.class_transfer(Decimal::ZERO, true, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn class_transfer_rejects_negative_amount() {
        let executor = test_executor(vec![]);
        let result = executor.class_transfer(Decimal::from(-5), true, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn send_asset_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .send_asset(
                "0x0000000000000000000000000000000000000002",
                "BTC",
                Decimal::from(1),
                None,
            )
            .await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn send_asset_rejects_invalid_address() {
        let executor = test_executor(vec![]);
        let result = executor
            .send_asset("not-an-address", "BTC", Decimal::from(1), None)
            .await;
        assert!(matches!(result, Err(HlError::InvalidAddress(_))));
    }

    #[tokio::test]
    async fn vault_transfer_deposit_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .vault_transfer(
                "0x0000000000000000000000000000000000000002",
                true,
                Decimal::from(50),
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, "ok");
    }

    #[tokio::test]
    async fn vault_transfer_withdraw_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .vault_transfer(
                "0x0000000000000000000000000000000000000002",
                false,
                Decimal::from(50),
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, "ok");
    }

    #[tokio::test]
    async fn withdraw_from_vault_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .withdraw_from_vault(
                "0x0000000000000000000000000000000000000002",
                Decimal::from(10),
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn transfer_to_vault_still_deposits() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .transfer_to_vault(
                "0x0000000000000000000000000000000000000002",
                Decimal::from(10),
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn vault_transfer_rejects_invalid_address() {
        let executor = test_executor(vec![]);
        let result = executor
            .vault_transfer("not-an-address", true, Decimal::from(5))
            .await;
        assert!(matches!(result, Err(HlError::InvalidAddress(_))));
    }

    #[tokio::test]
    async fn vault_transfer_rejects_zero_amount() {
        let executor = test_executor(vec![]);
        let result = executor
            .vault_transfer(
                "0x0000000000000000000000000000000000000002",
                true,
                Decimal::ZERO,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn vault_transfer_wire_format() {
        let (executor, transport) = test_executor_capturing(vec![ok_response()]);
        executor
            .vault_transfer(
                "0x0000000000000000000000000000000000000002",
                false,
                Decimal::from(50),
            )
            .await
            .unwrap();
        let req = transport.last_request().unwrap();
        assert_eq!(req["type"], "vaultTransfer");
        assert_eq!(
            req["vaultAddress"],
            "0x0000000000000000000000000000000000000002"
        );
        assert_eq!(req["isDeposit"], false);
        // `usd` MUST be a JSON integer in micro-units (50 USD => 50_000_000),
        // NOT a string — this is the bug the vault fix addressed.
        assert_eq!(req["usd"].as_u64(), Some(50_000_000));
        assert!(req["usd"].as_str().is_none());
    }
}
