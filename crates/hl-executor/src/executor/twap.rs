use rust_decimal::Decimal;

use hl_types::{HlActionResponse, HlError};

use super::OrderExecutor;

impl OrderExecutor {
    /// Place a TWAP (Time-Weighted Average Price) order that executes over a
    /// specified duration.
    ///
    /// The order is split into slices and executed gradually over `duration_secs`
    /// seconds. When `randomize` is true the execution timing of each slice is
    /// randomised to reduce market impact.
    ///
    /// # Arguments
    ///
    /// * `symbol`        - Market symbol (e.g. `"BTC"`, `"ETH-PERP"`).
    /// * `is_buy`        - `true` for a buy TWAP, `false` for sell.
    /// * `size`          - Total order size.
    /// * `duration_secs` - Duration over which the TWAP executes, in seconds.
    /// * `reduce_only`   - Whether the order should only reduce an existing position.
    /// * `randomize`     - Randomise slice execution timing (recommended `true`).
    /// * `vault`         - Optional vault address.
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip(self))]
    pub async fn place_twap_order(
        &self,
        symbol: &str,
        is_buy: bool,
        size: Decimal,
        duration_secs: u64,
        reduce_only: bool,
        randomize: bool,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let asset = self.resolve_asset(symbol)?;

        let action = serde_json::json!({
            "type": "twapOrder",
            "twap": {
                "a": asset,
                "b": is_buy,
                "s": size.to_string(),
                "r": reduce_only,
                "m": duration_secs,
                "t": randomize,
            }
        });

        let result = self.send_signed_action(action, vault).await?;

        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("place_twap_order response: {e}")))
    }

    /// Cancel a running TWAP order.
    ///
    /// # Arguments
    ///
    /// * `symbol`  - Market symbol for the TWAP (used to resolve the asset index).
    /// * `twap_id` - The exchange-assigned TWAP order ID.
    /// * `vault`   - Optional vault address.
    #[tracing::instrument(skip(self))]
    pub async fn cancel_twap(
        &self,
        symbol: &str,
        twap_id: u64,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let asset = self.resolve_asset(symbol)?;

        let action = serde_json::json!({
            "type": "twapCancel",
            "a": asset,
            "t": twap_id,
        });

        let result = self.send_signed_action(action, vault).await?;

        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("cancel_twap response: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use hl_test_utils::{ok_response, test_executor};

    #[tokio::test]
    async fn place_twap_order_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .place_twap_order("BTC", true, Decimal::from(1), 3600, false, true, None)
            .await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn cancel_twap_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor.cancel_twap("BTC", 42, None).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn place_twap_order_unknown_symbol() {
        let executor = test_executor(vec![]);
        let result = executor
            .place_twap_order(
                "NOSUCHCOIN",
                true,
                Decimal::from(1),
                3600,
                false,
                true,
                None,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cancel_twap_unknown_symbol() {
        let executor = test_executor(vec![]);
        let result = executor.cancel_twap("NOSUCHCOIN", 42, None).await;
        assert!(result.is_err());
    }
}
