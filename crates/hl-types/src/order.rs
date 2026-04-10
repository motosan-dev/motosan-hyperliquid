use serde::{Deserialize, Serialize};
use std::fmt;

/// Order side: buy or sell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    /// Returns `true` if this is the buy side.
    pub fn is_buy(self) -> bool {
        matches!(self, Side::Buy)
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Buy => write!(f, "buy"),
            Side::Sell => write!(f, "sell"),
        }
    }
}

/// Time-in-force for limit orders.
///
/// Wire format uses PascalCase: `"Gtc"`, `"Ioc"`, `"Alo"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tif {
    /// Good-til-cancelled.
    Gtc,
    /// Immediate-or-cancel.
    Ioc,
    /// Add-liquidity-only (post-only).
    Alo,
}

impl fmt::Display for Tif {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tif::Gtc => write!(f, "Gtc"),
            Tif::Ioc => write!(f, "Ioc"),
            Tif::Alo => write!(f, "Alo"),
        }
    }
}

/// Trigger order type: stop-loss or take-profit.
///
/// Wire format uses lowercase: `"sl"`, `"tp"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tpsl {
    /// Stop-loss trigger.
    Sl,
    /// Take-profit trigger.
    Tp,
}

impl fmt::Display for Tpsl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tpsl::Sl => write!(f, "sl"),
            Tpsl::Tp => write!(f, "tp"),
        }
    }
}

/// Position side: long or short.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionSide {
    Long,
    Short,
}

impl fmt::Display for PositionSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PositionSide::Long => write!(f, "long"),
            PositionSide::Short => write!(f, "short"),
        }
    }
}

/// Order status returned by the exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    /// Fully filled.
    Filled,
    /// Partially filled.
    Partial,
    /// Resting on the book.
    Open,
    /// Rejected by the exchange.
    Rejected,
    /// Triggered as stop-loss.
    TriggerSl,
    /// Triggered as take-profit.
    TriggerTp,
}

impl fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderStatus::Filled => write!(f, "filled"),
            OrderStatus::Partial => write!(f, "partial"),
            OrderStatus::Open => write!(f, "open"),
            OrderStatus::Rejected => write!(f, "rejected"),
            OrderStatus::TriggerSl => write!(f, "trigger_sl"),
            OrderStatus::TriggerTp => write!(f, "trigger_tp"),
        }
    }
}

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
    pub tif: Tif,
}

