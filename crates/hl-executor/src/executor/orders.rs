use std::str::FromStr;

use rust_decimal::Decimal;

use hl_types::*;

use super::response::{parse_order_response, parse_bulk_order_response_with_fallbacks};
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
        let fallback_price: Decimal =
            Decimal::from_str(&order.limit_px).unwrap_or(Decimal::ZERO);
        let fallback_size: Decimal = Decimal::from_str(&order.sz).unwrap_or(Decimal::ZERO);

        let order_json = order_to_json(&order)?;

        let action = serde_json::json!({
            "type": "order",
            "orders": [order_json],
            "grouping": "na"
        });

        let result = self.send_signed_action(action, vault).await?;

        let api_status = result
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");

        if api_status != "ok" {
            return Err(HlError::Rejected {
                reason: format!("Exchange rejected order: {}", result),
            });
        }

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

        let api_status = result
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");

        if api_status != "ok" {
            return Err(HlError::Rejected {
                reason: format!("Trigger order rejected: {}", result),
            });
        }

        let (order_id, fill_price, fill_size) =
            parse_order_response(&result, trigger_price, size)?;

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
        let nonce = self.next_nonce();
        let sig = hl_signing::sign_l1_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            nonce,
            self.client.is_mainnet(),
            None,
        )?;
        let resp = self.client.post_action(action, &sig, nonce, None).await?;
        serde_json::from_value(resp)
            .map_err(|e| HlError::Parse(format!("transfer_to_vault response: {e}")))
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

        let api_status = result
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");

        if api_status != "ok" {
            return Err(HlError::Rejected {
                reason: format!("Exchange rejected bulk order: {}", result),
            });
        }

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
}
