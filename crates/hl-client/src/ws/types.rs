use hl_types::HlCandle;
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

/// Typed subscription for Hyperliquid WebSocket channels.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[non_exhaustive]
pub enum Subscription {
    /// Subscribe to all mid-prices.
    AllMids,
    /// Subscribe to L2 orderbook updates for a coin.
    L2Book { coin: String },
    /// Subscribe to trades for a coin.
    Trades { coin: String },
    /// Subscribe to candle (OHLCV) updates for a coin and interval.
    Candle { coin: String, interval: String },
    /// Subscribe to best bid/offer updates for a coin.
    Bbo { coin: String },
    /// Subscribe to order updates for a user.
    OrderUpdates { user: String },
    /// Subscribe to all user events.
    UserEvents { user: String },
    /// Subscribe to user fill events.
    UserFills { user: String },
    /// Subscribe to user funding events.
    UserFundings { user: String },
    /// Subscribe to user non-funding ledger updates.
    UserNonFundingLedgerUpdates { user: String },
    /// Subscribe to notifications for a user.
    Notification { user: String },
    /// Subscribe to web data v2 for a user.
    WebData2 { user: String },
    /// Subscribe to web data v3 (aggregate user info).
    WebData3 { user: String },
    /// Subscribe to clearinghouse state updates.
    ClearinghouseState { user: String },
    /// Subscribe to active asset context.
    ActiveAssetCtx { coin: String },
    /// Subscribe to active asset data.
    ActiveAssetData { user: String, coin: String },
    /// Subscribe to user TWAP history.
    UserTwapHistory { user: String },
    /// Subscribe to user TWAP slice fills.
    UserTwapSliceFills { user: String },
}

/// A single price level in the L2 orderbook.
///
/// Each level represents an aggregate of orders at a given price.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceLevel {
    /// Price at this level.
    pub px: Decimal,
    /// Aggregate size at this level.
    pub sz: Decimal,
    /// Number of orders at this level.
    pub n: u32,
}

/// Data received from the `allMids` WebSocket channel.
///
/// Contains mid prices for all assets, updated on every trade or book change.
#[derive(Debug, Clone, PartialEq)]
pub struct AllMidsData {
    /// Mid prices keyed by coin symbol (e.g. `"BTC"` -> `Decimal(90000)`).
    /// Each value is the average of the best bid and best ask.
    pub mids: HashMap<String, Decimal>,
}

/// Data received from the `l2Book` WebSocket channel.
///
/// Provides an L2 orderbook snapshot for a single coin, containing bid and ask
/// price levels with their sizes.
#[derive(Debug, Clone, PartialEq)]
pub struct L2BookData {
    /// The coin symbol this orderbook belongs to (e.g. `"BTC"`).
    pub coin: String,
    /// Bid and ask levels as a two-element array: `[bids, asks]`.
    /// Each side is a `Vec<PriceLevel>` with price, size, and order count.
    pub levels: Vec<Vec<PriceLevel>>,
    /// Server-side timestamp in milliseconds since the Unix epoch.
    pub time: u64,
}

/// A single trade from the WebSocket `trades` channel.
///
/// Contains price, size, side, and metadata for one executed trade.
#[derive(Debug, Clone, PartialEq)]
pub struct WsTrade {
    /// The coin symbol (e.g. `"BTC"`).
    pub coin: String,
    /// Trade side: `"B"` for buy, `"A"` for sell.
    pub side: String,
    /// Execution price.
    pub px: Decimal,
    /// Execution size.
    pub sz: Decimal,
    /// Trade timestamp in milliseconds since the Unix epoch.
    pub time: u64,
    /// Transaction hash.
    pub hash: String,
}

/// Data received from the `trades` WebSocket channel.
///
/// Contains recent trades for a single coin, pushed in real time.
#[derive(Debug, Clone, PartialEq)]
pub struct TradesData {
    /// The coin symbol these trades belong to (e.g. `"ETH"`).
    pub coin: String,
    /// Individual typed trade objects.
    pub trades: Vec<WsTrade>,
}

/// Data received from the `candle` WebSocket channel.
///
/// Contains an OHLCV candle update for a coin at the subscribed interval.
#[derive(Debug, Clone, PartialEq)]
pub struct CandleData {
    /// The coin symbol this candle belongs to (e.g. `"BTC"`).
    pub coin: String,
    /// The OHLCV candle data.
    pub candle: HlCandle,
}

/// Data received from the `bbo` (best bid/offer) WebSocket channel.
///
/// Contains the current best bid and ask for a single coin.
#[derive(Debug, Clone, PartialEq)]
pub struct BboData {
    /// The coin symbol (e.g. `"SOL"`).
    pub coin: String,
    /// Best bid price.
    pub bid_px: Decimal,
    /// Best bid size.
    pub bid_sz: Decimal,
    /// Best ask price.
    pub ask_px: Decimal,
    /// Best ask size.
    pub ask_sz: Decimal,
    /// Server-side timestamp in milliseconds since the Unix epoch.
    pub time: u64,
}

