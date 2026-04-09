/// Errors that can occur in Hyperliquid operations.
#[derive(Debug, thiserror::Error)]
pub enum HlError {
    #[error("Signing error: {0}")]
    Signing(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("API error (HTTP {status}): {body}")]
    Api { status: u16, body: String },
    #[error("Invalid address: {0}")]
    InvalidAddress(String),
    #[error("Rate limited (429): retry after {retry_after_ms}ms")]
    RateLimited {
        retry_after_ms: u64,
        message: String,
    },
    #[error("Parse error: {0}")]
    Parse(String),
}

impl HlError {
    /// Returns `true` if the error is retryable (network timeout, 5xx, or 429).
    pub fn is_retryable(&self) -> bool {
        match self {
            HlError::Http(_) => true,
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
        assert!(HlError::Http("timeout".into()).is_retryable());
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
    fn not_retryable_signing() {
        assert!(!HlError::Signing("key error".into()).is_retryable());
    }

    #[test]
    fn not_retryable_parse() {
        assert!(!HlError::Parse("bad json".into()).is_retryable());
    }

    #[test]
    fn not_retryable_serialization() {
        assert!(!HlError::Serialization("serde fail".into()).is_retryable());
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
        assert_eq!(HlError::Http("x".into()).retry_after_ms(), None);
        assert_eq!(HlError::Signing("x".into()).retry_after_ms(), None);
        assert_eq!(HlError::Parse("x".into()).retry_after_ms(), None);
        assert_eq!(
            HlError::Api {
                status: 500,
                body: "x".into()
            }
            .retry_after_ms(),
            None
        );
    }

    #[test]
    fn error_display_formatting() {
        let err = HlError::Http("connection refused".into());
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
    }
}
