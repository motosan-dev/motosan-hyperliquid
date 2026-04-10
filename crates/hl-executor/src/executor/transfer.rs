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
        let chain = if self.client.is_mainnet() {
            "Mainnet"
        } else {
            "Testnet"
        };
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

        let api_status = result
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");
        if api_status != "ok" {
            return Err(HlError::Rejected {
                reason: format!("Exchange rejected usdSend: {}", result),
            });
        }

        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("usdc_transfer response: {e}")))
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
        let chain = if self.client.is_mainnet() {
            "Mainnet"
        } else {
            "Testnet"
        };
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

        let api_status = result
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");
        if api_status != "ok" {
            return Err(HlError::Rejected {
                reason: format!("Exchange rejected withdraw3: {}", result),
            });
        }

        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("withdraw response: {e}")))
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
        let chain = if self.client.is_mainnet() {
            "Mainnet"
        } else {
            "Testnet"
        };
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

        let api_status = result
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");
        if api_status != "ok" {
            return Err(HlError::Rejected {
                reason: format!("Exchange rejected spotSend: {}", result),
            });
        }

        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("spot_send response: {e}")))
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
