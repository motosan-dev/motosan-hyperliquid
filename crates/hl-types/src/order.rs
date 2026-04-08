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
