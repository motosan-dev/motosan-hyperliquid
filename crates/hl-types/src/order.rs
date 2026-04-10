use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Builder for constructing [`OrderWire`] instances.
///
/// Use the convenience constructors [`OrderWire::limit_buy`],
/// [`OrderWire::limit_sell`], [`OrderWire::trigger_buy`], or
/// [`OrderWire::trigger_sell`] to start building.
#[derive(Debug, Clone)]
pub struct OrderWireBuilder {
    asset: u32,
    is_buy: bool,
    limit_px: String,
    sz: String,
    reduce_only: bool,
    order_type: OrderTypeWire,
    cloid: Option<String>,
}

impl OrderWireBuilder {
    /// Set the time-in-force (only meaningful for limit orders).
    ///
    /// For trigger orders this is a no-op.
    pub fn tif(mut self, tif: Tif) -> Self {
        if let OrderTypeWire::Limit(ref mut limit) = self.order_type {
            limit.tif = tif;
        }
        self
    }

    /// Set the client order ID.
    pub fn cloid(mut self, cloid: impl Into<String>) -> Self {
        self.cloid = Some(cloid.into());
        self
    }

    /// Mark the order as reduce-only.
    pub fn reduce_only(mut self, reduce_only: bool) -> Self {
        self.reduce_only = reduce_only;
        self
    }

    /// Build the final [`OrderWire`].
    pub fn build(self) -> OrderWire {
        OrderWire {
            asset: self.asset,
            is_buy: self.is_buy,
            limit_px: self.limit_px,
            sz: self.sz,
            reduce_only: self.reduce_only,
            order_type: self.order_type,
            cloid: self.cloid,
        }
    }
}

impl OrderWire {
    /// Start building a limit buy order.
    ///
    /// Defaults to `Tif::Gtc`, `reduce_only = false`, no `cloid`.
    ///
    /// # Example
    ///
    /// ```
    /// use hl_types::{OrderWire, Tif};
    ///
    /// let order = OrderWire::limit_buy(0, "90000.0", "0.001")
    ///     .tif(Tif::Gtc)
    ///     .cloid("my-order-1")
    ///     .build();
    ///
    /// assert!(order.is_buy);
    /// assert_eq!(order.limit_px, "90000.0");
    /// ```
    pub fn limit_buy(
        asset: u32,
        limit_px: impl Into<String>,
        sz: impl Into<String>,
    ) -> OrderWireBuilder {
        OrderWireBuilder {
            asset,
            is_buy: true,
            limit_px: limit_px.into(),
            sz: sz.into(),
            reduce_only: false,
            order_type: OrderTypeWire::Limit(LimitOrderType { tif: Tif::Gtc }),
            cloid: None,
        }
    }

    /// Start building a limit sell order.
    ///
    /// Defaults to `Tif::Gtc`, `reduce_only = false`, no `cloid`.
    pub fn limit_sell(
        asset: u32,
        limit_px: impl Into<String>,
        sz: impl Into<String>,
    ) -> OrderWireBuilder {
        OrderWireBuilder {
            asset,
            is_buy: false,
            limit_px: limit_px.into(),
            sz: sz.into(),
            reduce_only: false,
            order_type: OrderTypeWire::Limit(LimitOrderType { tif: Tif::Gtc }),
            cloid: None,
        }
    }

    /// Start building a trigger buy order (e.g. stop-loss or take-profit).
    ///
    /// Trigger orders fire as market orders when the trigger price is hit.
    /// Defaults to `reduce_only = true`, no `cloid`.
    pub fn trigger_buy(
        asset: u32,
        trigger_px: impl Into<String>,
        sz: impl Into<String>,
        tpsl: Tpsl,
    ) -> OrderWireBuilder {
        let trigger_px = trigger_px.into();
        OrderWireBuilder {
            asset,
            is_buy: true,
            limit_px: trigger_px.clone(),
            sz: sz.into(),
            reduce_only: true,
            order_type: OrderTypeWire::Trigger(TriggerOrderType {
                trigger_px,
                is_market: true,
                tpsl,
            }),
            cloid: None,
        }
    }

