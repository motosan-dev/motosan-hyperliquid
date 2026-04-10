use rust_decimal::Decimal;

use hl_types::{HlActionResponse, HlError};

use super::OrderExecutor;

impl OrderExecutor {
    /// Update leverage for an asset.
    ///
    /// `is_cross`: `true` for cross-margin, `false` for isolated.
    #[tracing::instrument(skip(self))]
    pub async fn update_leverage(
        &self,
        symbol: &str,
        leverage: u32,
        is_cross: bool,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let asset = self.resolve_asset(symbol)?;
        let action = serde_json::json!({
            "type": "updateLeverage",
            "asset": asset,
            "isCross": is_cross,
            "leverage": leverage
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("update_leverage response: {e}")))
    }

    /// Adjust isolated margin for a position.
    ///
    /// Positive `amount` adds margin, negative removes it. Amount is in USDC.
    #[tracing::instrument(skip(self))]
    pub async fn update_isolated_margin(
        &self,
        symbol: &str,
        amount: Decimal,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let asset = self.resolve_asset(symbol)?;
        let is_buy = amount > Decimal::ZERO;
        // Convert USDC amount to micro-units (multiply by 1_000_000)
        let ntli = (amount.abs() * Decimal::from(1_000_000))
            .to_string()
            .parse::<i64>()
            .map_err(|e| HlError::Parse(format!("margin amount conversion: {e}")))?;

        let action = serde_json::json!({
            "type": "updateIsolatedMargin",
            "asset": asset,
            "isBuy": is_buy,
            "ntli": ntli
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("update_isolated_margin response: {e}")))
    }
}
