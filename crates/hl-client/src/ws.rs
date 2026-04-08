use futures_util::{SinkExt, StreamExt};
use hl_types::HlError;
use rand::Rng;

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

/// A simplified Hyperliquid WebSocket client with auto-reconnect and heartbeat.
pub struct HyperliquidWs {
    url: String,
    stream: Option<WsStream>,
    subscriptions: Vec<serde_json::Value>,
    reconnect_delay_ms: u64,
    heartbeat_interval: Option<tokio::time::Interval>,
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
        Self {
            url,
            stream: None,
            subscriptions: Vec::new(),
            reconnect_delay_ms: RECONNECT_BASE_DELAY_MS,
            heartbeat_interval: None,
        }
    }

    /// Connect (or reconnect) to the WebSocket server.
    ///
    /// On success, re-sends all previously registered subscriptions.
    pub async fn connect(&mut self) -> Result<(), HlError> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(&self.url)
            .await
            .map_err(|e| HlError::Http(format!("WebSocket connection failed: {e}")))?;

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
    pub async fn subscribe(&mut self, subscription: serde_json::Value) -> Result<(), HlError> {
        let msg = serde_json::json!({
            "method": "subscribe",
            "subscription": subscription,
        });

        self.subscriptions.push(msg.clone());
        self.send_raw(&msg).await
    }

    /// Read the next message from the WebSocket.
    ///
    /// Returns `None` if the connection is closed. On transient failures,
    /// attempts to reconnect with exponential backoff and jitter before
    /// resuming message delivery.
    pub async fn next_message(&mut self) -> Option<Result<serde_json::Value, HlError>> {
        loop {
            // Ensure we have a connection.
            if self.stream.is_none() {
                if let Err(e) = self.reconnect_with_backoff().await {
                    return Some(Err(e));
                }
            }

            let stream = self.stream.as_mut()?;
            let heartbeat = self.heartbeat_interval.as_mut();

            tokio::select! {
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
        let stream = self.stream.as_mut().ok_or_else(|| {
            HlError::Http("WebSocket not connected".to_string())
        })?;

        let text = msg.to_string();
        stream
            .send(tokio_tungstenite::tungstenite::Message::Text(text))
            .await
            .map_err(|e| HlError::Http(format!("WebSocket send failed: {e}")))
    }

    /// Reconnect with exponential backoff and jitter.
    async fn reconnect_with_backoff(&mut self) -> Result<(), HlError> {
        loop {
            let jitter = rand::thread_rng().gen_range(0..=self.reconnect_delay_ms / 4);
            let actual_delay = self.reconnect_delay_ms + jitter;

            tracing::info!(
                delay_ms = actual_delay,
                "waiting before WebSocket reconnect"
            );
            tokio::time::sleep(std::time::Duration::from_millis(actual_delay)).await;

            match self.connect().await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tracing::warn!(error = %e, "WebSocket reconnect failed, retrying");
                    self.reconnect_delay_ms =
                        (self.reconnect_delay_ms * 2).min(RECONNECT_MAX_DELAY_MS);
                }
            }
        }
    }
}
