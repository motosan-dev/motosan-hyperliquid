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