    /// Start building a trigger sell order (e.g. stop-loss or take-profit).
    ///
    /// Trigger orders fire as market orders when the trigger price is hit.
    /// Defaults to `reduce_only = true`, no `cloid`.
    pub fn trigger_sell(
        asset: u32,
        trigger_px: impl Into<String>,
        sz: impl Into<String>,
        tpsl: Tpsl,
    ) -> OrderWireBuilder {
        let trigger_px = trigger_px.into();
        OrderWireBuilder {
            asset,
            is_buy: false,
            limit_px: trigger_px.clone(),
            sz: sz.into(),
            reduce_only: true,
            order_type: OrderTypeWire::Trigger(TriggerOrderType {
                trigger_px,
                is_market: true,
                tpsl,
            }),
            cloid: None,
        }
    }
}

/// Wire format for order type — either a limit order or a trigger order.
///
/// Serializes to the Hyperliquid wire format:
/// - Limit: `{"limit": {"tif": "Gtc"}}`
/// - Trigger: `{"trigger": {"triggerPx": "...", "isMarket": true, "tpsl": "sl"}}`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderTypeWire {
    /// A limit order with time-in-force.
    Limit(LimitOrderType),
    /// A trigger (stop-loss / take-profit) order.
    Trigger(TriggerOrderType),
}

impl OrderTypeWire {
    /// Returns `true` if this is a limit order.
    pub fn is_limit(&self) -> bool {
        matches!(self, OrderTypeWire::Limit(_))
    }

    /// Returns `true` if this is a trigger order.
    pub fn is_trigger(&self) -> bool {
        matches!(self, OrderTypeWire::Trigger(_))
    }
}

impl Serialize for OrderTypeWire {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            OrderTypeWire::Limit(limit) => {
                map.serialize_entry("limit", limit)?;
            }
            OrderTypeWire::Trigger(trigger) => {
                map.serialize_entry("trigger", trigger)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for OrderTypeWire {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OrderTypeWireVisitor;

        impl<'de> Visitor<'de> for OrderTypeWireVisitor {
            type Value = OrderTypeWire;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map with either a \"limit\" or \"trigger\" key")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let key: String = map
                    .next_key()?
                    .ok_or_else(|| de::Error::custom("empty order type object"))?;
                match key.as_str() {
                    "limit" => {
                        let limit: LimitOrderType = map.next_value()?;
                        Ok(OrderTypeWire::Limit(limit))
                    }
                    "trigger" => {
                        let trigger: TriggerOrderType = map.next_value()?;
                        Ok(OrderTypeWire::Trigger(trigger))
                    }
                    other => Err(de::Error::unknown_field(other, &["limit", "trigger"])),
                }
            }
        }

        deserializer.deserialize_map(OrderTypeWireVisitor)
    }
}

/// Limit order type wire format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitOrderType {
    pub tif: Tif,
}

