use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::order::OrderStatus;

/// Response returned after placing an order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
}
