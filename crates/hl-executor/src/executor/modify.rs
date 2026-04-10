use std::str::FromStr;

use rust_decimal::Decimal;

use hl_types::*;

use super::orders::{determine_status, order_to_json};
use super::response::{parse_bulk_order_response_with_fallbacks, parse_order_response};
use super::OrderExecutor;

impl OrderExecutor {
    /// Modify an existing order in-place (atomic amendment, not cancel+replace).
    #[tracing::instrument(skip(self, new_order), fields(oid))]
    pub async fn modify_order(
        &self,
        oid: u64,
        new_order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let fallback_price =
            Decimal::from_str(&new_order.limit_px).unwrap_or(Decimal::ZERO);
        let fallback_size =
            Decimal::from_str(&new_order.sz).unwrap_or(Decimal::ZERO);

        let order_json = order_to_json(&new_order)?;
        let action = serde_json::json!({
            "type": "batchModify",
            "modifies": [{"oid": oid, "order": order_json}]
        });

        let result = self.send_signed_action(action, vault).await?;

        let api_status = result
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");

        if api_status != "ok" {
            return Err(HlError::Rejected {
                reason: format!("Exchange rejected modify order: {}", result),
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

    /// Modify multiple orders in a single signed action.
    #[tracing::instrument(skip(self, modifies), fields(count = modifies.len()))]
    pub async fn bulk_modify(
        &self,
        modifies: Vec<ModifyRequest>,
        vault: Option<&str>,
    ) -> Result<Vec<OrderResponse>, HlError> {
        if modifies.is_empty() {
            return Ok(vec![]);
        }

        let mut modify_jsons = Vec::with_capacity(modifies.len());
        let mut fallbacks: Vec<(Decimal, Decimal)> = Vec::with_capacity(modifies.len());

        for m in &modifies {
            let order_json = order_to_json(&m.order)?;
            modify_jsons.push(serde_json::json!({"oid": m.oid, "order": order_json}));
            fallbacks.push((
                Decimal::from_str(&m.order.limit_px).unwrap_or(Decimal::ZERO),
                Decimal::from_str(&m.order.sz).unwrap_or(Decimal::ZERO),
            ));
        }

        let action = serde_json::json!({
            "type": "batchModify",
            "modifies": modify_jsons
        });

        let result = self.send_signed_action(action, vault).await?;

        let api_status = result
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");

        if api_status != "ok" {
            return Err(HlError::Rejected {
                reason: format!("Exchange rejected bulk modify: {}", result),
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
}