/// Trigger order type wire format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerOrderType {
    pub trigger_px: String,
    pub is_market: bool,
    pub tpsl: Tpsl,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── OrderTypeWire enum serde ────────────────────────────────

    #[test]
    fn order_type_wire_limit_serialization() {
        let ot = OrderTypeWire::Limit(LimitOrderType { tif: Tif::Gtc });
        let json = serde_json::to_string(&ot).unwrap();
        assert_eq!(json, r#"{"limit":{"tif":"Gtc"}}"#);
    }

    #[test]
    fn order_type_wire_trigger_serialization() {
        let ot = OrderTypeWire::Trigger(TriggerOrderType {
            trigger_px: "99.0".into(),
            is_market: true,
            tpsl: Tpsl::Sl,
        });
        let json = serde_json::to_string(&ot).unwrap();
        assert_eq!(
            json,
            r#"{"trigger":{"triggerPx":"99.0","isMarket":true,"tpsl":"sl"}}"#
        );
    }

    #[test]
    fn order_type_wire_limit_roundtrip() {
        let original = OrderTypeWire::Limit(LimitOrderType { tif: Tif::Ioc });
        let json = serde_json::to_string(&original).unwrap();
        let parsed: OrderTypeWire = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn order_type_wire_trigger_roundtrip() {
        let original = OrderTypeWire::Trigger(TriggerOrderType {
            trigger_px: "50.5".into(),
            is_market: false,
            tpsl: Tpsl::Tp,
        });
        let json = serde_json::to_string(&original).unwrap();
        let parsed: OrderTypeWire = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn order_type_wire_is_limit_and_is_trigger() {
        let limit = OrderTypeWire::Limit(LimitOrderType { tif: Tif::Gtc });
        assert!(limit.is_limit());
        assert!(!limit.is_trigger());

        let trigger = OrderTypeWire::Trigger(TriggerOrderType {
            trigger_px: "1.0".into(),
            is_market: true,
            tpsl: Tpsl::Sl,
        });
        assert!(trigger.is_trigger());
        assert!(!trigger.is_limit());
    }

    #[test]
    fn order_type_wire_invalid_key_fails() {
        let json = r#"{"unknown":{"tif":"Gtc"}}"#;
        assert!(serde_json::from_str::<OrderTypeWire>(json).is_err());
    }

    #[test]
    fn order_type_wire_empty_object_fails() {
        let json = r#"{}"#;
        assert!(serde_json::from_str::<OrderTypeWire>(json).is_err());
    }

    // ── OrderWire builder ───────────────────────────────────────

    #[test]
    fn builder_limit_buy_defaults() {
        let order = OrderWire::limit_buy(1, "90000.0", "0.001").build();
        assert_eq!(order.asset, 1);
        assert!(order.is_buy);
        assert_eq!(order.limit_px, "90000.0");
        assert_eq!(order.sz, "0.001");
        assert!(!order.reduce_only);
        assert!(order.order_type.is_limit());
        assert!(order.cloid.is_none());
        if let OrderTypeWire::Limit(ref l) = order.order_type {
            assert_eq!(l.tif, Tif::Gtc);
        }
    }

    #[test]
    fn builder_limit_sell_with_options() {
        let order = OrderWire::limit_sell(5, "3000.0", "2.0")
            .tif(Tif::Ioc)
            .cloid("my-order-1")
            .reduce_only(true)
            .build();
        assert_eq!(order.asset, 5);
        assert!(!order.is_buy);
        assert_eq!(order.limit_px, "3000.0");
        assert_eq!(order.sz, "2.0");
        assert!(order.reduce_only);
        assert_eq!(order.cloid.as_deref(), Some("my-order-1"));
        if let OrderTypeWire::Limit(ref l) = order.order_type {
            assert_eq!(l.tif, Tif::Ioc);
        } else {
            panic!("expected limit order type");
        }
    }

    #[test]
    fn builder_trigger_buy() {
        let order = OrderWire::trigger_buy(0, "99.0", "10.0", Tpsl::Sl)
            .cloid("trigger-1")
            .build();
        assert_eq!(order.asset, 0);
        assert!(order.is_buy);
        assert!(order.reduce_only);
        assert!(order.order_type.is_trigger());
        if let OrderTypeWire::Trigger(ref t) = order.order_type {
            assert_eq!(t.trigger_px, "99.0");
            assert!(t.is_market);
            assert_eq!(t.tpsl, Tpsl::Sl);
        } else {
            panic!("expected trigger order type");
        }
    }

    #[test]
    fn builder_trigger_sell() {
        let order = OrderWire::trigger_sell(2, "150.0", "5.0", Tpsl::Tp)
            .reduce_only(false)
            .build();
        assert_eq!(order.asset, 2);
        assert!(!order.is_buy);
        assert!(!order.reduce_only); // overridden from default true
        assert!(order.order_type.is_trigger());
        if let OrderTypeWire::Trigger(ref t) = order.order_type {
            assert_eq!(t.trigger_px, "150.0");
            assert_eq!(t.tpsl, Tpsl::Tp);
        } else {
            panic!("expected trigger order type");
        }
    }

    #[test]
    fn builder_tif_noop_on_trigger() {
        // Calling .tif() on a trigger builder should not panic or change anything
        let order = OrderWire::trigger_buy(0, "99.0", "1.0", Tpsl::Sl)
            .tif(Tif::Ioc)
            .build();
        assert!(order.order_type.is_trigger());
    }

    // ── OrderWire serde (full struct) ───────────────────────────

    #[test]
    fn order_wire_limit_serde_roundtrip() {
        let order = OrderWire::limit_buy(1, "50000.0", "0.1")
            .cloid("test-cloid")
            .build();
        let json = serde_json::to_string(&order).unwrap();
        let parsed: OrderWire = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.asset, 1);
        assert!(parsed.is_buy);
        assert_eq!(parsed.limit_px, "50000.0");
        assert_eq!(parsed.sz, "0.1");
        assert!(!parsed.reduce_only);
        assert_eq!(parsed.cloid.as_deref(), Some("test-cloid"));
        assert!(parsed.order_type.is_limit());
    }

    #[test]
    fn order_wire_trigger_serde_roundtrip() {
        let order = OrderWire::trigger_buy(0, "100.0", "10.0", Tpsl::Tp).build();
        let json = serde_json::to_string(&order).unwrap();
        let parsed: OrderWire = serde_json::from_str(&json).unwrap();
        let trigger = match parsed.order_type {
            OrderTypeWire::Trigger(t) => t,
            _ => panic!("expected trigger"),
        };
        assert_eq!(trigger.trigger_px, "100.0");
        assert!(trigger.is_market);
        assert_eq!(trigger.tpsl, Tpsl::Tp);
    }

    #[test]
    fn order_wire_camel_case_serialization() {
        let order = OrderWire::limit_buy(0, "1.0", "1.0").build();
        let json = serde_json::to_string(&order).unwrap();
        assert!(json.contains("isBuy"));
        assert!(json.contains("limitPx"));
        assert!(json.contains("reduceOnly"));
        assert!(json.contains("orderType"));
        // cloid is None and skip_serializing_if, so should not appear
        assert!(!json.contains("cloid"));
    }

    #[test]
    fn order_wire_with_cloid_roundtrip() {
        let order = OrderWire::limit_sell(5, "3000.5", "2.0")
            .reduce_only(true)
            .cloid("my-order-123")
            .build();
        let json = serde_json::to_string(&order).unwrap();
        let parsed: OrderWire = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.cloid.as_deref(), Some("my-order-123"));
        assert!(parsed.reduce_only);
        assert!(!parsed.is_buy);
    }

    // ── Wire format backward compatibility ──────────────────────

    #[test]
    fn wire_format_limit_matches_hyperliquid() {
        // Hyperliquid expects: {"limit": {"tif": "Gtc"}}
        let ot = OrderTypeWire::Limit(LimitOrderType { tif: Tif::Gtc });
        let json = serde_json::to_value(&ot).unwrap();
        assert!(json.get("limit").is_some());
        assert_eq!(json["limit"]["tif"], "Gtc");
    }

    #[test]
    fn wire_format_trigger_matches_hyperliquid() {
        // Hyperliquid expects: {"trigger": {"triggerPx": "...", "isMarket": ..., "tpsl": "..."}}
        let ot = OrderTypeWire::Trigger(TriggerOrderType {
            trigger_px: "99.0".into(),
            is_market: true,
            tpsl: Tpsl::Sl,
        });
        let json = serde_json::to_value(&ot).unwrap();
        assert!(json.get("trigger").is_some());
        assert_eq!(json["trigger"]["triggerPx"], "99.0");
        assert_eq!(json["trigger"]["isMarket"], true);
        assert_eq!(json["trigger"]["tpsl"], "sl");
    }

    #[test]
    fn deserialize_from_hyperliquid_limit_json() {
        // Simulate what Hyperliquid would send back
        let json = r#"{"limit":{"tif":"Gtc"}}"#;
        let ot: OrderTypeWire = serde_json::from_str(json).unwrap();
        assert_eq!(ot, OrderTypeWire::Limit(LimitOrderType { tif: Tif::Gtc }));
    }

    #[test]
    fn deserialize_from_hyperliquid_trigger_json() {
        let json = r#"{"trigger":{"triggerPx":"99.0","isMarket":true,"tpsl":"sl"}}"#;
        let ot: OrderTypeWire = serde_json::from_str(json).unwrap();
        assert_eq!(
            ot,
            OrderTypeWire::Trigger(TriggerOrderType {
                trigger_px: "99.0".into(),
                is_market: true,
                tpsl: Tpsl::Sl,
            })
        );
    }

    // ── Existing enum serde tests (preserved) ───────────────────

    #[test]
    fn tif_serde_wire_format() {
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