/// Trigger order type wire format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerOrderType {
    pub trigger_px: String,
    pub is_market: bool,
    pub tpsl: Tpsl,
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
                limit: Some(LimitOrderType { tif: Tif::Gtc }),
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
        assert_eq!(parsed.order_type.limit.as_ref().unwrap().tif, Tif::Gtc);
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
            order_type: OrderTypeWire {
                limit: None,
                trigger: None,
            },
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
                    tpsl: Tpsl::Tp,
                }),
            },
            cloid: None,
        };
        let json = serde_json::to_string(&order).unwrap();
        let parsed: OrderWire = serde_json::from_str(&json).unwrap();
        let trigger = parsed.order_type.trigger.unwrap();
        assert_eq!(trigger.trigger_px, "99.0");
        assert!(trigger.is_market);
        assert_eq!(trigger.tpsl, Tpsl::Tp);
    }

    #[test]
    fn order_wire_camel_case_serialization() {
        let order = OrderWire {
            asset: 0,
            is_buy: true,
            limit_px: "1.0".into(),
            sz: "1.0".into(),
            reduce_only: false,
            order_type: OrderTypeWire {
                limit: None,
                trigger: None,
            },
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

    #[test]
    fn tif_serde_wire_format() {
        // Tif serializes as PascalCase to match Hyperliquid wire format
        assert_eq!(serde_json::to_string(&Tif::Gtc).unwrap(), "\"Gtc\"");
        assert_eq!(serde_json::to_string(&Tif::Ioc).unwrap(), "\"Ioc\"");
        assert_eq!(serde_json::to_string(&Tif::Alo).unwrap(), "\"Alo\"");

        assert_eq!(serde_json::from_str::<Tif>("\"Gtc\"").unwrap(), Tif::Gtc);
        assert_eq!(serde_json::from_str::<Tif>("\"Ioc\"").unwrap(), Tif::Ioc);
        assert_eq!(serde_json::from_str::<Tif>("\"Alo\"").unwrap(), Tif::Alo);
    }

    #[test]
    fn tpsl_serde_wire_format() {
        assert_eq!(serde_json::to_string(&Tpsl::Sl).unwrap(), "\"sl\"");
        assert_eq!(serde_json::to_string(&Tpsl::Tp).unwrap(), "\"tp\"");

        assert_eq!(serde_json::from_str::<Tpsl>("\"sl\"").unwrap(), Tpsl::Sl);
        assert_eq!(serde_json::from_str::<Tpsl>("\"tp\"").unwrap(), Tpsl::Tp);
    }

    #[test]
    fn side_serde_wire_format() {
        assert_eq!(serde_json::to_string(&Side::Buy).unwrap(), "\"buy\"");
        assert_eq!(serde_json::to_string(&Side::Sell).unwrap(), "\"sell\"");

        assert_eq!(serde_json::from_str::<Side>("\"buy\"").unwrap(), Side::Buy);
        assert_eq!(
            serde_json::from_str::<Side>("\"sell\"").unwrap(),
            Side::Sell
        );
    }

    #[test]
    fn side_is_buy() {
        assert!(Side::Buy.is_buy());
        assert!(!Side::Sell.is_buy());
    }

    #[test]
    fn position_side_serde_wire_format() {
        assert_eq!(
            serde_json::to_string(&PositionSide::Long).unwrap(),
            "\"long\""
        );
        assert_eq!(
            serde_json::to_string(&PositionSide::Short).unwrap(),
            "\"short\""
        );

        assert_eq!(
            serde_json::from_str::<PositionSide>("\"long\"").unwrap(),
            PositionSide::Long
        );
        assert_eq!(
            serde_json::from_str::<PositionSide>("\"short\"").unwrap(),
            PositionSide::Short
        );
    }

    #[test]
    fn order_status_serde_wire_format() {
        assert_eq!(
            serde_json::to_string(&OrderStatus::Filled).unwrap(),
            "\"filled\""
        );
        assert_eq!(
            serde_json::to_string(&OrderStatus::Partial).unwrap(),
            "\"partial\""
        );
        assert_eq!(
            serde_json::to_string(&OrderStatus::Open).unwrap(),
            "\"open\""
        );
        assert_eq!(
            serde_json::to_string(&OrderStatus::TriggerSl).unwrap(),
            "\"trigger_sl\""
        );
        assert_eq!(
            serde_json::to_string(&OrderStatus::TriggerTp).unwrap(),
            "\"trigger_tp\""
        );

        assert_eq!(
            serde_json::from_str::<OrderStatus>("\"filled\"").unwrap(),
            OrderStatus::Filled
        );
        assert_eq!(
            serde_json::from_str::<OrderStatus>("\"trigger_sl\"").unwrap(),
            OrderStatus::TriggerSl
        );
    }

    #[test]
    fn display_impls() {
        assert_eq!(Side::Buy.to_string(), "buy");
        assert_eq!(Side::Sell.to_string(), "sell");
        assert_eq!(Tif::Gtc.to_string(), "Gtc");
        assert_eq!(Tpsl::Sl.to_string(), "sl");
        assert_eq!(Tpsl::Tp.to_string(), "tp");
        assert_eq!(PositionSide::Long.to_string(), "long");
        assert_eq!(PositionSide::Short.to_string(), "short");
        assert_eq!(OrderStatus::Filled.to_string(), "filled");
        assert_eq!(OrderStatus::TriggerSl.to_string(), "trigger_sl");
    }

    #[test]
    fn invalid_side_deserialization_fails() {
        assert!(serde_json::from_str::<Side>("\"BUY\"").is_err());
        assert!(serde_json::from_str::<Side>("\"Buy\"").is_err());
    }

    #[test]
    fn invalid_tif_deserialization_fails() {
        assert!(serde_json::from_str::<Tif>("\"gtc\"").is_err());
        assert!(serde_json::from_str::<Tif>("\"GTC\"").is_err());
    }
}
