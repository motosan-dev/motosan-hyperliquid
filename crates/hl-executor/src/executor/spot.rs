use std::str::FromStr;

use rust_decimal::Decimal;

use hl_types::{HlActionResponse, HlError, OrderResponse, OrderWire, Tif};

use super::orders::{determine_status, order_to_json};
use super::response::{parse_bulk_order_response_with_fallbacks, parse_order_response};
use super::OrderExecutor;

impl OrderExecutor {
    /// Place a spot limit order.
    ///
    /// The `OrderWire` must have its `asset` field set to a **spot token index**
    /// (from [`AssetMetaCache::spot_asset_index`]). Use
    /// [`spot_market_open`](Self::spot_market_open) for market orders.
    #[tracing::instrument(skip(self, order), fields(asset = order.asset, is_buy = order.is_buy))]
    pub async fn place_spot_order(
        &self,
        order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let fallback_price = Decimal::from_str(&order.limit_px).unwrap_or(Decimal::ZERO);
        let fallback_size = Decimal::from_str(&order.sz).unwrap_or(Decimal::ZERO);

        let order_json = order_to_json(&order)?;
        let action = serde_json::json!({
            "type": "order",
            "orders": [order_json],
            "grouping": "na"
        });

        let result = self.send_signed_action(action, vault).await?;
        let (order_id, fill_price, fill_size) =
            parse_order_response(&result, fallback_price, fallback_size)?;
        let status = determine_status(fill_size, fallback_size, &order_id);

        Ok(OrderResponse::new(
            order_id,
            if fill_size > Decimal::ZERO {
                Some(fill_price)
            } else {
                None
            },
            fill_size,
            fallback_size,
            status,
        ))
    }

    /// Place multiple spot orders atomically.
    #[tracing::instrument(skip(self, orders), fields(count = orders.len()))]
    pub async fn bulk_spot_order(
        &self,
        orders: Vec<OrderWire>,
        vault: Option<&str>,
    ) -> Result<Vec<OrderResponse>, HlError> {
        if orders.is_empty() {
            return Ok(vec![]);
        }

        let mut order_jsons = Vec::with_capacity(orders.len());
        let mut fallbacks: Vec<(Decimal, Decimal)> = Vec::with_capacity(orders.len());

        for order in &orders {
            order_jsons.push(order_to_json(order)?);
            fallbacks.push((
                Decimal::from_str(&order.limit_px).unwrap_or(Decimal::ZERO),
                Decimal::from_str(&order.sz).unwrap_or(Decimal::ZERO),
            ));
        }

        let action = serde_json::json!({
            "type": "order",
            "orders": order_jsons,
            "grouping": "na"
        });

        let result = self.send_signed_action(action, vault).await?;
        let parsed = parse_bulk_order_response_with_fallbacks(&result, &fallbacks)?;

        let mut responses = Vec::with_capacity(parsed.len());
        for (i, (order_id, fill_price, fill_size)) in parsed.into_iter().enumerate() {
            let (_, fallback_size) = fallbacks
                .get(i)
                .copied()
                .unwrap_or((Decimal::ZERO, Decimal::ZERO));
            let status = determine_status(fill_size, fallback_size, &order_id);
            responses.push(OrderResponse::new(
                order_id,
                if fill_size > Decimal::ZERO {
                    Some(fill_price)
                } else {
                    None
                },
                fill_size,
                fallback_size,
                status,
            ));
        }

        Ok(responses)
    }

    /// Market buy or sell a spot token with slippage tolerance.
    ///
    /// Fetches the current mid-price from the L2 orderbook and places an IOC
    /// limit order adjusted by `slippage`. Default slippage is 5%.
    #[tracing::instrument(skip(self))]
    pub async fn spot_market_open(
        &self,
        symbol: &str,
        is_buy: bool,
        size: Decimal,
        slippage: Option<Decimal>,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let asset_idx = self.resolve_spot_asset(symbol)?;
        let coin = super::normalize_symbol(symbol);

        let resp = self
            .client
            .post_info(serde_json::json!({"type": "l2Book", "coin": coin}))
            .await?;
        let mid = hl_types::parse_mid_price_from_l2book(&resp)?;

        let slippage = slippage.unwrap_or(Decimal::new(5, 2)); // 5%
        let limit_price = if is_buy {
            mid * (Decimal::ONE + slippage)
        } else {
            mid * (Decimal::ONE - slippage)
        };

        let order = if is_buy {
            OrderWire::limit_buy(asset_idx, limit_price, size)
        } else {
            OrderWire::limit_sell(asset_idx, limit_price, size)
        };
        let order = order.tif(Tif::Ioc).build()?;

        self.place_spot_order(order, vault).await
    }

    /// Cancel a spot order by exchange order ID.
    #[tracing::instrument(skip(self))]
    pub async fn cancel_spot_order(
        &self,
        asset_idx: u32,
        oid: u64,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({
            "type": "cancel",
            "cancels": [{"a": asset_idx, "o": oid}]
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("cancel_spot_order response: {e}")))
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
    }
}
