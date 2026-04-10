use std::time::Duration;

use hl_types::HlError;

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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    /// Validate that the retry configuration has sensible values.
    pub fn validate(&self) -> Result<(), HlError> {
        if self.base_delay_ms == 0 {
            return Err(HlError::Config("base_delay_ms must be > 0".into()));
        }
        if self.backoff_factor == 0 {
            return Err(HlError::Config("backoff_factor must be > 0".into()));
        }
        Ok(())
    }

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeoutConfig {
    /// Maximum time to wait for a complete response (default: 30s).
    pub request_timeout: Duration,
    /// Maximum time to wait for a TCP connection to be established (default: 10s).
    pub connect_timeout: Duration,
}

impl TimeoutConfig {
    /// Validate that the timeout configuration has sensible values.
    pub fn validate(&self) -> Result<(), HlError> {
        if self.request_timeout.is_zero() {
            return Err(HlError::Config("request_timeout must be > 0".into()));
        }
        if self.connect_timeout.is_zero() {
            return Err(HlError::Config("connect_timeout must be > 0".into()));
        }
        if self.connect_timeout > self.request_timeout {
            return Err(HlError::Config(
                "connect_timeout must be <= request_timeout".into(),
            ));
        }
        Ok(())
    }
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
            connect_timeout: Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 500);
        assert_eq!(config.backoff_factor, 2);
    }

    #[test]
    fn delay_for_attempt_zero() {
        let config = RetryConfig::default();
        let delay = config.delay_for_attempt(0);
        // 500 * 2^0 = 500ms
        assert_eq!(delay, Duration::from_millis(500));
    }

    #[test]
    fn delay_for_attempt_exponential() {
        let config = RetryConfig::default();
        // attempt 0: 500 * 1 = 500
        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(500));
        // attempt 1: 500 * 2 = 1000
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(1000));
        // attempt 2: 500 * 4 = 2000
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(2000));
    }

    #[test]
    fn delay_capped_at_max() {
        let config = RetryConfig {
            max_retries: 10,
            base_delay_ms: 1000,
            backoff_factor: 10,
        };
        let delay = config.delay_for_attempt(5);
        assert!(delay <= Duration::from_millis(30_000));
    }

    #[test]
    fn delay_no_overflow_with_large_attempt() {
        let config = RetryConfig::default();
        let delay = config.delay_for_attempt(100); // should not panic
        assert!(delay <= Duration::from_millis(30_000));
    }

    #[test]
    fn delay_no_overflow_with_large_base_and_factor() {
        let config = RetryConfig {
            max_retries: 5,
            base_delay_ms: u64::MAX,
            backoff_factor: u32::MAX,
        };
        let delay = config.delay_for_attempt(10); // should not panic
        assert!(delay <= Duration::from_millis(30_000));
    }

    #[test]
    fn default_timeout_config() {
        let config = TimeoutConfig::default();
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
    }

    #[test]
    fn validate_default_retry_ok() {
        assert!(RetryConfig::default().validate().is_ok());
    }

    #[test]
    fn validate_zero_base_delay_fails() {
        let config = RetryConfig {
            base_delay_ms: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_zero_backoff_factor_fails() {
        let config = RetryConfig {
            backoff_factor: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_default_timeout_ok() {
        assert!(TimeoutConfig::default().validate().is_ok());
    }

    #[test]
    fn validate_zero_request_timeout_fails() {
        let config = TimeoutConfig {
            request_timeout: Duration::ZERO,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_connect_exceeds_request_fails() {
        let config = TimeoutConfig {
            request_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(10),
        };
        assert!(config.validate().is_err());
    }
}
