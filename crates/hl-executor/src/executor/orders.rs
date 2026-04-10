use std::str::FromStr;

use rust_decimal::Decimal;

use hl_types::{HlError, OrderResponse, OrderStatus, OrderTypeWire, OrderWire, Side, Tif, Tpsl};

use super::response::{parse_bulk_order_response_with_fallbacks, parse_order_response};
use super::{OrderExecutor, FILL_THRESHOLD};

/// Build wire-format JSON from an [`OrderWire`].
pub(crate) fn order_to_json(order: &OrderWire) -> Result<serde_json::Value, HlError> {
    let mut order_json = serde_json::json!({
        "a": order.asset,
        "b": order.is_buy,
        "p": order.limit_px,
        "s": order.sz,
        "r": order.reduce_only,
        "t": {},
    });

    match &order.order_type {
        OrderTypeWire::Limit(limit) => {
            order_json["t"] = serde_json::json!({ "limit": { "tif": limit.tif.to_string() } });
        }
        OrderTypeWire::Trigger(trigger) => {
            order_json["t"] = serde_json::json!({
                "trigger": {
                    "triggerPx": trigger.trigger_px,
                    "isMarket": trigger.is_market,
                    "tpsl": trigger.tpsl.to_string(),
                }
            });
        }
        _ => {
            return Err(HlError::serialization("unknown OrderTypeWire variant"));
        }
    }

    if let Some(ref cloid) = order.cloid {
        order_json["c"] = serde_json::json!(cloid);
    }

    Ok(order_json)
}

/// Determine the order status from fill information.
pub(crate) fn determine_status(
    fill_size: Decimal,
    requested_size: Decimal,
    order_id: &str,
) -> OrderStatus {
    if fill_size >= requested_size * FILL_THRESHOLD {
        OrderStatus::Filled
    } else if fill_size > Decimal::ZERO {
        tracing::warn!(
            order_id = %order_id,
            filled = %fill_size,
            requested = %requested_size,
            "Partial fill detected"
        );
        OrderStatus::Partial
    } else {
        OrderStatus::Open
    }
}