/// A strongly-typed order snapshot from the `orderUpdates` WebSocket channel.
#[derive(Debug, Clone, PartialEq)]
pub struct WsOrderUpdate {
    /// Unique order ID assigned by the exchange.
    pub oid: u64,
    /// The coin symbol (e.g. `"BTC"`).
    pub coin: String,
    /// Trade side: `"B"` for buy, `"A"` for sell.
    pub side: String,
    /// Limit price of the order.
    pub limit_px: Decimal,
    /// Current remaining size.
    pub sz: Decimal,
    /// Original order size.
    pub orig_sz: Decimal,
    /// Optional client-assigned order ID.
    pub cloid: Option<String>,
}

/// Data for a single order status change from the `orderUpdates` channel.
///
/// Pushed whenever an order is opened, filled, cancelled, or otherwise changes.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderUpdateData {
    /// The strongly-typed order snapshot.
    pub order: WsOrderUpdate,
    /// The new order status (e.g. `"open"`, `"filled"`, `"canceled"`).
    pub status: String,
    /// Timestamp in milliseconds when this status change occurred.
    pub timestamp: u64,
}

/// Data received from the `user` (user events) WebSocket channel.
///
/// Aggregates fills, funding payments, liquidations, and ledger updates.
#[derive(Debug, Clone, PartialEq)]
pub struct UserEventsData {
    /// List of user event objects with type-specific fields.
    pub events: Vec<serde_json::Value>,
}

/// Data received from the `userFills` WebSocket channel.
///
/// Contains trade fill events for a specific user, pushed in real time.
#[derive(Debug, Clone, PartialEq)]
pub struct UserFillsData {
    /// The user's address.
    pub user: String,
    /// Individual fill objects containing `coin`, `px`, `sz`, `side`, `time`, `fee`.
    pub fills: Vec<serde_json::Value>,
}

/// Data received from the `userFundings` WebSocket channel.
///
/// Contains funding payment events for a specific user.
#[derive(Debug, Clone, PartialEq)]
pub struct UserFundingsData {
    /// The user's address.
    pub user: String,
    /// Funding payment details including `coin`, `usdc`, `szi`, `fundingRate`.
    pub funding: serde_json::Value,
}

/// Data received from the `webData3` WebSocket channel.
///
/// Contains aggregate user information including positions, balances, and orders.
#[derive(Debug, Clone, PartialEq)]
pub struct WebData3Data {
    /// The user's address.
    pub user: String,
    /// Aggregate user data payload.
    pub data: serde_json::Value,
}

/// Data received from the `clearinghouseState` WebSocket channel.
///
/// Contains margin and position state updates for a user.
#[derive(Debug, Clone, PartialEq)]
pub struct ClearinghouseStateData {
    /// The user's address.
    pub user: String,
    /// Clearinghouse state including margin summary and positions.
    pub data: serde_json::Value,
}

/// Data received from the `activeAssetCtx` WebSocket channel.
///
/// Contains asset context information such as funding rate and open interest.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveAssetCtxData {
    /// The coin symbol (e.g. `"BTC"`).
    pub coin: String,
    /// Asset context payload including funding, OI, and mark price.
    pub ctx: serde_json::Value,
}

/// Data received from the `activeAssetData` WebSocket channel.
///
/// Contains leverage and trade sizing limits for a user's asset.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveAssetDataMsg {
    /// The coin symbol.
    pub coin: String,
    /// Asset data payload with leverage and sizing details.
    pub data: serde_json::Value,
}

/// Data received from the `userTwapHistory` WebSocket channel.
///
/// Contains TWAP order execution history for a user.
#[derive(Debug, Clone, PartialEq)]
pub struct UserTwapHistoryData {
    /// The user's address.
    pub user: String,
    /// List of TWAP execution history entries.
    pub history: Vec<serde_json::Value>,
}

/// Data received from the `userTwapSliceFills` WebSocket channel.
///
/// Contains individual TWAP slice fill events for a user.
#[derive(Debug, Clone, PartialEq)]
pub struct UserTwapSliceFillsData {
    /// The user's address.
    pub user: String,
    /// Individual TWAP slice fill objects.
    pub fills: Vec<serde_json::Value>,
}

/// Configuration for WebSocket reconnection behavior.
#[derive(Debug, Clone, Default)]
pub struct WsConfig {
    /// Maximum number of reconnect attempts before giving up.
    /// `None` means infinite retries (the previous default behavior).
    pub max_reconnect_attempts: Option<u32>,
    /// An optional cancellation token that, when cancelled, stops the
    /// reconnect loop and returns an error.
    pub cancellation_token: Option<CancellationToken>,
}

impl WsConfig {
    /// Create a config with a maximum number of reconnect attempts.
    pub fn with_max_attempts(max_attempts: u32) -> Self {
        Self {
            max_reconnect_attempts: Some(max_attempts),
            ..Default::default()
        }
    }

    /// Create a config with a cancellation token.
    pub fn with_cancellation_token(token: CancellationToken) -> Self {
        Self {
            cancellation_token: Some(token),
            ..Default::default()
        }
    }

    /// Set the maximum number of reconnect attempts.
    pub fn max_reconnect_attempts(mut self, max: u32) -> Self {
        self.max_reconnect_attempts = Some(max);
        self
    }

    /// Set the cancellation token.
    pub fn cancellation_token(mut self, token: CancellationToken) -> Self {
        self.cancellation_token = Some(token);
        self
    }
}
