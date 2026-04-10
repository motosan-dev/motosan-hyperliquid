use futures_util::{SinkExt, StreamExt};
use hl_types::HlError;
use rand::Rng;
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;
use std::str::FromStr;
use tokio_util::sync::CancellationToken;

/// Mainnet WebSocket URL.
const WS_URL_MAINNET: &str = "wss://api.hyperliquid.xyz/ws";
/// Testnet WebSocket URL.
const WS_URL_TESTNET: &str = "wss://api.hyperliquid-testnet.xyz/ws";

/// Heartbeat interval in seconds.
const HEARTBEAT_INTERVAL_SECS: u64 = 30;
/// Base delay for reconnection backoff (1 second).
const RECONNECT_BASE_DELAY_MS: u64 = 1_000;
/// Maximum reconnection delay (30 seconds).
const RECONNECT_MAX_DELAY_MS: u64 = 30_000;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

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
    /// Each side is an array of `{"px": "<price>", "sz": "<size>"}` objects.
    pub levels: serde_json::Value,
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
    /// The candle object containing `t`, `o`, `h`, `l`, `c`, `v` fields.
    pub candle: serde_json::Value,
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

/// Data for a single order status change from the `orderUpdates` channel.
///
/// Pushed whenever an order is opened, filled, cancelled, or otherwise changes.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderUpdateData {
    /// The full order object including `oid`, `coin`, `side`, `limitPx`, `sz`.
    pub order: serde_json::Value,
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

/// Typed WebSocket message parsed from raw JSON.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WsMessage {
    AllMids(AllMidsData),
    L2Book(L2BookData),
    Trades(TradesData),
    Candle(CandleData),
    Bbo(BboData),
    OrderUpdates(Vec<OrderUpdateData>),
    UserEvents(UserEventsData),
    UserFills(UserFillsData),
    UserFundings(UserFundingsData),
    WebData3(WebData3Data),
    ClearinghouseState(ClearinghouseStateData),
    ActiveAssetCtx(ActiveAssetCtxData),
    ActiveAssetData(ActiveAssetDataMsg),
    UserTwapHistory(UserTwapHistoryData),
    UserTwapSliceFills(UserTwapSliceFillsData),
    SubscriptionResponse,
    Pong,
    Unknown(serde_json::Value),
}

