use rust_decimal::Decimal;

use hl_types::*;

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
        let resp = self.send_signed_action(action, vault).await?;
        serde_json::from_value(resp)
            .map_err(|e| HlError::Parse(format!("usdc_transfer response: {e}")))
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
        let micro = (amount * Decimal::from(1_000_000))
            .to_string()
            .parse::<u64>()
            .map_err(|e| HlError::Parse(format!("class_transfer: invalid micro amount: {e}")))?;
        let action = serde_json::json!({
            "type": "spotUser",
            "classTransfer": {
                "usdc": micro,
                "toPerp": to_perp,
            },
        });
        let resp = self.send_signed_action(action, vault).await?;
        serde_json::from_value(resp)
            .map_err(|e| HlError::Parse(format!("class_transfer response: {e}")))
    }
}
