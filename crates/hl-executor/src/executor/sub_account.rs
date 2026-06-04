use rust_decimal::Decimal;

use hl_types::{HlActionResponse, HlError};

use super::{validate_eth_address, OrderExecutor};

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
        validate_eth_address(sub_account_user)?;
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
        validate_eth_address(sub_account_user)?;
        if amount <= Decimal::ZERO {
            return Err(HlError::Validation(
                "sub_account_transfer amount must be positive".into(),
            ));
        }

        // Truncate to 6 decimal places (micro-units), then convert to integer
        let micro = (amount * Decimal::from(1_000_000)).trunc();
        let micro_u64: u64 = micro.to_string().parse().map_err(|e| {
            HlError::Validation(format!(
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
            .post_action(action, &signature, nonce, vault, None)
            .await?;

        Self::check_and_parse_response(result, "subAccountTransfer")
    }

    /// Transfer **spot tokens** between the master account and a sub-account.
    ///
    /// `is_deposit = true` moves tokens master → sub-account; `false` is the
    /// reverse. `token` is the `"name:id"` token identifier (e.g. `"PURR:0x…"`),
    /// passed through verbatim — not a perp coin symbol. `amount` is a decimal
    /// quantity. This is an **L1-signed** action (unlike the USD-only
    /// [`Self::sub_account_transfer`], which is EIP-712 user-signed).
    #[tracing::instrument(skip(self))]
    pub async fn sub_account_spot_transfer(
        &self,
        sub_account_user: &str,
        is_deposit: bool,
        token: &str,
        amount: Decimal,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        validate_eth_address(sub_account_user)?;
        if amount <= Decimal::ZERO {
            return Err(HlError::Validation(
                "sub_account_spot_transfer amount must be positive".into(),
            ));
        }
        let action = serde_json::json!({
            "type": "subAccountSpotTransfer",
            "subAccountUser": sub_account_user,
            "isDeposit": is_deposit,
            "token": token,
            "amount": amount.to_string(),
        });
        let resp = self.send_signed_action(action, vault).await?;
        serde_json::from_value(resp)
            .map_err(|e| HlError::Parse(format!("sub_account_spot_transfer response: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use hl_test_utils::{ok_response, test_executor};

    #[tokio::test]
    async fn create_sub_account_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor.create_sub_account("trading-sub", None).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn sub_account_transfer_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .sub_account_transfer(
                "0x0000000000000000000000000000000000000005",
                true,
                Decimal::from(500),
                None,
            )
            .await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn sub_account_transfer_rejects_zero_amount() {
        let executor = test_executor(vec![]);
        let result = executor
            .sub_account_transfer(
                "0x0000000000000000000000000000000000000005",
                true,
                Decimal::ZERO,
                None,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn sub_account_transfer_rejects_negative_amount() {
        let executor = test_executor(vec![]);
        let result = executor
            .sub_account_transfer(
                "0x0000000000000000000000000000000000000005",
                false,
                Decimal::from(-10),
                None,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn sub_account_spot_transfer_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .sub_account_spot_transfer(
                "0x0000000000000000000000000000000000000005",
                true,
                "PURR:0xc1fb593aeffbeb02f85e0b7c0a3d8a2f55f4f9d4",
                Decimal::from(10),
                None,
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, "ok");
    }

    #[tokio::test]
    async fn sub_account_spot_transfer_rejects_zero_amount() {
        let executor = test_executor(vec![]);
        let result = executor
            .sub_account_spot_transfer(
                "0x0000000000000000000000000000000000000005",
                true,
                "PURR:0xc1fb593aeffbeb02f85e0b7c0a3d8a2f55f4f9d4",
                Decimal::ZERO,
                None,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn sub_account_spot_transfer_rejects_invalid_address() {
        let executor = test_executor(vec![]);
        let result = executor
            .sub_account_spot_transfer("not-an-address", true, "PURR:0x", Decimal::from(1), None)
            .await;
        assert!(matches!(result, Err(HlError::InvalidAddress(_))));
    }

    #[tokio::test]
    async fn sub_account_spot_transfer_wire_format() {
        let (executor, transport) = hl_test_utils::test_executor_capturing(vec![ok_response()]);
        executor
            .sub_account_spot_transfer(
                "0x0000000000000000000000000000000000000005",
                false,
                "PURR:0xabc",
                Decimal::from(10),
                None,
            )
            .await
            .unwrap();
        let req = transport.last_request().unwrap();
        assert_eq!(req["type"], "subAccountSpotTransfer");
        assert_eq!(
            req["subAccountUser"],
            "0x0000000000000000000000000000000000000005"
        );
        assert_eq!(req["isDeposit"], false);
        assert_eq!(req["token"], "PURR:0xabc");
        // amount is a decimal STRING, not a number; no `time` field (L1 action).
        assert_eq!(req["amount"], "10");
        assert!(req["amount"].is_string());
        assert!(req.get("time").is_none());
    }
}
