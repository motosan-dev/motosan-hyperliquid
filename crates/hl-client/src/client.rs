use std::time::Duration;

use async_trait::async_trait;
use hl_types::{HlError, Signature};

use crate::retry::{RetryConfig, TimeoutConfig};
use crate::transport::HttpTransport;

/// Mainnet REST API URL.
const MAINNET_API_URL: &str = "https://api.hyperliquid.xyz";
/// Testnet REST API URL.
const TESTNET_API_URL: &str = "https://api.hyperliquid-testnet.xyz";

/// Default Retry-After fallback when the header is missing on a 429 response.
const DEFAULT_RATE_LIMIT_WAIT_MS: u64 = 1_000;

/// Hyperliquid REST client.
///
/// Handles sending signed actions to the exchange API and querying
/// the info endpoint. Includes automatic retry with exponential backoff
/// for transient failures (timeouts, 5xx, 429).
pub struct HyperliquidClient {
    http: reqwest::Client,
    base_url: String,
    is_mainnet: bool,
    retry_config: RetryConfig,
}

impl HyperliquidClient {
    /// Create a new client for mainnet or testnet with default retry and timeout config.
    pub fn new(is_mainnet: bool) -> Result<Self, HlError> {
        Self::with_config(is_mainnet, RetryConfig::default(), TimeoutConfig::default())
    }

    /// Create a mainnet client with default configuration.
    pub fn mainnet() -> Result<Self, HlError> {
        Self::new(true)
    }

    /// Create a testnet client with default configuration.
    pub fn testnet() -> Result<Self, HlError> {
        Self::new(false)
    }

    /// Create a new client with a custom retry configuration and default timeouts.
    pub fn with_retry_config(is_mainnet: bool, retry_config: RetryConfig) -> Result<Self, HlError> {
        Self::with_config(is_mainnet, retry_config, TimeoutConfig::default())
    }

    /// Create a new client with custom retry and timeout configurations.
    pub fn with_config(
        is_mainnet: bool,
        retry_config: RetryConfig,
        timeout_config: TimeoutConfig,
    ) -> Result<Self, HlError> {
        let base_url = Self::base_url_for(is_mainnet).to_string();
        let http = reqwest::Client::builder()
            .timeout(timeout_config.request_timeout)
            .connect_timeout(timeout_config.connect_timeout)
            .build()
            .map_err(|e| HlError::Http {
                message: format!("Failed to build HTTP client: {e}"),
                source: Some(Box::new(e)),
            })?;
        Ok(Self {
            http,
            base_url,
            is_mainnet,
            retry_config,
        })
    }

    /// Returns the base URL for the given network.
    pub fn base_url_for(is_mainnet: bool) -> &'static str {
        if is_mainnet {
            MAINNET_API_URL
        } else {
            TESTNET_API_URL
        }
    }

    /// Whether this client targets mainnet.
    pub fn is_mainnet(&self) -> bool {
        self.is_mainnet
    }

    /// Generate a client order ID (cloid) for idempotent order submission.
    ///
    /// Uses UUID v4 formatted as a 128-bit hex string with `0x` prefix,
    /// which is the format Hyperliquid expects for the `cloid` field.
    pub fn generate_cloid() -> String {
        let id = uuid::Uuid::new_v4();
        format!("0x{}", id.as_simple())
    }

    /// Send a signed action to the exchange `/exchange` endpoint with retry.
    ///
    /// The payload includes the action, signature, nonce, and optional vault address.
    /// Retries on transient failures (network errors, 5xx, 429) with exponential backoff.
    #[tracing::instrument(skip_all)]
    pub async fn post_action(
        &self,
        action: serde_json::Value,
        signature: &Signature,
        nonce: u64,
        vault_address: Option<&str>,
    ) -> Result<serde_json::Value, HlError> {
        let mut payload = serde_json::json!({
            "action": action,
            "nonce": nonce,
            "signature": {
                "r": signature.r,
                "s": signature.s,
                "v": signature.v,
            },
        });

        if let Some(vault) = vault_address {
            let obj = payload.as_object_mut().ok_or_else(|| {
                HlError::serialization("payload is not a JSON object")
            })?;
            obj.insert(
                "vaultAddress".to_string(),
                serde_json::Value::String(vault.to_string()),
            );
        }

        let url = format!("{}/exchange", self.base_url);
        self.post_with_retry(&url, &payload).await
    }

    /// Query the info API at `/info` with retry.
    ///
    /// Used for read-only queries like clearinghouseState, meta, l2Book, etc.
    /// Retries on transient failures (network errors, 5xx, 429) with exponential backoff.
    #[tracing::instrument(skip_all)]
    pub async fn post_info(
        &self,
        request: serde_json::Value,
    ) -> Result<serde_json::Value, HlError> {
        let url = format!("{}/info", self.base_url);
        self.post_with_retry(&url, &request).await
    }

    /// Internal: POST a JSON payload to the given URL with exponential backoff retry.
    ///
    /// Only retries on:
    /// - Network / connection errors (reqwest send failure)
    /// - HTTP 5xx server errors
    /// - HTTP 429 rate-limit responses (respects Retry-After header)
    ///
    /// Does NOT retry on:
    /// - HTTP 4xx client errors (except 429)
    /// - Successful responses with API-level errors (e.g. order rejected)
    async fn post_with_retry(
        &self,
        url: &str,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value, HlError> {
        let mut last_error: Option<HlError> = None;

        for attempt in 0..=self.retry_config.max_retries {
            if attempt > 0 {
                let delay = if let Some(HlError::RateLimited { retry_after_ms, .. }) = &last_error {
                    Duration::from_millis(*retry_after_ms)
                } else {
                    self.retry_config.delay_for_attempt(attempt - 1)
                };

                tracing::warn!(
                    attempt = attempt,
                    delay_ms = delay.as_millis() as u64,
                    error = ?last_error,
                    url = %url,
                    "Retrying HTTP request after transient failure"
                );

                tokio::time::sleep(delay).await;
            }

            match self.post_once(url, payload).await {
                Ok(body) => return Ok(body),
                Err(e) if e.is_retryable() && attempt < self.retry_config.max_retries => {
                    last_error = Some(e);
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        // Should not reach here, but return last error if it does.
        Err(last_error
            .unwrap_or_else(|| HlError::http("Retry loop exhausted without error")))
    }

    /// Internal: perform a single POST request without retry.
    async fn post_once(
        &self,
        url: &str,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value, HlError> {
        let response = self
            .http
            .post(url)
            .json(payload)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    HlError::Timeout {
                        message: e.to_string(),
                        source: Some(Box::new(e)),
                    }
                } else {
                    HlError::Http {
                        message: e.to_string(),
                        source: Some(Box::new(e)),
                    }
                }
            })?;

        let status = response.status();

        // Handle 429 specially: extract Retry-After header before consuming the body.
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after_ms = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(|secs| secs * 1000)
                .unwrap_or(DEFAULT_RATE_LIMIT_WAIT_MS);

            let body_text = response
                .text()
                .await
                .unwrap_or_else(|_| "rate limited".to_string());

            return Err(HlError::RateLimited {
                retry_after_ms,
                message: body_text,
            });
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| HlError::Serialization {
                message: format!("Failed to parse response: {}", e),
                source: Some(Box::new(e)),
            })?;

        if !status.is_success() {
            return Err(HlError::Api {
                status: status.as_u16(),
                body: body.to_string(),
            });
        }

        Ok(body)
    }
}

