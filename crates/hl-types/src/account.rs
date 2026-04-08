use serde::{Deserialize, Serialize};

/// A position held on Hyperliquid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlPosition {
    /// The coin/asset symbol.
    pub coin: String,
    /// Position size (negative for short).
    pub size: f64,
    /// Average entry price.
    pub entry_px: f64,
    /// Unrealised PnL.
    pub unrealized_pnl: f64,
    /// Leverage used.
    pub leverage: f64,
    /// Liquidation price, if applicable.
    pub liquidation_px: Option<f64>,
}

/// A trade fill on Hyperliquid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlFill {
    /// The coin/asset symbol.
    pub coin: String,
    /// Fill price.
    pub px: f64,
    /// Fill size.
    pub sz: f64,
    /// Whether the fill was on the buy side.
    pub is_buy: bool,
    /// Timestamp in milliseconds.
    pub timestamp: u64,
    /// Fee paid.
    pub fee: f64,
    /// Realized PnL from closing a position (0.0 if this fill opened a position).
    pub closed_pnl: f64,
}

/// Snapshot of an account's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HlAccountState {
    /// Account equity.
    pub equity: f64,
    /// Available margin.
    pub margin_available: f64,
    /// Open positions.
    pub positions: Vec<HlPosition>,
}
