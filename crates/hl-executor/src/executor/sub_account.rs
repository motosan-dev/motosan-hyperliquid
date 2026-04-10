use rust_decimal::Decimal;

use hl_types::{HlActionResponse, HlError};

use super::OrderExecutor;

impl OrderExecutor {
    /// Create a new sub-account under the master wallet.
    ///
    /// Sub-accounts share the fee tier with the master account.
    /// This is an L1-signed action.
    #[tracing::instrument(skip(self))]
    pub async fn create_sub_account(
        &self,
        name: &str,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({
            "type": "createSubAccount",
            "name": name,
        });
        let resp = self.send_signed_action(action, vault).await?;
        serde_json::from_value(resp)
            .map_err(|e| HlError::Parse(format!("create_sub_account response: {e}")))
    }

    /// Rename an existing sub-account.
    ///
    /// `sub_account_user` is the address of the sub-account to rename.
    /// This is an L1-signed action.
    #[tracing::instrument(skip(self))]
    pub async fn sub_account_modify(
        &self,
        sub_account_user: &str,
        name: &str,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({
            "type": "subAccountModify",
            "subAccountUser": sub_account_user,
            "name": name,
        });
        let resp = self.send_signed_action(action, vault).await?;
        serde_json::from_value(resp)
            .map_err(|e| HlError::Parse(format!("sub_account_modify response: {e}")))
    }

    /// Transfer funds between the master account and a sub-account.
    ///
    /// When `is_deposit` is `true`, funds move from master to sub-account.
    /// When `is_deposit` is `false`, funds move from sub-account to master.
    /// The `amount` is in USDC (will be converted to micro-units internally).
    ///
    /// This is a user-signed EIP-712 action (like `usdc_transfer`).
    #[tracing::instrument(skip(self))]
    pub async fn sub_account_transfer(
        &self,
        sub_account_user: &str,
        is_deposit: bool,
        amount: Decimal,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        if amount <= Decimal::ZERO {
            return Err(HlError::Parse(
                "sub_account_transfer amount must be positive".into(),
            ));
        }

        // Truncate to 6 decimal places (micro-units), then convert to integer
        let micro = (amount * Decimal::from(1_000_000)).trunc();
        let micro_u64: u64 = micro.to_string().parse().map_err(|e| {
            HlError::Parse(format!(
                "sub_account_transfer: amount {} converts to invalid micro-units: {e}",
                amount
            ))
        })?;

        let nonce = self.next_nonce();
        let action = serde_json::json!({
            "type": "subAccountTransfer",
            "subAccountUser": sub_account_user,
            "isDeposit": is_deposit,
            "usd": micro_u64,
            "time": nonce,
        });

        let types = vec![
            hl_signing::EIP712Field::new("subAccountUser", "address"),
            hl_signing::EIP712Field::new("isDeposit", "bool"),
            hl_signing::EIP712Field::new("usd", "uint64"),
        ];

        let signature = hl_signing::sign_user_signed_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            &types,
            "HyperliquidTransaction:SubAccountTransfer",
            self.client.is_mainnet(),
        )?;

        let result = self
            .client
            .post_action(action, &signature, nonce, vault)
            .await?;

        Self::check_and_parse_response(result, "subAccountTransfer")
    }
}
