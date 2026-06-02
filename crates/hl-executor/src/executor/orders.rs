use std::str::FromStr;

use rust_decimal::{Decimal, RoundingStrategy};

use hl_types::*;

use super::response::{parse_bulk_order_response_with_fallbacks, parse_order_response};
use super::{OrderExecutor, FILL_THRESHOLD};

/// Round a **perp** price to Hyperliquid's rule: at most 5 significant figures
/// AND at most `6 - sz_decimals` decimal places. Integer prices are returned
/// unchanged — Hyperliquid allows them regardless of significant figures.
///
/// The `6` is the perp `MAX_DECIMALS`; spot uses `8 - sz_decimals`. This is safe
/// today because the meta cache only loads the perp universe — parameterize the
/// `6` if/when spot support is added.
pub(crate) fn round_price_perp(px: Decimal, sz_decimals: u32) -> Decimal {
    let max_dp = 6u32.saturating_sub(sz_decimals);
    let sf = if px.fract() == Decimal::ZERO {
        px
    } else {
        px.round_sf(5).unwrap_or(px)
    };
    sf.round_dp(max_dp)
}

/// Round an order size down (toward zero) to the asset's `szDecimals`.
pub(crate) fn round_size(sz: Decimal, sz_decimals: u32) -> Decimal {
    sz.round_dp_with_strategy(sz_decimals, RoundingStrategy::ToZero)
}

/// Generate a Hyperliquid client order id (`0x` + 32 hex chars) for
/// idempotent submission — the exchange dedups retries by cloid.
pub(crate) fn new_cloid() -> String {
    format!("0x{}", uuid::Uuid::new_v4().as_simple())
}

/// Attach a generated cloid if the order has none (idempotent).
pub(crate) fn ensure_cloid(order: &mut OrderWire) {
    if order.cloid.is_none() {
        order.cloid = Some(new_cloid());
    }
}

/// Derive the close side and size for `market_close`. The side is ALWAYS taken
/// from the live position sign (long -> Sell, short -> Buy). `size` is an
/// unsigned magnitude: Some(m) closes m (must be > 0), None closes the full
/// position. Direction is never encoded in the sign of `size`.
pub(crate) fn resolve_close(
    size: Option<Decimal>,
    position_side: Side,
    position_size: Decimal,
) -> Result<(Side, Decimal), HlError> {
    let close_side = if position_side.is_buy() {
        Side::Sell
    } else {
        Side::Buy
    };
    let close_size = match size {
        Some(m) if m > Decimal::ZERO => m,
        Some(_) => {
            return Err(HlError::Parse(
                "market_close: size must be a positive magnitude".into(),
            ))
        }
        None => position_size,
    };
    Ok((close_side, close_size))
}

