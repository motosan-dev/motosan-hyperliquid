//! Proactive token-bucket rate limiter and concurrency gate.
//!
//! Applied before the retry loop in [`super::client::HyperliquidClient`] to
//! prevent thundering-herd 429 responses.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// User-facing rate-limit configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitConfig {
    /// Maximum requests per second. `None` disables the token bucket.
    pub max_rps: Option<u32>,
    /// Maximum concurrent in-flight requests. `None` disables the semaphore.
    pub max_concurrent: Option<u32>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_rps: Some(10),
            max_concurrent: Some(5),
        }
    }
}

/// Simple token-bucket rate limiter using [`AtomicU32`] + [`Mutex<Instant>`].
pub(crate) struct TokenBucket {
    tokens: AtomicU32,
    max_tokens: u32,
    refill_interval: Duration,
    last_refill: Mutex<Instant>,
}

impl TokenBucket {
    /// Create a new token bucket that allows `max_rps` requests per second.
    pub(crate) fn new(max_rps: u32) -> Self {
        assert!(max_rps > 0, "max_rps must be positive");
        let refill_interval = Duration::from_secs(1) / max_rps;
        Self {
            tokens: AtomicU32::new(max_rps),
            max_tokens: max_rps,
            refill_interval,
            last_refill: Mutex::new(Instant::now()),
        }
    }

    /// Refill tokens based on elapsed time since the last refill.
    fn try_refill(&self) {
        let mut last = self.last_refill.lock().expect("lock poisoned");
        let now = Instant::now();
        let elapsed = now.duration_since(*last);
        if elapsed >= self.refill_interval {
            let new_tokens = (elapsed.as_nanos() / self.refill_interval.as_nanos()) as u32;
            if new_tokens > 0 {
                let current = self.tokens.load(Ordering::Relaxed);
                let refilled = current.saturating_add(new_tokens).min(self.max_tokens);
                self.tokens.store(refilled, Ordering::Relaxed);
                *last = now;
            }
        }
    }

    /// Wait until a token is available, then consume it.
    pub(crate) async fn acquire(&self) {
        loop {
            self.try_refill();
            let current = self.tokens.load(Ordering::Relaxed);
            if current > 0 {
                // CAS decrement
                if self
                    .tokens
                    .compare_exchange(current, current - 1, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
                {
                    return;
                }
                // CAS failed — another task grabbed it, retry immediately.
                continue;
            }
            // No tokens available — sleep for one refill interval and retry.
            tokio::time::sleep(self.refill_interval).await;
        }
    }
}

/// Internal rate limiter state combining token bucket and concurrency semaphore.
pub(crate) struct RateLimiter {
    pub(crate) bucket: Option<TokenBucket>,
    pub(crate) semaphore: Option<tokio::sync::Semaphore>,
}

impl RateLimiter {
    /// Build a [`RateLimiter`] from the user-facing configuration.
    pub(crate) fn from_config(config: &RateLimitConfig) -> Self {
        Self {
            bucket: config.max_rps.map(TokenBucket::new),
            semaphore: config
                .max_concurrent
                .map(|n| tokio::sync::Semaphore::new(n as usize)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn token_bucket_allows_initial_burst() {
        let bucket = TokenBucket::new(5);
        // Should be able to acquire all 5 tokens immediately.
        for _ in 0..5 {
            bucket.acquire().await;
        }
        // After exhausting the burst, tokens should be 0.
        assert_eq!(bucket.tokens.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.max_rps, Some(10));
        assert_eq!(config.max_concurrent, Some(5));
    }

    #[test]
    fn rate_limiter_from_unlimited_config() {
        let config = RateLimitConfig {
            max_rps: None,
            max_concurrent: None,
        };
        let limiter = RateLimiter::from_config(&config);
        assert!(limiter.bucket.is_none());
        assert!(limiter.semaphore.is_none());
    }
}