impl WsMessage {
    /// Parse a raw JSON value into a typed [`WsMessage`].
    pub fn parse(value: serde_json::Value) -> Self {
        if value.get("method").and_then(|m| m.as_str()) == Some("pong") {
            return WsMessage::Pong;
        }

        let channel = value.get("channel").and_then(|c| c.as_str()).unwrap_or("");
        let data = value
            .get("data")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        match channel {
            "allMids" => {
                let mids_val = data.get("mids").cloned().unwrap_or(data.clone());
                let mut mids = HashMap::new();
                if let Some(obj) = mids_val.as_object() {
                    for (k, v) in obj {
                        if let Some(s) = v.as_str() {
                            if let Ok(d) = Decimal::from_str(s) {
                                mids.insert(k.clone(), d);
                            }
                        }
                    }
                }
                WsMessage::AllMids(AllMidsData { mids })
            }
            "l2Book" => WsMessage::L2Book(L2BookData {
                coin: data
                    .get("coin")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .into(),
                levels: data.get("levels").cloned().unwrap_or_default(),
                time: data.get("time").and_then(|t| t.as_u64()).unwrap_or(0),
            }),
            "trades" => {
                let raw_trades = data.as_array().cloned().unwrap_or_default();
                let coin = raw_trades
                    .first()
                    .and_then(|t| t.get("coin"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();
                let trades = raw_trades
                    .iter()
                    .filter_map(|t| {
                        let coin = t.get("coin")?.as_str()?.to_string();
                        let side = t.get("side")?.as_str()?.to_string();
                        let px = Decimal::from_str(t.get("px")?.as_str()?).ok()?;
                        let sz = Decimal::from_str(t.get("sz")?.as_str()?).ok()?;
                        let time = t.get("time").and_then(|v| v.as_u64()).unwrap_or(0);
                        let hash = t
                            .get("hash")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        Some(WsTrade {
                            coin,
                            side,
                            px,
                            sz,
                            time,
                            hash,
                        })
                    })
                    .collect();
                WsMessage::Trades(TradesData { coin, trades })
            }
            "candle" => WsMessage::Candle(CandleData {
                coin: data.get("s").and_then(|c| c.as_str()).unwrap_or("").into(),
                candle: data,
            }),
            "bbo" => {
                let coin = data
                    .get("coin")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();
                let parse_decimal = |key: &str| -> Decimal {
                    data.get(key)
                        .and_then(|v| v.as_str())
                        .and_then(|s| Decimal::from_str(s).ok())
                        .unwrap_or_default()
                };
                WsMessage::Bbo(BboData {
                    coin,
                    bid_px: parse_decimal("bidPx"),
                    bid_sz: parse_decimal("bidSz"),
                    ask_px: parse_decimal("askPx"),
                    ask_sz: parse_decimal("askSz"),
                    time: data.get("time").and_then(|t| t.as_u64()).unwrap_or(0),
                })
            }
            "orderUpdates" => {
                let updates = data
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .map(|item| OrderUpdateData {
                                order: item.get("order").cloned().unwrap_or_default(),
                                status: item
                                    .get("status")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("")
                                    .into(),
                                timestamp: item
                                    .get("statusTimestamp")
                                    .and_then(|t| t.as_u64())
                                    .unwrap_or(0),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                WsMessage::OrderUpdates(updates)
            }
            "user" => WsMessage::UserEvents(UserEventsData {
                events: data.as_array().cloned().unwrap_or_default(),
            }),
            "userFills" => WsMessage::UserFills(UserFillsData {
                user: data
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .into(),
                fills: data
                    .get("fills")
                    .and_then(|f| f.as_array())
                    .cloned()
                    .unwrap_or_default(),
            }),
            "userFundings" => WsMessage::UserFundings(UserFundingsData {
                user: data
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .into(),
                funding: data,
            }),
            "webData3" => WsMessage::WebData3(WebData3Data {
                user: data
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .into(),
                data: data.clone(),
            }),
            "clearinghouseState" => WsMessage::ClearinghouseState(ClearinghouseStateData {
                user: data
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .into(),
                data: data.clone(),
            }),
            "activeAssetCtx" => WsMessage::ActiveAssetCtx(ActiveAssetCtxData {
                coin: data
                    .get("coin")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .into(),
                ctx: data.get("ctx").cloned().unwrap_or_default(),
            }),
            "activeAssetData" => WsMessage::ActiveAssetData(ActiveAssetDataMsg {
                coin: data
                    .get("coin")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .into(),
                data: data.clone(),
            }),
            "userTwapHistory" => WsMessage::UserTwapHistory(UserTwapHistoryData {
                user: data
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .into(),
                history: data
                    .get("history")
                    .and_then(|h| h.as_array())
                    .cloned()
                    .unwrap_or_default(),
            }),
            "userTwapSliceFills" => WsMessage::UserTwapSliceFills(UserTwapSliceFillsData {
                user: data
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .into(),
                fills: data
                    .get("fills")
                    .and_then(|f| f.as_array())
                    .cloned()
                    .unwrap_or_default(),
            }),
            "subscriptionResponse" => WsMessage::SubscriptionResponse,
            "pong" => WsMessage::Pong,
            _ => WsMessage::Unknown(value),
        }
    }
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

/// A simplified Hyperliquid WebSocket client with auto-reconnect and heartbeat.
pub struct HyperliquidWs {
    url: String,
    stream: Option<WsStream>,
    subscriptions: Vec<serde_json::Value>,
    reconnect_delay_ms: u64,
    heartbeat_interval: Option<tokio::time::Interval>,
    config: WsConfig,
}

impl HyperliquidWs {
    /// Create a WebSocket client targeting mainnet.
    pub fn mainnet() -> Self {
        Self::new(WS_URL_MAINNET.to_string())
    }

    /// Create a WebSocket client targeting testnet.
    pub fn testnet() -> Self {
        Self::new(WS_URL_TESTNET.to_string())
    }

    /// Create a WebSocket client with a custom URL.
    pub fn new(url: String) -> Self {
        Self::with_config(url, WsConfig::default())
    }

    /// Create a WebSocket client with a custom URL and configuration.
    pub fn with_config(url: String, config: WsConfig) -> Self {
        Self {
            url,
            stream: None,
            subscriptions: Vec::new(),
            reconnect_delay_ms: RECONNECT_BASE_DELAY_MS,
            heartbeat_interval: None,
            config,
        }
    }

    /// Create a mainnet client with the given configuration.
    pub fn mainnet_with_config(config: WsConfig) -> Self {
        Self::with_config(WS_URL_MAINNET.to_string(), config)
    }

    /// Create a testnet client with the given configuration.
    pub fn testnet_with_config(config: WsConfig) -> Self {
        Self::with_config(WS_URL_TESTNET.to_string(), config)
    }

    /// Connect (or reconnect) to the WebSocket server.
    ///
    /// On success, re-sends all previously registered subscriptions.
    pub async fn connect(&mut self) -> Result<(), HlError> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(&self.url)
            .await
            .map_err(|e| HlError::WebSocket {
                message: format!("WebSocket connection failed: {e}"),
                source: Some(Box::new(e)),
            })?;

        tracing::info!(url = %self.url, "WebSocket connected");
        self.stream = Some(ws_stream);
        self.reconnect_delay_ms = RECONNECT_BASE_DELAY_MS;
        self.heartbeat_interval = Some(tokio::time::interval(std::time::Duration::from_secs(
            HEARTBEAT_INTERVAL_SECS,
        )));

        // Re-send all existing subscriptions after (re)connect.
        let subs = self.subscriptions.clone();
        for sub in &subs {
            self.send_raw(sub).await?;
        }

        Ok(())
    }

    /// Subscribe to a channel. The subscription is remembered so it will be
    /// re-sent automatically on reconnect.
    pub async fn subscribe_raw(&mut self, subscription: serde_json::Value) -> Result<(), HlError> {
        let msg = serde_json::json!({
            "method": "subscribe",
            "subscription": subscription,
        });

        self.subscriptions.push(msg.clone());
        self.send_raw(&msg).await
    }

    /// Subscribe to a typed channel. The subscription is serialized and
    /// forwarded to [`subscribe_raw`](Self::subscribe_raw).
    pub async fn subscribe_typed(&mut self, sub: Subscription) -> Result<(), HlError> {
        let value = serde_json::to_value(&sub).map_err(|e| {
            HlError::serialization(format!("failed to serialize subscription: {e}"))
        })?;
        self.subscribe_raw(value).await
    }

    /// Subscribe to the `allMids` channel.
    pub async fn subscribe_all_mids(&mut self) -> Result<(), HlError> {
        self.subscribe_typed(Subscription::AllMids).await
    }

    /// Subscribe to L2 book updates for a coin.
    pub async fn subscribe_l2_book(&mut self, coin: &str) -> Result<(), HlError> {
        self.subscribe_typed(Subscription::L2Book { coin: coin.into() })
            .await
    }

    /// Subscribe to trades for a coin.
    pub async fn subscribe_trades(&mut self, coin: &str) -> Result<(), HlError> {
        self.subscribe_typed(Subscription::Trades { coin: coin.into() })
            .await
    }

    /// Subscribe to candle updates for a coin and interval.
    pub async fn subscribe_candle(&mut self, coin: &str, interval: &str) -> Result<(), HlError> {
        self.subscribe_typed(Subscription::Candle {
            coin: coin.into(),
            interval: interval.into(),
        })
        .await
    }

    /// Subscribe to order updates for a user.
    pub async fn subscribe_order_updates(&mut self, user: &str) -> Result<(), HlError> {
        self.subscribe_typed(Subscription::OrderUpdates { user: user.into() })
            .await
    }

    /// Subscribe to user fills.
    pub async fn subscribe_user_fills(&mut self, user: &str) -> Result<(), HlError> {
        self.subscribe_typed(Subscription::UserFills { user: user.into() })
            .await
    }

    /// Subscribe to user events.
    pub async fn subscribe_user_events(&mut self, user: &str) -> Result<(), HlError> {
        self.subscribe_typed(Subscription::UserEvents { user: user.into() })
            .await
    }

    /// Subscribe to user fundings.
    pub async fn subscribe_user_fundings(&mut self, user: &str) -> Result<(), HlError> {
        self.subscribe_typed(Subscription::UserFundings { user: user.into() })
            .await
    }

    /// Read the next typed message from the WebSocket.
    ///
    /// This is a convenience wrapper around [`next_message`](Self::next_message)
    /// that parses the raw JSON into a [`WsMessage`].
    pub async fn next_typed_message(&mut self) -> Option<Result<WsMessage, HlError>> {
        let raw = self.next_message().await?;
        Some(raw.map(WsMessage::parse))
    }

    /// Read the next message from the WebSocket.
    ///
    /// Returns `None` if the connection is closed. On transient failures,
    /// attempts to reconnect with exponential backoff and jitter before
    /// resuming message delivery.
    ///
    /// If a `CancellationToken` is configured and cancelled, returns
    /// `Some(Err(HlError::WsCancelled))`. If `max_reconnect_attempts` is
    /// set and exhausted, returns `Some(Err(HlError::WsReconnectExhausted))`.
    pub async fn next_message(&mut self) -> Option<Result<serde_json::Value, HlError>> {
        loop {
            // Check cancellation before doing anything.
            if let Some(ref token) = self.config.cancellation_token {
                if token.is_cancelled() {
                    return Some(Err(HlError::WsCancelled));
                }
            }

            // Ensure we have a connection.
            if self.stream.is_none() {
                if let Err(e) = self.reconnect_with_backoff().await {
                    return Some(Err(e));
                }
            }

            let stream = self.stream.as_mut()?;
            let heartbeat = self.heartbeat_interval.as_mut();

            // Build the main select with optional cancellation.
            let cancel_fut = async {
                if let Some(ref token) = self.config.cancellation_token {
                    token.cancelled().await;
                } else {
                    std::future::pending::<()>().await;
                }
            };

            tokio::select! {
                _ = cancel_fut => {
                    return Some(Err(HlError::WsCancelled));
                }

                _ = async {
                    if let Some(hb) = heartbeat {
                        hb.tick().await;
                    } else {
                        // If no heartbeat interval, pend forever on this branch
                        std::future::pending::<()>().await;
                    }
                } => {
                    let ping = serde_json::json!({"method": "ping"}).to_string();
                    if let Err(e) = stream
                        .send(tokio_tungstenite::tungstenite::Message::Text(ping))
                        .await
                    {
                        tracing::warn!(error = %e, "heartbeat failed, reconnecting");
                        self.stream = None;
                        self.heartbeat_interval = None;
                        continue;
                    }
                }

                msg = stream.next() => {
                    match msg {
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(val) => return Some(Ok(val)),
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        raw = &text[..text.len().min(200)],
                                        "failed to parse WS message"
                                    );
                                    // Skip unparseable messages, continue reading
                                    continue;
                                }
                            }
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(data))) => {
                            let _ = stream
                                .send(tokio_tungstenite::tungstenite::Message::Pong(data))
                                .await;
                            continue;
                        }
                        Some(Ok(_)) => {
                            // Ignore binary, pong, close frames
                            continue;
                        }
                        Some(Err(e)) => {
                            tracing::error!(error = %e, "WebSocket read error, reconnecting");
                            self.stream = None;
                            self.heartbeat_interval = None;
                            continue;
                        }
                        None => {
                            tracing::warn!("WebSocket connection closed, reconnecting");
                            self.stream = None;
                            self.heartbeat_interval = None;
                            continue;
                        }
                    }
                }
            }
        }
    }

    /// Send a raw JSON message over the WebSocket.
    async fn send_raw(&mut self, msg: &serde_json::Value) -> Result<(), HlError> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| HlError::websocket("WebSocket not connected"))?;

        let text = msg.to_string();
        stream
            .send(tokio_tungstenite::tungstenite::Message::Text(text))
            .await
            .map_err(|e| HlError::WebSocket {
                message: format!("WebSocket send failed: {e}"),
                source: Some(Box::new(e)),
            })
    }

    /// Reconnect with exponential backoff and jitter.
    ///
    /// Respects `max_reconnect_attempts` and `cancellation_token` from
    /// the configured [`WsConfig`]. Returns an appropriate error if either
    /// limit is hit.
    async fn reconnect_with_backoff(&mut self) -> Result<(), HlError> {
        let mut attempts: u32 = 0;

        loop {
            // Check cancellation before waiting.
            if let Some(ref token) = self.config.cancellation_token {
                if token.is_cancelled() {
                    return Err(HlError::WsCancelled);
                }
            }

            // Check max attempts.
            if let Some(max) = self.config.max_reconnect_attempts {
                if attempts >= max {
                    return Err(HlError::WsReconnectExhausted { attempts });
                }
            }

            let jitter = rand::thread_rng().gen_range(0..=self.reconnect_delay_ms / 4);
            let actual_delay = self.reconnect_delay_ms + jitter;

            tracing::info!(
                delay_ms = actual_delay,
                attempt = attempts + 1,
                max_attempts = ?self.config.max_reconnect_attempts,
                "waiting before WebSocket reconnect"
            );

            // Wait with cancellation support.
            let sleep_fut = tokio::time::sleep(std::time::Duration::from_millis(actual_delay));
            if let Some(ref token) = self.config.cancellation_token {
                tokio::select! {
                    _ = token.cancelled() => {
                        return Err(HlError::WsCancelled);
                    }
                    _ = sleep_fut => {}
                }
            } else {
                sleep_fut.await;
            }

            attempts += 1;

            match self.connect().await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        attempt = attempts,
                        "WebSocket reconnect failed, retrying"
                    );
                    self.reconnect_delay_ms =
                        (self.reconnect_delay_ms * 2).min(RECONNECT_MAX_DELAY_MS);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_config_default_is_infinite_retries() {
        let config = WsConfig::default();
        assert!(config.max_reconnect_attempts.is_none());
        assert!(config.cancellation_token.is_none());
    }

    #[test]
    fn ws_config_with_max_attempts() {
        let config = WsConfig::with_max_attempts(5);
        assert_eq!(config.max_reconnect_attempts, Some(5));
        assert!(config.cancellation_token.is_none());
    }

    #[test]
    fn ws_config_with_cancellation_token() {
        let token = CancellationToken::new();
        let config = WsConfig::with_cancellation_token(token);
        assert!(config.cancellation_token.is_some());
        assert!(config.max_reconnect_attempts.is_none());
    }

    #[test]
    fn ws_config_builder_pattern() {
        let token = CancellationToken::new();
        let config = WsConfig::default()
            .max_reconnect_attempts(10)
            .cancellation_token(token);
        assert_eq!(config.max_reconnect_attempts, Some(10));
        assert!(config.cancellation_token.is_some());
    }

    #[test]
    fn new_uses_default_config() {
        let ws = HyperliquidWs::new("wss://example.com".to_string());
        assert!(ws.config.max_reconnect_attempts.is_none());
        assert!(ws.config.cancellation_token.is_none());
    }

    #[test]
    fn with_config_stores_config() {
        let config = WsConfig::with_max_attempts(3);
        let ws = HyperliquidWs::with_config("wss://example.com".to_string(), config);
        assert_eq!(ws.config.max_reconnect_attempts, Some(3));
    }

    #[test]
    fn mainnet_with_config_uses_mainnet_url() {
        let config = WsConfig::with_max_attempts(5);
        let ws = HyperliquidWs::mainnet_with_config(config);
        assert_eq!(ws.url, WS_URL_MAINNET);
        assert_eq!(ws.config.max_reconnect_attempts, Some(5));
    }

    #[test]
    fn testnet_with_config_uses_testnet_url() {
        let config = WsConfig::with_max_attempts(5);
        let ws = HyperliquidWs::testnet_with_config(config);
        assert_eq!(ws.url, WS_URL_TESTNET);
        assert_eq!(ws.config.max_reconnect_attempts, Some(5));
    }

    #[tokio::test]
    async fn reconnect_exhausted_with_max_attempts_zero() {
        // With max_reconnect_attempts = 0, reconnect should fail immediately.
        let config = WsConfig::with_max_attempts(0);
        let mut ws = HyperliquidWs::with_config("wss://127.0.0.1:1".to_string(), config);

        let err = ws.reconnect_with_backoff().await.unwrap_err();
        assert!(
            matches!(err, HlError::WsReconnectExhausted { attempts: 0 }),
            "expected WsReconnectExhausted with 0 attempts, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn reconnect_exhausted_with_max_attempts() {
        // With max_reconnect_attempts = 2, should try twice then fail.
        // Use a non-routable address so connect fails quickly.
        let config = WsConfig::with_max_attempts(2);
        let mut ws = HyperliquidWs::with_config("wss://127.0.0.1:1".to_string(), config);
        // Override delay to speed up the test.
        ws.reconnect_delay_ms = 1;

        let err = ws.reconnect_with_backoff().await.unwrap_err();
        assert!(
            matches!(err, HlError::WsReconnectExhausted { attempts: 2 }),
            "expected WsReconnectExhausted with 2 attempts, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn reconnect_cancelled_by_token() {
        let token = CancellationToken::new();
        let config = WsConfig::with_cancellation_token(token.clone());
        let mut ws = HyperliquidWs::with_config("wss://127.0.0.1:1".to_string(), config);

        // Cancel immediately.
        token.cancel();

        let err = ws.reconnect_with_backoff().await.unwrap_err();
        assert!(
            matches!(err, HlError::WsCancelled),
            "expected WsCancelled, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn reconnect_cancelled_during_backoff_sleep() {
        let token = CancellationToken::new();
        let config = WsConfig::with_cancellation_token(token.clone());
        let mut ws = HyperliquidWs::with_config("wss://127.0.0.1:1".to_string(), config);
        // Use a very long delay so we are guaranteed to be sleeping.
        ws.reconnect_delay_ms = 60_000;

        // No max attempts, so without cancellation it would loop forever.
        // Cancel after a short delay.
        let token_clone = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            token_clone.cancel();
        });

        let err = ws.reconnect_with_backoff().await.unwrap_err();
        assert!(
            matches!(err, HlError::WsCancelled),
            "expected WsCancelled, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn next_message_returns_cancelled_when_token_pre_cancelled() {
        let token = CancellationToken::new();
        token.cancel();

        let config = WsConfig::with_cancellation_token(token);
        let mut ws = HyperliquidWs::with_config("wss://127.0.0.1:1".to_string(), config);

        let result = ws.next_message().await;
        assert!(result.is_some());
        let err = result.unwrap().unwrap_err();
        assert!(
            matches!(err, HlError::WsCancelled),
            "expected WsCancelled, got: {err:?}"
        );
    }

    #[test]
    fn error_variants_not_retryable() {
        assert!(!HlError::WsCancelled.is_retryable());
        assert!(!HlError::WsReconnectExhausted { attempts: 5 }.is_retryable());
    }

    #[test]
    fn subscription_all_mids_serialization() {
        let sub = Subscription::AllMids;
        let json = serde_json::to_value(&sub).unwrap();
        assert_eq!(json, serde_json::json!({"type": "allMids"}));
    }

    #[test]
    fn subscription_l2_book_serialization() {
        let sub = Subscription::L2Book { coin: "BTC".into() };
        let json = serde_json::to_value(&sub).unwrap();
        assert_eq!(json, serde_json::json!({"type": "l2Book", "coin": "BTC"}));
    }

    #[test]
    fn subscription_trades_serialization() {
        let sub = Subscription::Trades { coin: "ETH".into() };
        let json = serde_json::to_value(&sub).unwrap();
        assert_eq!(json, serde_json::json!({"type": "trades", "coin": "ETH"}));
    }

    #[test]
    fn subscription_candle_serialization() {
        let sub = Subscription::Candle {
            coin: "BTC".into(),
            interval: "1h".into(),
        };
        let json = serde_json::to_value(&sub).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "candle", "coin": "BTC", "interval": "1h"})
        );
    }

    #[test]
    fn subscription_user_fills_serialization() {
        let sub = Subscription::UserFills {
            user: "0xABC".into(),
        };
        let json = serde_json::to_value(&sub).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "userFills", "user": "0xABC"})
        );
    }

    #[test]
    fn subscription_order_updates_serialization() {
        let sub = Subscription::OrderUpdates {
            user: "0xDEF".into(),
        };
        let json = serde_json::to_value(&sub).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "orderUpdates", "user": "0xDEF"})
        );
    }

    #[test]
    fn subscription_user_events_serialization() {
        let sub = Subscription::UserEvents {
            user: "0x123".into(),
        };
        let json = serde_json::to_value(&sub).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "userEvents", "user": "0x123"})
        );
    }

    #[test]
    fn subscription_bbo_serialization() {
        let sub = Subscription::Bbo { coin: "SOL".into() };
        let json = serde_json::to_value(&sub).unwrap();
        assert_eq!(json, serde_json::json!({"type": "bbo", "coin": "SOL"}));
    }

    #[test]
    fn error_display_messages() {
        let err = HlError::WsCancelled;
        assert_eq!(format!("{err}"), "WebSocket reconnect cancelled");

        let err = HlError::WsReconnectExhausted { attempts: 3 };
        assert_eq!(
            format!("{err}"),
            "WebSocket reconnect failed after 3 attempts"
        );
    }

    #[test]
    fn parse_l2_book_message() {
        let raw = serde_json::json!({
            "channel": "l2Book",
            "data": {
                "coin": "BTC",
                "levels": [[{"px":"90000","sz":"1.0"}],[{"px":"90001","sz":"0.5"}]],
                "time": 1_700_000_000_000u64
            }
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::L2Book(d) => {
                assert_eq!(d.coin, "BTC");
                assert_eq!(d.time, 1_700_000_000_000);
            }
            other => panic!("expected L2Book, got: {other:?}"),
        }
    }

    #[test]
    fn parse_user_fills_message() {
        let raw = serde_json::json!({
            "channel": "userFills",
            "data": {"user": "0xABC", "fills": [{"coin":"BTC"}]}
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::UserFills(d) => {
                assert_eq!(d.user, "0xABC");
                assert_eq!(d.fills.len(), 1);
            }
            other => panic!("expected UserFills, got: {other:?}"),
        }
    }

    #[test]
    fn parse_order_updates_message() {
        let raw = serde_json::json!({
            "channel": "orderUpdates",
            "data": [{"order": {"oid": 123}, "status": "filled", "statusTimestamp": 1_700_000_000_000u64}]
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::OrderUpdates(u) => {
                assert_eq!(u.len(), 1);
                assert_eq!(u[0].status, "filled");
            }
            other => panic!("expected OrderUpdates, got: {other:?}"),
        }
    }

    #[test]
    fn parse_subscription_response() {
        let raw = serde_json::json!({
            "channel": "subscriptionResponse",
            "data": {"method": "subscribe"}
        });
        assert!(matches!(
            WsMessage::parse(raw),
            WsMessage::SubscriptionResponse
        ));
    }

    #[test]
    fn parse_unknown_channel() {
        let raw = serde_json::json!({"channel": "futureChannel", "data": {}});
        assert!(matches!(WsMessage::parse(raw), WsMessage::Unknown(_)));
    }

    #[test]
    fn parse_malformed_returns_unknown() {
        let raw = serde_json::json!("just a string");
        assert!(matches!(WsMessage::parse(raw), WsMessage::Unknown(_)));
    }

    #[test]
    fn parse_all_mids_message() {
        let raw = serde_json::json!({
            "channel": "allMids",
            "data": {"mids": {"BTC": "90000", "ETH": "3000"}}
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::AllMids(data) => {
                assert_eq!(data.mids.len(), 2);
                assert_eq!(
                    *data.mids.get("BTC").unwrap(),
                    Decimal::from_str("90000").unwrap()
                );
                assert_eq!(
                    *data.mids.get("ETH").unwrap(),
                    Decimal::from_str("3000").unwrap()
                );
            }
            other => panic!("expected AllMids, got: {:?}", other),
        }
    }

    #[test]
    fn parse_all_mids_empty() {
        let raw = serde_json::json!({
            "channel": "allMids",
            "data": {"mids": {}}
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::AllMids(data) => assert!(data.mids.is_empty()),
            other => panic!("expected AllMids, got: {:?}", other),
        }
    }

    #[test]
    fn parse_all_mids_skips_unparseable_values() {
        let raw = serde_json::json!({
            "channel": "allMids",
            "data": {"mids": {"BTC": "90000", "BAD": "not_a_number"}}
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::AllMids(data) => {
                assert_eq!(data.mids.len(), 1);
                assert!(data.mids.contains_key("BTC"));
                assert!(!data.mids.contains_key("BAD"));
            }
            other => panic!("expected AllMids, got: {:?}", other),
        }
    }

    #[test]
    fn parse_trades_message() {
        let raw = serde_json::json!({
            "channel": "trades",
            "data": [
                {"coin": "ETH", "side": "B", "px": "3000.50", "sz": "1.2", "time": 1700000000000u64, "hash": "0xabc"},
                {"coin": "ETH", "side": "A", "px": "3001.00", "sz": "0.5", "time": 1700000000001u64, "hash": "0xdef"}
            ]
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::Trades(data) => {
                assert_eq!(data.coin, "ETH");
                assert_eq!(data.trades.len(), 2);
                assert_eq!(data.trades[0].coin, "ETH");
                assert_eq!(data.trades[0].side, "B");
                assert_eq!(data.trades[0].px, Decimal::from_str("3000.50").unwrap());
                assert_eq!(data.trades[0].sz, Decimal::from_str("1.2").unwrap());
                assert_eq!(data.trades[0].time, 1700000000000);
                assert_eq!(data.trades[0].hash, "0xabc");
                assert_eq!(data.trades[1].side, "A");
            }
            other => panic!("expected Trades, got: {:?}", other),
        }
    }

    #[test]
    fn parse_trades_skips_malformed_entries() {
        let raw = serde_json::json!({
            "channel": "trades",
            "data": [
                {"coin": "ETH", "side": "B", "px": "3000", "sz": "1.0", "time": 100u64, "hash": "0x1"},
                {"coin": "ETH", "bad_field": true}
            ]
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::Trades(data) => {
                assert_eq!(data.trades.len(), 1);
                assert_eq!(data.trades[0].px, Decimal::from_str("3000").unwrap());
            }
            other => panic!("expected Trades, got: {:?}", other),
        }
    }

    #[test]
    fn parse_trades_empty_array() {
        let raw = serde_json::json!({
            "channel": "trades",
            "data": []
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::Trades(data) => {
                assert_eq!(data.coin, "");
                assert!(data.trades.is_empty());
            }
            other => panic!("expected Trades, got: {:?}", other),
        }
    }

    #[test]
    fn parse_bbo_message() {
        let raw = serde_json::json!({
            "channel": "bbo",
            "data": {
                "coin": "SOL",
                "bidPx": "150.25",
                "bidSz": "100.0",
                "askPx": "150.30",
                "askSz": "50.0",
                "time": 1700000000000u64
            }
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::Bbo(data) => {
                assert_eq!(data.coin, "SOL");
                assert_eq!(data.bid_px, Decimal::from_str("150.25").unwrap());
                assert_eq!(data.bid_sz, Decimal::from_str("100.0").unwrap());
                assert_eq!(data.ask_px, Decimal::from_str("150.30").unwrap());
                assert_eq!(data.ask_sz, Decimal::from_str("50.0").unwrap());
                assert_eq!(data.time, 1700000000000);
            }
            other => panic!("expected Bbo, got: {:?}", other),
        }
    }

    #[test]
    fn parse_bbo_missing_fields_defaults_to_zero() {
        let raw = serde_json::json!({
            "channel": "bbo",
            "data": {"coin": "BTC"}
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::Bbo(data) => {
                assert_eq!(data.coin, "BTC");
                assert_eq!(data.bid_px, Decimal::default());
                assert_eq!(data.ask_px, Decimal::default());
                assert_eq!(data.time, 0);
            }
            other => panic!("expected Bbo, got: {:?}", other),
        }
    }

    #[test]
    fn parse_pong_message() {
        let raw = serde_json::json!({"channel": "pong"});
        assert!(matches!(WsMessage::parse(raw), WsMessage::Pong));
    }

    #[test]
    fn parse_pong_method_message() {
        let raw = serde_json::json!({"method": "pong"});
        assert!(matches!(WsMessage::parse(raw), WsMessage::Pong));
    }
}