impl OrderExecutor {
    /// Place an order on the Hyperliquid L1.
    ///
    /// The `OrderWire` must already have the asset index, price, size, order
    /// type, etc. fully populated. This method constructs the action JSON,
    /// signs it, submits it, and parses the response.
    #[tracing::instrument(skip(self, order), fields(asset = order.asset, is_buy = order.is_buy))]
    pub async fn place_order(
        &self,
        order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let fallback_price: Decimal = Decimal::from_str(&order.limit_px).unwrap_or(Decimal::ZERO);
        let fallback_size: Decimal = Decimal::from_str(&order.sz).unwrap_or(Decimal::ZERO);

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

    /// Place a trigger order (stop-loss or take-profit) on Hyperliquid.
    ///
    /// `side` indicates the order direction (opposite of position side).
    /// `tpsl` indicates whether this is a stop-loss or take-profit trigger.
    /// The order fires as a market order when the trigger price is hit.
    #[tracing::instrument(skip(self))]
    pub async fn place_trigger_order(
        &self,
        symbol: &str,
        side: Side,
        size: Decimal,
        trigger_price: Decimal,
        tpsl: Tpsl,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let asset_idx = self.resolve_asset(symbol)?;

        let is_buy = side.is_buy();
        let cloid = uuid::Uuid::new_v4().to_string();

        let action = serde_json::json!({
            "type": "order",
            "orders": [{
                "a": asset_idx,
                "b": is_buy,
                "p": trigger_price.to_string(),
                "s": size.to_string(),
                "r": true,
                "t": {
                    "trigger": {
                        "triggerPx": trigger_price.to_string(),
                        "isMarket": true,
                        "tpsl": tpsl.to_string()
                    }
                },
                "c": cloid
            }],
            "grouping": "na"
        });

        tracing::debug!(
            symbol = %symbol,
            side = %side,
            size = %size,
            tpsl = %tpsl,
            "Submitting trigger order"
        );

        let result = self.send_signed_action(action, vault).await?;

        let (order_id, fill_price, fill_size) = parse_order_response(&result, trigger_price, size)?;

        // Trigger orders typically rest unfilled until the trigger fires
        let status = if fill_size < size * FILL_THRESHOLD && fill_size > Decimal::ZERO {
            tracing::warn!(
                order_id = %order_id,
                filled = %fill_size,
                requested = %size,
                "Partial fill detected on trigger order"
            );
            OrderStatus::Partial
        } else if fill_size == Decimal::ZERO {
            OrderStatus::Open
        } else {
            match tpsl {
                Tpsl::Sl => OrderStatus::TriggerSl,
                Tpsl::Tp => OrderStatus::TriggerTp,
            }
        };

        Ok(OrderResponse::new(
            order_id,
            if fill_size > Decimal::ZERO {
                Some(fill_price)
            } else {
                None
            },
            fill_size,
            size,
            status,
        ))
    }

    /// Place multiple orders in a single signed action.
    #[tracing::instrument(skip(self, orders), fields(count = orders.len()))]
    pub async fn bulk_order(
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

    /// Like `place_order` but resolves the asset index from a symbol string.
    #[tracing::instrument(skip(self, order))]
    pub async fn place_order_by_symbol(
        &self,
        symbol: &str,
        mut order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        order.asset = self.resolve_asset(symbol)?;
        self.place_order(order, vault).await
    }

    /// Place a market order (IOC limit at a slippage-adjusted price).
    ///
    /// Market orders on Hyperliquid are implemented as IOC (immediate-or-cancel)
    /// limit orders at a price that accounts for slippage. The mid-price is
    /// fetched from the L2 orderbook, then adjusted:
    /// - **Buy**: `mid * (1 + slippage)`
    /// - **Sell**: `mid * (1 - slippage)`
    ///
    /// If `slippage` is `None`, a default of 5% is used.
    #[tracing::instrument(skip(self))]
    pub async fn market_open(
        &self,
        symbol: &str,
        side: Side,
        size: Decimal,
        slippage: Option<Decimal>,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let asset_idx = self.resolve_asset(symbol)?;
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

    /// Close an open position with a market order.
    ///
    /// If `size` is `None`, the current position size is queried from the
    /// exchange via `clearinghouseState`. The close side is determined from
    /// the position sign (long → sell, short → buy).
    #[tracing::instrument(skip(self))]
    pub async fn market_close(
        &self,
        symbol: &str,
        size: Option<Decimal>,
        slippage: Option<Decimal>,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let coin = super::normalize_symbol(symbol);

        let (close_side, close_size) = match size {
            Some(sz) => {
                // Caller must indicate direction via sign: positive = close long (sell),
                // negative = close short (buy).
                if sz > Decimal::ZERO {
                    (Side::Sell, sz)
                } else if sz < Decimal::ZERO {
                    (Side::Buy, sz.abs())
                } else {
                    return Err(HlError::Parse("market_close: size must not be zero".into()));
                }
            }
            None => {
                // Query current position from exchange
                let resp = self
                    .client
                    .post_info(serde_json::json!({
                        "type": "clearinghouseState",
                        "user": self.address,
                    }))
                    .await?;

                let (szi_side, szi_size) = extract_position_szi(&resp, &coin)?;
                let close_side = if szi_side.is_buy() {
                    // Position is long → sell to close
                    Side::Sell
                } else {
                    // Position is short → buy to close
                    Side::Buy
                };
                (close_side, szi_size)
            }
        };

        let asset_idx = self.resolve_asset(symbol)?;
        let mid = extract_mid_price(&self.client, &coin).await?;

        let slippage = slippage.unwrap_or_else(|| Decimal::new(5, 2));
        let limit_price = if close_side.is_buy() {
            mid * (Decimal::ONE + slippage)
        } else {
            mid * (Decimal::ONE - slippage)
        };

        let order = if close_side.is_buy() {
            OrderWire::limit_buy(asset_idx, limit_price, close_size)
        } else {
            OrderWire::limit_sell(asset_idx, limit_price, close_size)
        }
        .tif(Tif::Ioc)
        .reduce_only(true)
        .build()?;

        self.place_order(order, vault).await
    }
}

/// Fetch the mid-price from the L2 orderbook.
///
/// Queries `l2Book` for the given coin and delegates parsing to
/// [`hl_types::parse_mid_price_from_l2book`].
async fn extract_mid_price(
    client: &std::sync::Arc<dyn hl_client::HttpTransport>,
    coin: &str,
) -> Result<Decimal, HlError> {
    let resp = client
        .post_info(serde_json::json!({
            "type": "l2Book",
            "coin": coin,
        }))
        .await?;

    hl_types::parse_mid_price_from_l2book(&resp)
}

/// Extract a position's size and side from a `clearinghouseState` response.
///
/// Delegates to [`hl_types::parse_position_szi`].
fn extract_position_szi(resp: &serde_json::Value, coin: &str) -> Result<(Side, Decimal), HlError> {
    hl_types::parse_position_szi(resp, coin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slippage_buy_increases_price() {
        let mid = Decimal::from(90000);
        let slippage = Decimal::new(5, 2);
        let limit = mid * (Decimal::ONE + slippage);
        assert_eq!(limit, Decimal::from(94500));
    }

    #[test]
    fn slippage_sell_decreases_price() {
        let mid = Decimal::from(90000);
        let slippage = Decimal::new(5, 2);
        let limit = mid * (Decimal::ONE - slippage);
        assert_eq!(limit, Decimal::from(85500));
    }

    // ── Mock-based integration tests ───────────────────────────

    use async_trait::async_trait;
    use hl_client::HttpTransport;
    use hl_signing::PrivateKeySigner;
    use hl_types::Signature;
    use std::sync::{Arc, Mutex};

    /// Mock transport that returns pre-queued responses in FIFO order.
    struct MockTransport {
        responses: Mutex<Vec<serde_json::Value>>,
        is_mainnet: bool,
    }

    impl MockTransport {
        fn new(responses: Vec<serde_json::Value>) -> Self {
            Self {
                responses: Mutex::new(responses),
                is_mainnet: true,
            }
        }
    }

    #[async_trait]
    impl HttpTransport for MockTransport {
        async fn post_info(&self, _req: serde_json::Value) -> Result<serde_json::Value, HlError> {
            let mut q = self.responses.lock().unwrap();
            if q.is_empty() {
                return Err(HlError::http("no mock responses"));
            }
            Ok(q.remove(0))
        }

        async fn post_action(
            &self,
            _action: serde_json::Value,
            _sig: &Signature,
            _nonce: u64,
            _vault: Option<&str>,
        ) -> Result<serde_json::Value, HlError> {
            let mut q = self.responses.lock().unwrap();
            if q.is_empty() {
                return Err(HlError::http("no mock responses"));
            }
            Ok(q.remove(0))
        }

        fn is_mainnet(&self) -> bool {
            self.is_mainnet
        }
    }

    fn test_signer() -> Box<dyn hl_signing::Signer> {
        Box::new(
            PrivateKeySigner::from_hex(
                "0x0000000000000000000000000000000000000000000000000000000000000001",
            )
            .unwrap(),
        )
    }

    fn test_executor(responses: Vec<serde_json::Value>) -> OrderExecutor {
        use crate::meta_cache::AssetMetaCache;
        let mut name_to_idx = std::collections::HashMap::new();
        name_to_idx.insert("BTC".to_string(), 0u32);
        name_to_idx.insert("ETH".to_string(), 1u32);
        let cache = AssetMetaCache::from_maps(name_to_idx, Default::default());
        OrderExecutor::with_meta_cache(
            Arc::new(MockTransport::new(responses)),
            test_signer(),
            "0x0000000000000000000000000000000000000001".to_string(),
            cache,
        )
    }

    /// Canned "ok" response with a single resting order status.
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

    /// Canned "ok" response with a single filled order status.
    fn ok_filled_response(oid: u64, avg_px: &str, total_sz: &str) -> serde_json::Value {
        serde_json::json!({
            "status": "ok",
            "response": {
                "type": "order",
                "data": {
                    "statuses": [{
                        "filled": {
                            "oid": oid,
                            "avgPx": avg_px,
                            "totalSz": total_sz
                        }
                    }]
                }
            }
        })
    }

    #[tokio::test]
    async fn place_order_resting() {
        let executor = test_executor(vec![ok_resting_response(123)]);
        let order = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from(1))
            .build()
            .unwrap();

        let resp = executor.place_order(order, None).await.unwrap();
        assert_eq!(resp.order_id, "123");
        assert_eq!(resp.status, OrderStatus::Open);
        assert_eq!(resp.filled_size, Decimal::ZERO);
        assert!(resp.filled_price.is_none());
    }

    #[tokio::test]
    async fn place_order_filled() {
        let executor = test_executor(vec![ok_filled_response(456, "90100.5", "1.0")]);
        let order = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from(1))
            .build()
            .unwrap();

        let resp = executor.place_order(order, None).await.unwrap();
        assert_eq!(resp.order_id, "456");
        assert_eq!(resp.status, OrderStatus::Filled);
        assert_eq!(
            resp.filled_price,
            Some(Decimal::from_str("90100.5").unwrap())
        );
        assert_eq!(resp.filled_size, Decimal::from_str("1.0").unwrap());
    }

    #[tokio::test]
    async fn cancel_order_success() {
        let canned = serde_json::json!({
            "status": "ok",
            "response": {
                "type": "cancel",
                "data": {
                    "statuses": ["success"]
                }
            }
        });
        let executor = test_executor(vec![canned]);

        let resp = executor.cancel_order(0, 123, None).await.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn bulk_order_multiple() {
        let canned = serde_json::json!({
            "status": "ok",
            "response": {
                "type": "order",
                "data": {
                    "statuses": [
                        {"resting": {"oid": 100}},
                        {"filled": {"oid": 200, "avgPx": "3000.0", "totalSz": "2.0"}}
                    ]
                }
            }
        });
        let executor = test_executor(vec![canned]);

        let order1 = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from(1))
            .build()
            .unwrap();
        let order2 = OrderWire::limit_sell(1, Decimal::from(3000), Decimal::from(2))
            .build()
            .unwrap();

        let resps = executor
            .bulk_order(vec![order1, order2], None)
            .await
            .unwrap();
        assert_eq!(resps.len(), 2);

        assert_eq!(resps[0].order_id, "100");
        assert_eq!(resps[0].status, OrderStatus::Open);
        assert_eq!(resps[0].filled_size, Decimal::ZERO);

        assert_eq!(resps[1].order_id, "200");
        assert_eq!(resps[1].status, OrderStatus::Filled);
        assert_eq!(resps[1].filled_size, Decimal::from_str("2.0").unwrap());
    }

    #[tokio::test]
    async fn market_open_buy() {
        // First response: l2Book (post_info) for mid-price extraction
        let l2book = serde_json::json!({
            "levels": [
                [{"px": "89000.0", "sz": "1.0", "n": 1}],
                [{"px": "91000.0", "sz": "1.0", "n": 1}]
            ]
        });
        // Second response: order action (post_action) -> filled
        let order_resp = ok_filled_response(789, "90500.0", "0.5");

        let executor = test_executor(vec![l2book, order_resp]);

        let resp = executor
            .market_open(
                "BTC",
                Side::Buy,
                Decimal::from_str("0.5").unwrap(),
                None,
                None,
            )
            .await
            .unwrap();

        assert_eq!(resp.order_id, "789");
        assert_eq!(resp.filled_size, Decimal::from_str("0.5").unwrap());
    }

    #[tokio::test]
    async fn place_trigger_order_resting() {
        let executor = test_executor(vec![ok_resting_response(555)]);

        let resp = executor
            .place_trigger_order(
                "BTC",
                Side::Sell,
                Decimal::from(1),
                Decimal::from(85000),
                Tpsl::Sl,
                None,
            )
            .await
            .unwrap();

        assert_eq!(resp.order_id, "555");
        // Trigger orders rest until triggered, so fill_size is 0 -> Open
        assert_eq!(resp.status, OrderStatus::Open);
        assert_eq!(resp.filled_size, Decimal::ZERO);
    }

    #[tokio::test]
    async fn error_response_produces_rejected() {
        let canned = serde_json::json!({
            "status": "err",
            "response": "Insufficient margin"
        });
        let executor = test_executor(vec![canned]);
        let order = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from(1))
            .build()
            .unwrap();

        let result = executor.place_order(order, None).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            HlError::Rejected { reason } => {
                assert!(
                    reason.contains("rejected"),
                    "expected 'rejected' in reason, got: {}",
                    reason
                );
            }
            other => panic!("expected HlError::Rejected, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn bulk_order_empty_returns_empty() {
        // No mock responses needed — empty input short-circuits before any HTTP call
        let executor = test_executor(vec![]);
        let resps = executor.bulk_order(vec![], None).await.unwrap();
        assert!(resps.is_empty());
    }
}
