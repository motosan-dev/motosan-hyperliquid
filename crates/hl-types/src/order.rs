use serde::{Deserialize, Serialize};

/// Wire format for an order sent to the Hyperliquid exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderWire {
    /// Asset index (perp index or spot index with offset).
    pub asset: u32,
    /// Whether this is a buy order.
    pub is_buy: bool,
    /// Limit price as a decimal string.
    pub limit_px: String,
    /// Size as a decimal string.
    pub sz: String,
    /// Whether the order is reduce-only.
    pub reduce_only: bool,
    /// Order type wire format.
    pub order_type: OrderTypeWire,
    /// Optional client order ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloid: Option<String>,
}

/// Wire format for order type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderTypeWire {
    /// Limit order with time-in-force, or trigger order.
    pub limit: Option<LimitOrderType>,
    pub trigger: Option<TriggerOrderType>,
}

/// Limit order type wire format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitOrderType {
    pub tif: String,
}

/// Trigger order type wire format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerOrderType {
    pub trigger_px: String,
    pub is_market: bool,
    pub tpsl: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_wire_serde_roundtrip() {
        let order = OrderWire {
            asset: 1,
            is_buy: true,
            limit_px: "50000.0".into(),
            sz: "0.1".into(),
            reduce_only: false,
            order_type: OrderTypeWire {
                limit: Some(LimitOrderType { tif: "Gtc".into() }),
                trigger: None,
            },
            cloid: None,
        };
        let json = serde_json::to_string(&order).unwrap();
        let parsed: OrderWire = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.asset, 1);
        assert!(parsed.is_buy);
        assert_eq!(parsed.limit_px, "50000.0");
        assert_eq!(parsed.sz, "0.1");
        assert!(!parsed.reduce_only);
        assert!(parsed.cloid.is_none());
        assert_eq!(parsed.order_type.limit.as_ref().unwrap().tif, "Gtc");
        assert!(parsed.order_type.trigger.is_none());
    }

    #[test]
    fn order_wire_with_cloid_roundtrip() {
        let order = OrderWire {
            asset: 5,
            is_buy: false,
            limit_px: "3000.5".into(),
            sz: "2.0".into(),
            reduce_only: true,
            order_type: OrderTypeWire { limit: None, trigger: None },
            cloid: Some("my-order-123".into()),
        };
        let json = serde_json::to_string(&order).unwrap();
        let parsed: OrderWire = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.cloid.as_deref(), Some("my-order-123"));
        assert!(parsed.reduce_only);
        assert!(!parsed.is_buy);
    }

    #[test]
    fn order_wire_with_trigger_roundtrip() {
        let order = OrderWire {
            asset: 0,
            is_buy: true,
            limit_px: "100.0".into(),
            sz: "10.0".into(),
            reduce_only: false,
            order_type: OrderTypeWire {
                limit: None,
                trigger: Some(TriggerOrderType {
                    trigger_px: "99.0".into(),
                    is_market: true,
                    tpsl: "tp".into(),
                }),
            },
            cloid: None,
        };
        let json = serde_json::to_string(&order).unwrap();
        let parsed: OrderWire = serde_json::from_str(&json).unwrap();
        let trigger = parsed.order_type.trigger.unwrap();
        assert_eq!(trigger.trigger_px, "99.0");
        assert!(trigger.is_market);
        assert_eq!(trigger.tpsl, "tp");
    }

    #[test]
    fn order_wire_camel_case_serialization() {
        let order = OrderWire {
            asset: 0,
            is_buy: true,
            limit_px: "1.0".into(),
            sz: "1.0".into(),
            reduce_only: false,
            order_type: OrderTypeWire { limit: None, trigger: None },
            cloid: None,
        };
        let json = serde_json::to_string(&order).unwrap();
        assert!(json.contains("isBuy"));
        assert!(json.contains("limitPx"));
        assert!(json.contains("reduceOnly"));
        assert!(json.contains("orderType"));
        // cloid is None and skip_serializing_if, so should not appear
        assert!(!json.contains("cloid"));
    }
}
