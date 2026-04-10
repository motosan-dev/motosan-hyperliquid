use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::order::OrderStatus;

/// Response returned after placing an order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct OrderResponse {
    /// Exchange-assigned order identifier.
    pub order_id: String,
    /// Price at which the order was (partially) filled, if any.
    pub filled_price: Option<Decimal>,
    /// Size that was filled.
    pub filled_size: Decimal,
    /// Size that was originally requested.
    pub requested_size: Decimal,
    /// Order status.
    pub status: OrderStatus,
}

impl OrderResponse {
    /// Creates a new `OrderResponse`.
    pub fn new(
        order_id: String,
        filled_price: Option<Decimal>,
        filled_size: Decimal,
        requested_size: Decimal,
        status: OrderStatus,
    ) -> Self {
        Self {
            order_id,
            filled_price,
            filled_size,
            requested_size,
            status,
        }
    }
}

/// Generic response from an exchange action (cancel, transfer, etc.).
///
/// The Hyperliquid exchange returns `{"status": "ok", "response": {...}}` for
/// successful actions. This struct captures the top-level status and preserves
/// the inner response payload and any extra fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HlActionResponse {
    /// Top-level status string (typically `"ok"`).
    pub status: String,
    /// Inner response payload, if present.
    #[serde(default)]
    pub response: Option<serde_json::Value>,
    /// Any additional top-level fields returned by the API.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl HlActionResponse {
    /// Returns `true` if the exchange reported status `"ok"`.
    pub fn is_ok(&self) -> bool {
        self.status == "ok"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn order_response_serde_roundtrip() {
        let resp = OrderResponse {
            order_id: "abc123".into(),
            filled_price: Some(Decimal::from_str("50000.0").unwrap()),
            filled_size: Decimal::from_str("0.1").unwrap(),
            requested_size: Decimal::from_str("0.1").unwrap(),
            status: OrderStatus::Filled,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: OrderResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.order_id, "abc123");
        assert_eq!(
            parsed.filled_price,
            Some(Decimal::from_str("50000.0").unwrap())
        );
        assert_eq!(parsed.filled_size, Decimal::from_str("0.1").unwrap());
        assert_eq!(parsed.requested_size, Decimal::from_str("0.1").unwrap());
        assert_eq!(parsed.status, OrderStatus::Filled);
    }

    #[test]
    fn order_response_no_fill_price_roundtrip() {
        let resp = OrderResponse {
            order_id: "xyz".into(),
            filled_price: None,
            filled_size: Decimal::ZERO,
            requested_size: Decimal::ONE,
            status: OrderStatus::Open,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: OrderResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.filled_price.is_none());
        assert_eq!(parsed.status, OrderStatus::Open);
    }

    #[test]
    fn order_response_camel_case_keys() {
        let resp = OrderResponse {
            order_id: "x".into(),
            filled_price: None,
            filled_size: Decimal::ZERO,
            requested_size: Decimal::ZERO,
            status: OrderStatus::Open,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("orderId"));
        assert!(json.contains("filledPrice"));
        assert!(json.contains("filledSize"));
        assert!(json.contains("requestedSize"));
    }

    #[test]
    fn action_response_ok_roundtrip() {
        let json = serde_json::json!({
            "status": "ok",
            "response": {"type": "cancel", "data": {"statuses": ["success"]}}
        });
        let parsed: HlActionResponse = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.status, "ok");
        assert!(parsed.is_ok());
        assert!(parsed.response.is_some());
    }

    #[test]
    fn action_response_error() {
        let json = serde_json::json!({
            "status": "err",
            "response": "Order not found"
        });
        let parsed: HlActionResponse = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.status, "err");
        assert!(!parsed.is_ok());
    }

    #[test]
    fn action_response_no_response_field() {
        let json = serde_json::json!({ "status": "ok" });
        let parsed: HlActionResponse = serde_json::from_value(json).unwrap();
        assert!(parsed.is_ok());
        assert!(parsed.response.is_none());
    }

    #[test]
    fn action_response_extra_fields_captured() {
        let json = serde_json::json!({
            "status": "ok",
            "timestamp": 1700000000
        });
        let parsed: HlActionResponse = serde_json::from_value(json).unwrap();
        assert!(parsed.extra.contains_key("timestamp"));
    }

    #[test]
    fn action_response_serde_roundtrip() {
        let resp = HlActionResponse {
            status: "ok".into(),
            response: Some(serde_json::json!({"data": "test"})),
            extra: HashMap::new(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: HlActionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, "ok");
        assert!(parsed.response.is_some());
    }
}
