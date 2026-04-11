use rust_decimal::Decimal;

use hl_types::{HlActionResponse, HlError, OrderResponse, OrderWire, Side, Tif};

use super::orders::extract_mid_price;
use super::OrderExecutor;

impl OrderExecutor {
    /// Place a spot limit order.
    ///
    /// The `OrderWire` must have its `asset` field set to a **spot token index**
    /// (from [`AssetMetaCache::spot_asset_index`]). The wire format is identical
    /// to perp orders — only the asset index differs.
    ///
    /// This is a convenience wrapper around [`place_order`](Self::place_order).
    #[tracing::instrument(skip(self, order), fields(asset = order.asset, is_buy = order.is_buy))]
    pub async fn place_spot_order(
        &self,
        order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        self.place_order(order, vault).await
    }

    /// Place multiple spot orders atomically.
    ///
    /// Convenience wrapper around [`bulk_order`](Self::bulk_order).
    #[tracing::instrument(skip(self, orders), fields(count = orders.len()))]
    pub async fn bulk_spot_order(
        &self,
        orders: Vec<OrderWire>,
        vault: Option<&str>,
    ) -> Result<Vec<OrderResponse>, HlError> {
        self.bulk_order(orders, vault).await
    }

    /// Market buy or sell a spot token with slippage tolerance.
    ///
    /// Fetches the current mid-price from the L2 orderbook and places an IOC
    /// limit order adjusted by `slippage`. Default slippage is 5%.
    #[tracing::instrument(skip(self))]
    pub async fn spot_market_open(
        &self,
        symbol: &str,
        side: Side,
        size: Decimal,
        slippage: Option<Decimal>,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let asset_idx = self.resolve_spot_asset(symbol)?;
        let coin = super::normalize_symbol(symbol);
        let mid = extract_mid_price(&self.client, &coin).await?;

        let slippage = slippage.unwrap_or_else(|| Decimal::new(5, 2));
        let limit_price = if side.is_buy() {
            mid * (Decimal::ONE + slippage)
        } else {
            mid * (Decimal::ONE - slippage)
        };

        let order = if side.is_buy() {
            OrderWire::limit_buy(asset_idx, limit_price, size)
        } else {
            OrderWire::limit_sell(asset_idx, limit_price, size)
        }
        .tif(Tif::Ioc)
        .build()?;

        self.place_order(order, vault).await
    }

    /// Cancel a spot order by exchange order ID.
    ///
    /// Convenience wrapper around [`cancel_order`](Self::cancel_order).
    #[tracing::instrument(skip(self))]
    pub async fn cancel_spot_order(
        &self,
        asset_idx: u32,
        oid: u64,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        self.cancel_order(asset_idx, oid, vault).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta_cache::AssetMetaCache;
    use std::sync::Arc;

    fn ok_resting_response(oid: u64) -> serde_json::Value {
        serde_json::json!({
            "status": "ok",
            "response": {
                "type": "order",
                "data": {
                    "statuses": [{"resting": {"oid": oid}}]
                }
            }
        })
    }

    fn test_executor_with_spot(responses: Vec<serde_json::Value>) -> OrderExecutor {
        let mut perp_idx = std::collections::HashMap::new();
        perp_idx.insert("BTC".to_string(), 0u32);
        let mut spot_idx = std::collections::HashMap::new();
        spot_idx.insert("PURR".to_string(), 10000u32);
        spot_idx.insert("USDC".to_string(), 10001u32);
        let cache = AssetMetaCache::from_maps_with_spot(
            perp_idx,
            Default::default(),
            spot_idx,
            Default::default(),
        );
        OrderExecutor::with_meta_cache(
            Arc::new(hl_test_utils::MockTransport::new(responses)),
            hl_test_utils::test_signer(),
            "0x0000000000000000000000000000000000000001".to_string(),
            cache,
        )
    }

    #[tokio::test]
    async fn place_spot_order_success() {
        let executor = test_executor_with_spot(vec![ok_resting_response(9999)]);
        let order = OrderWire::limit_buy(10000, Decimal::from(1), Decimal::from(100))
            .build()
            .unwrap();
        let result = executor.place_spot_order(order, None).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.order_id, "9999");
    }

    #[tokio::test]
    async fn bulk_spot_order_empty() {
        let executor = test_executor_with_spot(vec![]);
        let result = executor.bulk_spot_order(vec![], None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn resolve_spot_asset_unknown_fails() {
        let executor = test_executor_with_spot(vec![]);
        let result = executor.resolve_spot_asset("UNKNOWN_TOKEN");
        assert!(matches!(result, Err(HlError::Validation(_))));
    }

    #[tokio::test]
    async fn cancel_spot_order_success() {
        let executor = test_executor_with_spot(vec![
            serde_json::json!({"status": "ok", "response": {"type": "cancel"}}),
        ]);
        let result = executor.cancel_spot_order(10000, 9999, None).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }
}
