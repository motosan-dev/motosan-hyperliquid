use std::str::FromStr;

use rust_decimal::{Decimal, RoundingStrategy};

use hl_types::{Grouping, HlError, OrderResponse, OrderStatus, OrderWire, Side, Tif, Tpsl};

use super::response::{parse_bulk_order_response_with_fallbacks, parse_order_response};
use super::{validate_eth_address, OrderExecutor, FILL_THRESHOLD};

/// Round a price to Hyperliquid's rule: at most 5 significant figures AND at
/// most `max_decimals - sz_decimals` decimal places. Integer prices are returned
/// unchanged — Hyperliquid allows them regardless of significant figures.
/// `max_decimals` is 6 for perps and 8 for spot.
fn round_price(px: Decimal, sz_decimals: u32, max_decimals: u32) -> Decimal {
    let max_dp = max_decimals.saturating_sub(sz_decimals);
    let sf = if px.fract() == Decimal::ZERO {
        px
    } else {
        px.round_sf(5).unwrap_or(px)
    };
    sf.round_dp(max_dp)
}

/// Round a **perp** price (`MAX_DECIMALS = 6`). See [`round_price`].
pub(crate) fn round_price_perp(px: Decimal, sz_decimals: u32) -> Decimal {
    round_price(px, sz_decimals, 6)
}

/// Round a **spot** price (`MAX_DECIMALS = 8`). See [`round_price`].
pub(crate) fn round_price_spot(px: Decimal, sz_decimals: u32) -> Decimal {
    round_price(px, sz_decimals, 8)
}

/// Round an order size down (toward zero) to the asset's `szDecimals`.
pub(crate) fn round_size(sz: Decimal, sz_decimals: u32) -> Decimal {
    sz.round_dp_with_strategy(sz_decimals, RoundingStrategy::ToZero)
}

/// Generate a Hyperliquid client order id (`0x` + 32 hex chars) for idempotent
/// submission — the exchange dedups retries by cloid.
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
/// unsigned magnitude: `Some(m)` closes `m` (must be > 0), `None` closes the
/// full position. Direction is never encoded in the sign of `size`.
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
            return Err(HlError::Validation(
                "market_close: size must be a positive magnitude".into(),
            ))
        }
        None => position_size,
    };
    Ok((close_side, close_size))
}

/// Build wire-format JSON from an [`OrderWire`].
///
/// The `t` (order-type) sub-object is produced by `OrderTypeWire`'s own
/// `Serialize` impl, so the limit/trigger wire shape is defined in one place
/// (and no `#[non_exhaustive]` wildcard arm is needed here).
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

