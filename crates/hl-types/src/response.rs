use serde::{Deserialize, Serialize};

/// Response returned after placing an order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponse {
    /// Exchange-assigned order identifier.
    pub order_id: String,
    /// Price at which the order was (partially) filled, if any.
    pub filled_price: Option<f64>,
    /// Size that was filled.
    pub filled_size: f64,
    /// Size that was originally requested.
    pub requested_size: f64,
    /// Order status (e.g. "filled", "partial", "open", "rejected").
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_response_serde_roundtrip() {
        let resp = OrderResponse {
            order_id: "abc123".into(),
            filled_price: Some(50000.0),
            filled_size: 0.1,
            requested_size: 0.1,
            status: "filled".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: OrderResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.order_id, "abc123");
        assert!((parsed.filled_price.unwrap() - 50000.0).abs() < f64::EPSILON);
        assert!((parsed.filled_size - 0.1).abs() < f64::EPSILON);
        assert!((parsed.requested_size - 0.1).abs() < f64::EPSILON);
        assert_eq!(parsed.status, "filled");
    }

    #[test]
    fn order_response_no_fill_price_roundtrip() {
        let resp = OrderResponse {
            order_id: "xyz".into(),
            filled_price: None,
            filled_size: 0.0,
            requested_size: 1.0,
            status: "open".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: OrderResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.filled_price.is_none());
        assert_eq!(parsed.status, "open");
    }

    #[test]
    fn order_response_camel_case_keys() {
        let resp = OrderResponse {
            order_id: "x".into(),
            filled_price: None,
            filled_size: 0.0,
            requested_size: 0.0,
            status: "x".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("orderId"));
        assert!(json.contains("filledPrice"));
        assert!(json.contains("filledSize"));
        assert!(json.contains("requestedSize"));
    }
}
