/// Errors that can occur in Hyperliquid operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HlError {
    #[error("Signing error: {message}")]
    Signing {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    #[error("Serialization error: {message}")]
    Serialization {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    #[error("HTTP error: {message}")]
    Http {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    #[error("Timeout: {message}")]
    Timeout {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    #[error("WebSocket error: {message}")]
    WebSocket {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    #[error("API error (HTTP {status}): {body}")]
    Api { status: u16, body: String },
    #[error("Order rejected: {reason}")]
    Rejected { reason: String },
    #[error("Invalid address: {0}")]
    InvalidAddress(String),
    #[error("Rate limited (429): retry after {retry_after_ms}ms")]
    RateLimited {
        retry_after_ms: u64,
        message: String,
    },
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Config error: {0}")]
    Config(String),
    #[error("WebSocket reconnect cancelled")]
    WsCancelled,
    #[error("WebSocket reconnect failed after {attempts} attempts")]
    WsReconnectExhausted { attempts: u32 },
}

impl HlError {
    /// Create an `Http` error without an underlying source.
    pub fn http(message: impl Into<String>) -> Self {
        HlError::Http {
            message: message.into(),
            source: None,
        }
    }

    /// Create a `Timeout` error without an underlying source.
    pub fn timeout(message: impl Into<String>) -> Self {
        HlError::Timeout {
            message: message.into(),
            source: None,
        }
    }

    /// Create a `Signing` error without an underlying source.
    pub fn signing(message: impl Into<String>) -> Self {
        HlError::Signing {
            message: message.into(),
            source: None,
        }
    }

    /// Create a `Serialization` error without an underlying source.
    pub fn serialization(message: impl Into<String>) -> Self {
        HlError::Serialization {
            message: message.into(),
            source: None,
        }
    }

    /// Create a `WebSocket` error without an underlying source.
    pub fn websocket(message: impl Into<String>) -> Self {
        HlError::WebSocket {
            message: message.into(),
            source: None,
        }
    }

    /// Returns `true` if the error is retryable (network timeout, 5xx, or 429).
    pub fn is_retryable(&self) -> bool {
        match self {
            HlError::Http { .. } => true,
            HlError::Timeout { .. } => true,
            HlError::WebSocket { .. } => true,
            HlError::RateLimited { .. } => true,
            HlError::Api { status, .. } => {
                // Retryable if server error (5xx)
                *status >= 500
            }
            _ => false,
        }
    }

    /// If this is a `RateLimited` error, returns the suggested wait time in milliseconds.
    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            HlError::RateLimited { retry_after_ms, .. } => Some(*retry_after_ms),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_retryable_http_error() {
        assert!(HlError::http("timeout").is_retryable());
    }

    #[test]
    fn is_retryable_rate_limited() {
        assert!(HlError::RateLimited {
            retry_after_ms: 1000,
            message: "slow down".into()
        }
        .is_retryable());
    }

    #[test]
    fn is_retryable_api_5xx() {
        assert!(HlError::Api {
            status: 500,
            body: "internal error".into()
        }
        .is_retryable());
        assert!(HlError::Api {
            status: 502,
            body: "bad gateway".into()
        }
        .is_retryable());
        assert!(HlError::Api {
            status: 503,
            body: "unavailable".into()
        }
        .is_retryable());
    }

    #[test]
    fn not_retryable_api_4xx() {
        assert!(!HlError::Api {
            status: 400,
            body: "bad request".into()
        }
        .is_retryable());
        assert!(!HlError::Api {
            status: 404,
            body: "not found".into()
        }
        .is_retryable());
        assert!(!HlError::Api {
            status: 422,
            body: "unprocessable".into()
        }
        .is_retryable());
    }

    #[test]
    fn is_retryable_timeout() {
        assert!(HlError::timeout("request timed out").is_retryable());
    }

    #[test]
    fn is_retryable_websocket() {
        assert!(HlError::websocket("connection failed").is_retryable());
    }

    #[test]
    fn not_retryable_rejected() {
        assert!(!HlError::Rejected {
            reason: "order rejected".into()
        }
        .is_retryable());
    }

    #[test]
    fn not_retryable_signing() {
        assert!(!HlError::signing("key error").is_retryable());
    }

    #[test]
    fn not_retryable_parse() {
        assert!(!HlError::Parse("bad json".into()).is_retryable());
    }

    #[test]
    fn not_retryable_serialization() {
        assert!(!HlError::serialization("serde fail").is_retryable());
    }

    #[test]
    fn not_retryable_invalid_address() {
        assert!(!HlError::InvalidAddress("bad addr".into()).is_retryable());
    }

    #[test]
    fn retry_after_ms_rate_limited() {
        let err = HlError::RateLimited {
            retry_after_ms: 5000,
            message: "".into(),
        };
        assert_eq!(err.retry_after_ms(), Some(5000));
    }

    #[test]
    fn retry_after_ms_none_for_other_errors() {
        assert_eq!(HlError::http("x").retry_after_ms(), None);
        assert_eq!(HlError::timeout("x").retry_after_ms(), None);
        assert_eq!(HlError::websocket("x").retry_after_ms(), None);
        assert_eq!(HlError::signing("x").retry_after_ms(), None);
        assert_eq!(HlError::Parse("x".into()).retry_after_ms(), None);
        assert_eq!(
            HlError::Api {
                status: 500,
                body: "x".into()
            }
            .retry_after_ms(),
            None
        );
        assert_eq!(
            HlError::Rejected { reason: "x".into() }.retry_after_ms(),
            None
        );
    }

    #[test]
    fn error_display_formatting() {
        let err = HlError::http("connection refused");
        assert_eq!(format!("{err}"), "HTTP error: connection refused");

        let err = HlError::Api {
            status: 404,
            body: "not found".into(),
        };
        assert_eq!(format!("{err}"), "API error (HTTP 404): not found");

        let err = HlError::RateLimited {
            retry_after_ms: 2000,
            message: "slow".into(),
        };
        assert!(format!("{err}").contains("2000ms"));

        let err = HlError::timeout("request timed out");
        assert_eq!(format!("{err}"), "Timeout: request timed out");

        let err = HlError::websocket("connection failed");
        assert_eq!(format!("{err}"), "WebSocket error: connection failed");

        let err = HlError::Rejected {
            reason: "insufficient margin".into(),
        };
        assert_eq!(format!("{err}"), "Order rejected: insufficient margin");
    }

    #[test]
    fn http_error_with_source_preserves_chain() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused");
        let err = HlError::Http {
            message: "request failed".into(),
            source: Some(Box::new(io_err)),
        };
        assert!(
            std::error::Error::source(&err).is_some(),
            "source should be present when provided"
        );
    }

    #[test]
    fn http_error_without_source() {
        let err = HlError::http("no underlying cause");
        assert!(
            std::error::Error::source(&err).is_none(),
            "source should be None for convenience constructor"
        );
    }

    #[test]
    fn serialization_not_retryable() {
        let err = HlError::serialization("bad json");
        assert!(
            !err.is_retryable(),
            "Serialization errors should not be retryable"
        );
    }

    #[test]
    fn config_error_not_retryable() {
        let err = HlError::Config("invalid timeout".into());
        assert!(
            !err.is_retryable(),
            "Config errors should not be retryable"
        );
    }

    #[test]
    fn config_error_display() {
        let err = HlError::Config("missing API key".into());
        assert_eq!(format!("{err}"), "Config error: missing API key");
    }
}