/// Build wire-format JSON from an [`OrderWire`].
///
/// The `t` (order-type) sub-object is produced by [`OrderTypeWire`]'s own
/// `Serialize` impl, so the limit/trigger wire shape is defined in exactly one
/// place (and no `#[non_exhaustive]` wildcard arm is needed here).
pub(crate) fn order_to_json(order: &OrderWire) -> Result<serde_json::Value, HlError> {
    let order_type = serde_json::to_value(&order.order_type)
        .map_err(|e| HlError::serialization(format!("order type: {e}")))?;

    let mut order_json = serde_json::json!({
        "a": order.asset,
        "b": order.is_buy,
        "p": order.limit_px,
        "s": order.sz,
        "r": order.reduce_only,
        "t": order_type,
    });

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
    ///
    /// Direct limit orders are normalized to canonical wire strings by
    /// [`OrderWire`] builders, but asset-specific grid rounding is the caller's
    /// responsibility.
    #[tracing::instrument(skip(self, order), fields(asset = order.asset, is_buy = order.is_buy))]
    pub async fn place_order(
        &self,
        mut order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        ensure_cloid(&mut order);

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
        let coin = super::normalize_symbol(symbol);
        let sz_decimals = self
            .meta_cache
            .sz_decimals(&coin)
            .ok_or_else(|| HlError::Parse(format!("szDecimals not found for '{}'", coin)))?;
        let trigger_price = round_price_perp(trigger_price, sz_decimals);
        let size = round_size(size, sz_decimals);

        // Route through the builder so price/size > 0 is validated — a sub-lot
        // size that round_size truncated to 0 must be rejected, not silently
        // sent as "0" — and wire serialization stays consistent with place_order.
        let order = if side.is_buy() {
            OrderWire::trigger_buy(asset_idx, trigger_price, size, tpsl)
        } else {
            OrderWire::trigger_sell(asset_idx, trigger_price, size, tpsl)
        }
        .cloid(new_cloid())
        .build()?;

        let action = serde_json::json!({
            "type": "order",
            "orders": [order_to_json(&order)?],
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
        mut orders: Vec<OrderWire>,
        vault: Option<&str>,
    ) -> Result<Vec<OrderResponse>, HlError> {
        if orders.is_empty() {
            return Ok(vec![]);
        }

        for order in &mut orders {
            ensure_cloid(order);
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

        let sz_decimals = self
            .meta_cache
            .sz_decimals(&coin)
            .ok_or_else(|| HlError::Parse(format!("szDecimals not found for '{}'", coin)))?;
        let limit_price = round_price_perp(limit_price, sz_decimals);
        let size = round_size(size, sz_decimals);

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
    /// The live position is always queried via `clearinghouseState`; the close
    /// side is derived from the live position sign (long → sell, short → buy).
    /// If `size` is `Some`, it is an unsigned positive magnitude to close; if
    /// `None`, the full live position size is closed.
    #[tracing::instrument(skip(self))]
    pub async fn market_close(
        &self,
        symbol: &str,
        size: Option<Decimal>,
        slippage: Option<Decimal>,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let coin = super::normalize_symbol(symbol);

        let resp = self
            .client
            .post_info(serde_json::json!({
                "type": "clearinghouseState",
                "user": self.address,
            }))
            .await?;
        let (szi_side, position_size) = extract_position_szi(&resp, &coin)?;
        let (close_side, close_size) = resolve_close(size, szi_side, position_size)?;

        let asset_idx = self.resolve_asset(symbol)?;
        let sz_decimals = self
            .meta_cache
            .sz_decimals(&coin)
            .ok_or_else(|| HlError::Parse(format!("szDecimals not found for '{}'", coin)))?;
        let mid = extract_mid_price(&self.client, &coin).await?;
        let slippage = slippage.unwrap_or_else(|| Decimal::new(5, 2));
        let limit_price = if close_side.is_buy() {
            mid * (Decimal::ONE + slippage)
        } else {
            mid * (Decimal::ONE - slippage)
        };
        let limit_price = round_price_perp(limit_price, sz_decimals);
        let close_size = round_size(close_size, sz_decimals);

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
/// Queries `l2Book` for the given coin and returns `(best_bid + best_ask) / 2`.
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

    let levels = resp
        .get("levels")
        .and_then(|v| v.as_array())
        .ok_or_else(|| HlError::Parse("l2Book response missing 'levels' array".into()))?;

    if levels.len() < 2 {
        return Err(HlError::Parse(
            "l2Book 'levels' array has fewer than 2 entries".into(),
        ));
    }

    let best_bid = levels[0]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|e| e.get("px"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| HlError::Parse("l2Book: missing best bid price".into()))?;

    let best_ask = levels[1]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|e| e.get("px"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| HlError::Parse("l2Book: missing best ask price".into()))?;

    let bid: Decimal = Decimal::from_str(best_bid)
        .map_err(|e| HlError::Parse(format!("l2Book: invalid bid price '{}': {}", best_bid, e)))?;
    let ask: Decimal = Decimal::from_str(best_ask)
        .map_err(|e| HlError::Parse(format!("l2Book: invalid ask price '{}': {}", best_ask, e)))?;

    Ok((bid + ask) / Decimal::from(2))
}

/// Extract a position's size and side from a `clearinghouseState` response.
///
/// Returns `(side, abs_size)` where `side` is the position direction (Buy = long, Sell = short).
fn extract_position_szi(resp: &serde_json::Value, coin: &str) -> Result<(Side, Decimal), HlError> {
    let positions = resp
        .get("assetPositions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| HlError::Parse("clearinghouseState: missing 'assetPositions'".into()))?;

    for pos in positions {
        let position = &pos["position"];
        let pos_coin = position.get("coin").and_then(|v| v.as_str()).unwrap_or("");
        if pos_coin.to_uppercase() != coin.to_uppercase() {
            continue;
        }
        let szi_str = position
            .get("szi")
            .and_then(|v| v.as_str())
            .ok_or_else(|| HlError::Parse("clearinghouseState: missing 'szi' field".into()))?;
        let szi: Decimal = Decimal::from_str(szi_str).map_err(|e| {
            HlError::Parse(format!(
                "clearinghouseState: invalid szi '{}': {}",
                szi_str, e
            ))
        })?;

        if szi.is_zero() {
            return Err(HlError::Parse(format!(
                "market_close: position size for {} is zero",
                coin
            )));
        }

        let side = if szi > Decimal::ZERO {
            Side::Buy // long
        } else {
            Side::Sell // short
        };
        return Ok((side, szi.abs()));
    }

    Err(HlError::Parse(format!(
        "market_close: no open position found for {}",
        coin
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_close_derives_side_from_position_not_size_sign() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let mag = Decimal::from_str("1.5").unwrap();
        let pos = Decimal::from_str("4").unwrap();
        assert_eq!(
            resolve_close(Some(mag), Side::Buy, pos).unwrap(),
            (Side::Sell, mag)
        );
        assert_eq!(
            resolve_close(Some(mag), Side::Sell, pos).unwrap(),
            (Side::Buy, mag)
        );
        assert_eq!(
            resolve_close(None, Side::Buy, pos).unwrap(),
            (Side::Sell, pos)
        );
        assert!(resolve_close(Some(Decimal::from_str("-1").unwrap()), Side::Buy, pos).is_err());
        assert!(resolve_close(Some(Decimal::ZERO), Side::Buy, pos).is_err());
    }

    #[test]
    fn new_cloid_is_hyperliquid_format() {
        let c = new_cloid();
        assert!(c.starts_with("0x"), "cloid must be 0x-prefixed: {c}");
        assert_eq!(c.len(), 34, "cloid must be 0x + 32 hex chars: {c}");
        assert!(c[2..].chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn ensure_cloid_injects_then_is_idempotent() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let mut order = OrderWire::limit_buy(
            0,
            Decimal::from_str("100").unwrap(),
            Decimal::from_str("1").unwrap(),
        )
        .build()
        .unwrap();
        assert!(order.cloid.is_none());
        ensure_cloid(&mut order);
        let first = order.cloid.clone().unwrap();
        assert!(first.starts_with("0x") && first.len() == 34);
        ensure_cloid(&mut order);
        assert_eq!(
            order.cloid.as_deref(),
            Some(first.as_str()),
            "must not overwrite"
        );
    }

    #[test]
    fn round_price_perp_caps_5_sig_figs() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let p = round_price_perp(Decimal::from_str("66604.125").unwrap(), 5);
        assert_eq!(p, Decimal::from_str("66604").unwrap());
        let p2 = round_price_perp(Decimal::from_str("0.0034521").unwrap(), 0);
        assert_eq!(p2, Decimal::from_str("0.003452").unwrap());
    }

    #[test]
    fn round_size_truncates_to_sz_decimals() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let s = round_size(Decimal::from_str("0.123456").unwrap(), 3);
        assert_eq!(s, Decimal::from_str("0.123").unwrap());
    }

    #[test]
    fn round_price_perp_preserves_integer_prices() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        // Hyperliquid allows integer prices regardless of sig figs — must NOT round.
        assert_eq!(
            round_price_perp(Decimal::from(123456), 0),
            Decimal::from(123456)
        );
        assert_eq!(
            round_price_perp(Decimal::from(1234567), 0),
            Decimal::from(1234567)
        );
        // Non-integer prices are still capped to 5 significant figures.
        assert_eq!(
            round_price_perp(Decimal::from_str("123456.7").unwrap(), 0),
            Decimal::from(123460)
        );
    }

    #[test]
    fn round_size_to_zero_then_build_is_rejected() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        // A sub-lot size truncates to 0...
        let sz = round_size(Decimal::from_str("0.0009").unwrap(), 3);
        assert_eq!(sz, Decimal::ZERO);
        // ...and a trigger order built from it must be rejected, not sent as "0".
        let built = OrderWire::trigger_buy(0, Decimal::from(100), sz, Tpsl::Sl).build();
        assert!(built.is_err(), "zero size must fail build()");
    }

    #[test]
    fn order_to_json_limit_shape() {
        use rust_decimal::Decimal;
        let order = OrderWire::limit_buy(3, Decimal::from(100), Decimal::from(2))
            .tif(Tif::Ioc)
            .cloid("0xabc")
            .build()
            .unwrap();
        let j = order_to_json(&order).unwrap();
        assert_eq!(j["a"].as_u64(), Some(3));
        assert_eq!(j["b"].as_bool(), Some(true));
        assert_eq!(j["p"].as_str(), Some("100"));
        assert_eq!(j["s"].as_str(), Some("2"));
        assert_eq!(j["r"].as_bool(), Some(false));
        assert_eq!(j["t"]["limit"]["tif"].as_str(), Some("Ioc"));
        assert_eq!(j["c"].as_str(), Some("0xabc"));
        assert!(j["t"].get("trigger").is_none());
    }

    #[test]
    fn order_to_json_trigger_shape() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let order = OrderWire::trigger_sell(
            0,
            Decimal::from(95000),
            Decimal::from_str("0.5").unwrap(),
            Tpsl::Sl,
        )
        .build()
        .unwrap();
        let j = order_to_json(&order).unwrap();
        assert_eq!(j["a"].as_u64(), Some(0));
        assert_eq!(j["b"].as_bool(), Some(false));
        assert_eq!(j["p"].as_str(), Some("95000"));
        assert_eq!(j["s"].as_str(), Some("0.5"));
        assert_eq!(j["r"].as_bool(), Some(true));
        assert_eq!(j["t"]["trigger"]["triggerPx"].as_str(), Some("95000"));
        assert_eq!(j["t"]["trigger"]["isMarket"].as_bool(), Some(true));
        assert_eq!(j["t"]["trigger"]["tpsl"].as_str(), Some("sl"));
        assert!(j.get("c").is_none(), "no cloid set -> no c key");
    }

    #[test]
    fn determine_status_boundaries() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let req = Decimal::from(100);
        // Full fill and exactly the 0.99 (FILL_THRESHOLD) ratio are Filled.
        assert_eq!(determine_status(req, req, "x"), OrderStatus::Filled);
        assert_eq!(
            determine_status(Decimal::from(99), req, "x"),
            OrderStatus::Filled
        );
        // Just below the threshold but non-zero is Partial.
        assert_eq!(
            determine_status(Decimal::from_str("98.99").unwrap(), req, "x"),
            OrderStatus::Partial
        );
        assert_eq!(
            determine_status(Decimal::from_str("0.001").unwrap(), req, "x"),
            OrderStatus::Partial
        );
        // Zero fill is Open.
        assert_eq!(determine_status(Decimal::ZERO, req, "x"), OrderStatus::Open);
    }

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
}
