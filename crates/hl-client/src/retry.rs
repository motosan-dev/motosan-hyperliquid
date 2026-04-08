use std::time::Duration;

/// Maximum delay cap to prevent unbounded waits (30 seconds).
const MAX_DELAY_MS: u64 = 30_000;

/// Default retry configuration: 3 attempts, 500ms base delay, 2x backoff.
const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_BASE_DELAY_MS: u64 = 500;
const DEFAULT_BACKOFF_FACTOR: u32 = 2;

/// Default overall request timeout (30 seconds).
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;
/// Default TCP connect timeout (10 seconds).
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Configuration for HTTP retry behavior with exponential backoff.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (excluding the initial request).
    pub max_retries: u32,
    /// Base delay before the first retry (in milliseconds).
    pub base_delay_ms: u64,
    /// Multiplier applied to the delay after each retry.
    pub backoff_factor: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            base_delay_ms: DEFAULT_BASE_DELAY_MS,
            backoff_factor: DEFAULT_BACKOFF_FACTOR,
        }
    }
}

impl RetryConfig {
    /// Compute the delay for the given attempt number (0-indexed).
    ///
    /// Uses saturating arithmetic to prevent overflow with custom configs.
    pub(crate) fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let exp = (self.backoff_factor as u64).saturating_pow(attempt);
        let delay_ms = self.base_delay_ms.saturating_mul(exp);
        Duration::from_millis(delay_ms.min(MAX_DELAY_MS))
    }
}

/// Configuration for HTTP request timeouts.
///
/// Controls both the overall request timeout (including response body transfer)
/// and the TCP connection timeout. Prevents the client from hanging indefinitely
/// when an exchange API becomes unresponsive.
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// Maximum time to wait for a complete response (default: 30s).
    pub request_timeout: Duration,
    /// Maximum time to wait for a TCP connection to be established (default: 10s).
    pub connect_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
            connect_timeout: Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS),
        }
    }
}