#[async_trait]
impl HttpTransport for HyperliquidClient {
    async fn post_info(&self, request: serde_json::Value) -> Result<serde_json::Value, HlError> {
        self.post_info(request).await
    }

    async fn post_action(
        &self,
        action: serde_json::Value,
        signature: &Signature,
        nonce: u64,
        vault_address: Option<&str>,
    ) -> Result<serde_json::Value, HlError> {
        self.post_action(action, signature, nonce, vault_address)
            .await
    }

    fn is_mainnet(&self) -> bool {
        self.is_mainnet()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_url_mainnet() {
        assert_eq!(HyperliquidClient::base_url_for(true), MAINNET_API_URL);
    }

    #[test]
    fn test_base_url_testnet() {
        assert_eq!(HyperliquidClient::base_url_for(false), TESTNET_API_URL);
    }

    #[test]
    fn test_new_client_mainnet() {
        let client = HyperliquidClient::new(true).unwrap();
        assert!(client.is_mainnet());
        assert_eq!(client.base_url, MAINNET_API_URL);
    }

    #[test]
    fn test_new_client_testnet() {
        let client = HyperliquidClient::new(false).unwrap();
        assert!(!client.is_mainnet());
        assert_eq!(client.base_url, TESTNET_API_URL);
    }

    #[test]
    fn test_mainnet_convenience() {
        let client = HyperliquidClient::mainnet().unwrap();
        assert!(client.is_mainnet());
    }

    #[test]
    fn test_testnet_convenience() {
        let client = HyperliquidClient::testnet().unwrap();
        assert!(!client.is_mainnet());
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 500);
        assert_eq!(config.backoff_factor, 2);
    }

    #[test]
    fn test_retry_config_delay_for_attempt() {
        let config = RetryConfig::default();
        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(500));
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(1000));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(2000));
    }

    #[test]
    fn test_timeout_config_default() {
        let config = TimeoutConfig::default();
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
    }

    #[test]
    fn test_generate_cloid_format() {
        let cloid = HyperliquidClient::generate_cloid();
        assert!(cloid.starts_with("0x"), "cloid should start with 0x");
        assert_eq!(cloid.len(), 34, "cloid should be 34 chars (0x + 32 hex)");
        assert!(
            cloid[2..].chars().all(|c| c.is_ascii_hexdigit()),
            "cloid should contain only hex characters after 0x"
        );
    }

    #[test]
    fn test_generate_cloid_uniqueness() {
        let a = HyperliquidClient::generate_cloid();
        let b = HyperliquidClient::generate_cloid();
        assert_ne!(a, b, "two generated cloids should be unique");
    }
}
