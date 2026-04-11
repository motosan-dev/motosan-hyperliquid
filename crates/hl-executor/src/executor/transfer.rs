use rust_decimal::Decimal;

use hl_types::{HlActionResponse, HlError};

use super::OrderExecutor;

impl OrderExecutor {
    /// Transfer USDC into a vault.
    #[tracing::instrument(skip(self), fields(vault, amount = %amount))]
    pub async fn transfer_to_vault(
        &self,
        vault: &str,
        amount: Decimal,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({
            "type": "vaultTransfer",
            "vaultAddress": vault,
            "isDeposit": true,
            "usd": amount.to_string(),
        });
        let resp = self.send_signed_action(action, None).await?;
        serde_json::from_value(resp)
            .map_err(|e| HlError::Parse(format!("transfer_to_vault response: {e}")))
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
        let chain = self.chain_name();
        let nonce = self.next_nonce();
        let action = serde_json::json!({
            "type": "usdSend",
            "hyperliquidChain": chain,
            "signatureChainId": "0xa4b1",
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
        let chain = self.chain_name();
        let nonce = self.next_nonce();
        let action = serde_json::json!({
            "type": "withdraw3",
            "hyperliquidChain": chain,
            "signatureChainId": "0xa4b1",
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
        let chain = self.chain_name();
        let nonce = self.next_nonce();
        let action = serde_json::json!({
            "type": "spotSend",
            "hyperliquidChain": chain,
            "signatureChainId": "0xa4b1",
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
            return Err(HlError::Parse(
                "class_transfer amount must be positive".into(),
            ));
        }
        // Truncate to 6 decimal places (micro-units), then convert to integer
        let micro = (amount * Decimal::from(1_000_000)).trunc();
        let micro_u64: u64 = micro.to_string().parse().map_err(|e| {
            HlError::Parse(format!(
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

    use hl_test_utils::{ok_response, test_executor};

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
}