/// Build the optional `builder` sub-object for an `order` action:
/// `{"b": <lowercased address>, "f": <fee>}`. `fee` is in **tenths of a basis
/// point** (`f = 10` ⇒ 1 bp ⇒ 0.01%) and is emitted as a JSON integer, matching
/// the Python SDK's `BuilderInfo`. The address is validated and lowercased so
/// the signed msgpack bytes match the canonical wire. Returns `Ok(None)` when no
/// builder is supplied (the action then omits the `builder` key entirely).
fn builder_to_json(builder: Option<(&str, u32)>) -> Result<Option<serde_json::Value>, HlError> {
    match builder {
        Some((address, fee)) => {
            validate_eth_address(address)?;
            Ok(Some(serde_json::json!({
                "b": address.to_lowercase(),
                "f": fee,
            })))
        }
        None => Ok(None),
    }
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
    pub async fn place_order(
        &self,
        order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        self.place_order_with_builder(order, None, vault).await
    }

    /// Like [`Self::place_order`] but attaches a builder code to the order
    /// action so a builder earns the configured fee on the fill.
    ///
    /// `builder` is `(address, fee)` where `fee` is in **tenths of a basis
    /// point** (`10` ⇒ 1 bp ⇒ 0.01%) and must be ≤ the `maxFeeRate` the user
    /// previously authorized via [`Self::approve_builder_fee`]. Passing `None`
    /// produces the identical wire to [`Self::place_order`].
    #[tracing::instrument(skip(self, order), fields(asset = order.asset, is_buy = order.is_buy))]
    pub async fn place_order_with_builder(
        &self,
        mut order: OrderWire,
        builder: Option<(&str, u32)>,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        ensure_cloid(&mut order);

        let fallback_price: Decimal = Decimal::from_str(&order.limit_px).unwrap_or(Decimal::ZERO);
        let fallback_size: Decimal = Decimal::from_str(&order.sz).unwrap_or(Decimal::ZERO);

        let order_json = order_to_json(&order)?;

        let mut action = serde_json::json!({
            "type": "order",
            "orders": [order_json],
            "grouping": "na"
        });
        // `builder` must be inserted LAST (after type/orders/grouping) to match
        // the Python SDK's msgpack key order, which the action hash depends on.
        if let Some(builder_json) = builder_to_json(builder)? {
            action["builder"] = builder_json;
        }

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
    pub async fn place_trigger_order(
        &self,
        symbol: &str,
        side: Side,
        size: Decimal,
        trigger_price: Decimal,
        tpsl: Tpsl,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        self.place_trigger_order_with_builder(symbol, side, size, trigger_price, tpsl, None, vault)
            .await
    }

    /// Like [`Self::place_trigger_order`] but attaches a builder code
    /// `(address, fee)` (fee in tenths of a basis point) to the order action.
    #[tracing::instrument(skip(self))]
    #[allow(clippy::too_many_arguments)]
    pub async fn place_trigger_order_with_builder(
        &self,
        symbol: &str,
        side: Side,
        size: Decimal,
        trigger_price: Decimal,
        tpsl: Tpsl,
        builder: Option<(&str, u32)>,
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
        // size that round_size truncated to 0 must be rejected, not sent as "0".
        let order = if side.is_buy() {
            OrderWire::trigger_buy(asset_idx, trigger_price, size, tpsl)
        } else {
            OrderWire::trigger_sell(asset_idx, trigger_price, size, tpsl)
        }
        .cloid(new_cloid())
        .build()?;

        let mut action = serde_json::json!({
            "type": "order",
            "orders": [order_to_json(&order)?],
            "grouping": "na"
        });
        if let Some(builder_json) = builder_to_json(builder)? {
            action["builder"] = builder_json;
        }

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
    pub async fn bulk_order(
        &self,
        orders: Vec<OrderWire>,
        vault: Option<&str>,
    ) -> Result<Vec<OrderResponse>, HlError> {
        self.bulk_order_inner(orders, Grouping::Na, None, vault)
            .await
    }

    /// Place multiple orders with an explicit [`Grouping`] — e.g. an OCO TP/SL
    /// bracket via [`Grouping::NormalTpsl`].
    ///
    /// The orders are sent in the order given; this method does **not** reorder
    /// or auto-construct the bracket. For `NormalTpsl` the parent entry order
    /// must be at index 0, followed by its (reduce-only, opposite-side) TP/SL
    /// children — the caller is responsible for that, matching the Python SDK.
    pub async fn bulk_order_grouped(
        &self,
        orders: Vec<OrderWire>,
        grouping: Grouping,
        vault: Option<&str>,
    ) -> Result<Vec<OrderResponse>, HlError> {
        self.bulk_order_inner(orders, grouping, None, vault).await
    }

    /// Place multiple orders, attaching a builder code `(address, fee)` (fee in
    /// tenths of a basis point) to the action.
    pub async fn bulk_order_with_builder(
        &self,
        orders: Vec<OrderWire>,
        builder: Option<(&str, u32)>,
        vault: Option<&str>,
    ) -> Result<Vec<OrderResponse>, HlError> {
        self.bulk_order_inner(orders, Grouping::Na, builder, vault)
            .await
    }

    #[tracing::instrument(skip(self, orders), fields(count = orders.len(), grouping = grouping.as_str()))]
    async fn bulk_order_inner(
        &self,
        mut orders: Vec<OrderWire>,
        grouping: Grouping,
        builder: Option<(&str, u32)>,
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

        let mut action = serde_json::json!({
            "type": "order",
            "orders": order_jsons,
            "grouping": grouping.as_str()
        });
        if let Some(builder_json) = builder_to_json(builder)? {
            action["builder"] = builder_json;
        }

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
    /// `size` is an unsigned magnitude (`Some` closes that amount, `None` the
    /// full position) — direction is never encoded in its sign.
    #[tracing::instrument(skip(self))]
    pub async fn market_close(
        &self,
        symbol: &str,
        size: Option<Decimal>,
        slippage: Option<Decimal>,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let coin = super::normalize_symbol(symbol);

        // Always query the live position — the close side comes from the
        // position sign, never from the sign of the caller's size (R5).
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
/// Queries `l2Book` for the given coin and delegates parsing to
/// [`hl_types::parse_mid_price_from_l2book`].
pub(crate) async fn extract_mid_price(
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

    use std::str::FromStr;

    #[test]
    fn round_price_perp_caps_5_sig_figs() {
        let p = round_price_perp(Decimal::from_str("66604.125").unwrap(), 5);
        assert_eq!(p, Decimal::from_str("66604").unwrap());
        let p2 = round_price_perp(Decimal::from_str("0.0034521").unwrap(), 0);
        assert_eq!(p2, Decimal::from_str("0.003452").unwrap());
    }

    #[test]
    fn round_price_perp_preserves_integer_prices() {
        // Hyperliquid allows integer prices regardless of sig figs — must NOT round.
        assert_eq!(
            round_price_perp(Decimal::from(123456), 0),
            Decimal::from(123456)
        );
        assert_eq!(
            round_price_perp(Decimal::from(1234567), 0),
            Decimal::from(1234567)
        );
        assert_eq!(
            round_price_perp(Decimal::from_str("123456.7").unwrap(), 0),
            Decimal::from(123460)
        );
    }

    #[test]
    fn round_price_spot_uses_8_max_decimals() {
        // Spot allows MAX_DECIMALS = 8 vs perp's 6, so it keeps more decimals at
        // the same szDecimals (both still cap at 5 significant figures).
        assert_eq!(
            round_price_perp(Decimal::from_str("0.00012345678").unwrap(), 0),
            Decimal::from_str("0.000123").unwrap()
        );
        assert_eq!(
            round_price_spot(Decimal::from_str("0.00012345678").unwrap(), 0),
            Decimal::from_str("0.00012346").unwrap()
        );
        // Integers pass through for spot too.
        assert_eq!(
            round_price_spot(Decimal::from(123456), 0),
            Decimal::from(123456)
        );
    }

    #[test]
    fn round_size_truncates_to_sz_decimals() {
        let s = round_size(Decimal::from_str("0.123456").unwrap(), 3);
        assert_eq!(s, Decimal::from_str("0.123").unwrap());
    }

    #[test]
    fn round_size_to_zero_then_build_is_rejected() {
        let sz = round_size(Decimal::from_str("0.0009").unwrap(), 3);
        assert_eq!(sz, Decimal::ZERO);
        // A trigger order built from a zeroed size must be rejected, not sent as "0".
        let built = OrderWire::trigger_buy(0, Decimal::from(100), sz, Tpsl::Sl).build();
        assert!(built.is_err(), "zero size must fail build()");
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
        let mut order = OrderWire::limit_buy(0, Decimal::from(100), Decimal::from(1))
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
    fn resolve_close_derives_side_from_position_not_size_sign() {
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
    fn order_to_json_limit_shape() {
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
        let order = OrderWire::trigger_sell(
            0,
            Decimal::from(95000),
            Decimal::from_str("0.5").unwrap(),
            Tpsl::Sl,
        )
        .build()
        .unwrap();
        let j = order_to_json(&order).unwrap();
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
    fn builder_to_json_lowercases_and_emits_integer_fee() {
        let j = builder_to_json(Some(("0x00000000000000000000000000000000000000AB", 10)))
            .unwrap()
            .unwrap();
        // Address lowercased to match the canonical signed bytes.
        assert_eq!(
            j["b"].as_str(),
            Some("0x00000000000000000000000000000000000000ab")
        );
        // Fee is a JSON integer (not a string) — msgpack type matters for the hash.
        assert_eq!(j["f"].as_u64(), Some(10));
        assert_eq!(j["f"].as_str(), None);
    }

    #[test]
    fn builder_to_json_none_is_none() {
        assert!(builder_to_json(None).unwrap().is_none());
    }

    #[test]
    fn builder_to_json_rejects_bad_address() {
        assert!(matches!(
            builder_to_json(Some(("0x1234", 5))),
            Err(HlError::InvalidAddress(_))
        ));
    }

    #[test]
    fn grouping_action_json_shape() {
        // Mirror how bulk_order_inner builds the action: parent + TP + SL.
        let parent = OrderWire::limit_buy(1, Decimal::from(3000), Decimal::from(1))
            .build()
            .unwrap();
        let tp = OrderWire::trigger_sell(1, Decimal::from(3300), Decimal::from(1), Tpsl::Tp)
            .reduce_only(true)
            .build()
            .unwrap();
        let sl = OrderWire::trigger_sell(1, Decimal::from(2700), Decimal::from(1), Tpsl::Sl)
            .reduce_only(true)
            .build()
            .unwrap();
        let orders = [parent, tp, sl];
        let order_jsons: Vec<_> = orders.iter().map(|o| order_to_json(o).unwrap()).collect();
        let action = serde_json::json!({
            "type": "order",
            "orders": order_jsons,
            "grouping": Grouping::NormalTpsl.as_str()
        });
        assert_eq!(action["grouping"].as_str(), Some("normalTpsl"));
        assert_eq!(action["orders"].as_array().unwrap().len(), 3);
        assert_eq!(action["orders"][0]["r"].as_bool(), Some(false));
        assert_eq!(action["orders"][1]["r"].as_bool(), Some(true));
        assert_eq!(
            action["orders"][1]["t"]["trigger"]["tpsl"].as_str(),
            Some("tp")
        );
        assert_eq!(
            action["orders"][2]["t"]["trigger"]["tpsl"].as_str(),
            Some("sl")
        );
        // No builder key when none supplied.
        assert!(action.get("builder").is_none());
    }

    #[test]
    fn determine_status_boundaries() {
        let req = Decimal::from(100);
        assert_eq!(determine_status(req, req, "x"), OrderStatus::Filled);
        assert_eq!(
            determine_status(Decimal::from(99), req, "x"),
            OrderStatus::Filled
        );
        assert_eq!(
            determine_status(Decimal::from_str("98.99").unwrap(), req, "x"),
            OrderStatus::Partial
        );
        assert_eq!(
            determine_status(Decimal::from_str("0.001").unwrap(), req, "x"),
            OrderStatus::Partial
        );
        assert_eq!(determine_status(Decimal::ZERO, req, "x"), OrderStatus::Open);
    }

    // ── Mock-based integration tests ───────────────────────────

    use hl_test_utils::{test_executor, test_executor_capturing};

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
    async fn place_order_with_builder_accepts_and_parses() {
        let executor = test_executor(vec![ok_resting_response(123)]);
        let order = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from(1))
            .build()
            .unwrap();
        let resp = executor
            .place_order_with_builder(
                order,
                Some(("0x000000000000000000000000000000000000007a", 10)),
                None,
            )
            .await
            .unwrap();
        assert_eq!(resp.order_id, "123");
    }

    #[tokio::test]
    async fn bulk_order_grouped_threads_grouping() {
        let canned = serde_json::json!({
            "status": "ok",
            "response": {
                "type": "order",
                "data": {
                    "statuses": [
                        {"resting": {"oid": 100}},
                        {"resting": {"oid": 101}}
                    ]
                }
            }
        });
        let executor = test_executor(vec![canned]);
        let parent = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from(1))
            .build()
            .unwrap();
        let sl = OrderWire::trigger_sell(0, Decimal::from(85000), Decimal::from(1), Tpsl::Sl)
            .reduce_only(true)
            .build()
            .unwrap();
        let resps = executor
            .bulk_order_grouped(vec![parent, sl], Grouping::NormalTpsl, None)
            .await
            .unwrap();
        assert_eq!(resps.len(), 2);
        assert_eq!(resps[0].order_id, "100");
        assert_eq!(resps[1].order_id, "101");
    }

    #[tokio::test]
    async fn place_order_with_builder_wire_format() {
        let (executor, transport) = test_executor_capturing(vec![ok_resting_response(1)]);
        let order = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from(1))
            .build()
            .unwrap();
        executor
            .place_order_with_builder(
                order,
                Some(("0x000000000000000000000000000000000000007A", 10)),
                None,
            )
            .await
            .unwrap();
        let action = transport.last_request().unwrap();
        // Address lowercased; fee is a JSON integer (not a string).
        assert_eq!(
            action["builder"]["b"],
            "0x000000000000000000000000000000000000007a"
        );
        assert_eq!(action["builder"]["f"].as_u64(), Some(10));
        assert!(action["builder"]["f"].as_str().is_none());
        // `builder` MUST be the last key (msgpack key order is signature-critical).
        let keys: Vec<&String> = action.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["type", "orders", "grouping", "builder"]);
    }

    #[tokio::test]
    async fn place_order_without_builder_omits_key() {
        let (executor, transport) = test_executor_capturing(vec![ok_resting_response(1)]);
        let order = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from(1))
            .build()
            .unwrap();
        executor.place_order(order, None).await.unwrap();
        let action = transport.last_request().unwrap();
        assert!(action.get("builder").is_none());
        assert_eq!(action["grouping"], "na");
    }

    #[tokio::test]
    async fn bulk_order_grouped_wire_format() {
        let canned = serde_json::json!({
            "status": "ok",
            "response": {"type": "order", "data": {"statuses": [{"resting": {"oid": 1}}]}}
        });
        let (executor, transport) = test_executor_capturing(vec![canned]);
        let parent = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from(1))
            .build()
            .unwrap();
        executor
            .bulk_order_grouped(vec![parent], Grouping::PositionTpsl, None)
            .await
            .unwrap();
        let action = transport.last_request().unwrap();
        assert_eq!(action["grouping"], "positionTpsl");
        assert!(action.get("builder").is_none());
    }

    #[tokio::test]
    async fn set_expires_after_threads_into_post_action() {
        let (executor, transport) = test_executor_capturing(vec![ok_resting_response(1)]);
        executor.set_expires_after(Some(1_700_000_060_000));
        let order = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from(1))
            .build()
            .unwrap();
        executor.place_order(order, None).await.unwrap();
        assert_eq!(transport.last_expires_after(), Some(1_700_000_060_000));
    }

    #[tokio::test]
    async fn expires_after_omitted_when_unset() {
        let (executor, transport) = test_executor_capturing(vec![ok_resting_response(1)]);
        // no set_expires_after call
        let order = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from(1))
            .build()
            .unwrap();
        executor.place_order(order, None).await.unwrap();
        assert_eq!(transport.last_expires_after(), None);
    }

    #[test]
    fn set_expires_after_zero_is_unset() {
        let executor = test_executor(vec![]);
        executor.set_expires_after(Some(0));
        assert_eq!(executor.expires_after(), None);
        executor.set_expires_after(Some(1_700_000_060_000));
        assert_eq!(executor.expires_after(), Some(1_700_000_060_000));
        executor.set_expires_after(None);
        assert_eq!(executor.expires_after(), None);
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
