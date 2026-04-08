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
