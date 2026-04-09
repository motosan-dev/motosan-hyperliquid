use futures_util::{SinkExt, StreamExt};
use hl_types::HlError;
use rand::Rng;
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
            .map_err(|e| HlError::WebSocket(format!("WebSocket connection failed: {e}")))?;

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
            .ok_or_else(|| HlError::WebSocket("WebSocket not connected".to_string()))?;

        let text = msg.to_string();
        stream
            .send(tokio_tungstenite::tungstenite::Message::Text(text))
            .await
            .map_err(|e| HlError::WebSocket(format!("WebSocket send failed: {e}")))
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
    fn error_display_messages() {
        let err = HlError::WsCancelled;
        assert_eq!(format!("{err}"), "WebSocket reconnect cancelled");

        let err = HlError::WsReconnectExhausted { attempts: 3 };
        assert_eq!(
            format!("{err}"),
            "WebSocket reconnect failed after 3 attempts"
        );
    }
}
