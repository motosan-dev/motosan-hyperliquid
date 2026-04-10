# Production Safety Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden motosan-hyperliquid for real-money trading with CI/CD, key zeroing, error chain preservation, rate limiting, tracing, graceful shutdown, and config validation.

**Architecture:** Seven sequential tasks, each producing a self-contained commit. Tasks build on each other: error refactor (Task 3) introduces the `Config` variant used by validation (Task 7), and rate limiter (Task 4) and shutdown (Task 6) both modify `HyperliquidClient` fields. CI/CD (Task 1) and zeroize (Task 2) are fully independent.

**Tech Stack:** Rust, tokio, tokio-util (CancellationToken), zeroize, k256, tracing, GitHub Actions

**Spec:** `docs/superpowers/specs/2026-04-10-production-safety-design.md`

---

## File Structure

| File | Responsibility | Task |
|------|---------------|------|
| `.github/workflows/ci-rust.yml` | Create: CI pipeline | 1 |
| `.github/workflows/publish-rust.yml` | Create: Publish pipeline | 1 |
| `crates/hl-signing/Cargo.toml` | Modify: add zeroize dep + feature | 2 |
| `crates/hl-signing/src/private_key.rs` | Modify: add Drop impl | 2 |
| `crates/hl-types/src/error.rs` | Modify: refactor HlError variants | 3 |
| `crates/hl-client/src/client.rs` | Modify: error construction, rate limiter, shutdown, TCP keepalive, config validation | 3, 4, 5, 6, 7 |
| `crates/hl-client/src/retry.rs` | Modify: add RateLimitConfig, validate methods | 4, 7 |
| `crates/hl-client/src/rate_limit.rs` | Create: TokenBucket impl | 4 |
| `crates/hl-client/src/lib.rs` | Modify: export new modules/types | 4, 6 |
| `crates/hl-client/Cargo.toml` | Modify: add tokio-util dep | 6 |
| `crates/hl-executor/src/executor.rs` | Modify: update error construction, add tracing spans | 3, 5 |
| `crates/hl-market/src/market_data.rs` | Modify: update error construction, add tracing spans | 3, 5 |
| `crates/hl-account/src/account.rs` | Modify: update error construction, add tracing spans | 3, 5 |

---

### Task 1: CI/CD Pipeline

**Files:**
- Create: `.github/workflows/ci-rust.yml`
- Create: `.github/workflows/publish-rust.yml`

- [ ] **Step 1: Create CI workflow**

```yaml
# .github/workflows/ci-rust.yml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  check:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        rust: [stable, "1.75"]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.rust }}
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-features --all-targets -- -D warnings
      - run: cargo test
      - run: cargo test --features ws
```

- [ ] **Step 2: Create publish workflow**

```yaml
# .github/workflows/publish-rust.yml
name: Publish

on:
  push:
    tags: ["rust-v*"]
  workflow_dispatch:

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-features --all-targets -- -D warnings
      - run: cargo test --all-features
      - name: Publish crates in dependency order
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: |
          for crate in hl-types hl-signing hl-client hl-market hl-account hl-executor motosan-hyperliquid; do
            echo "Publishing $crate..."
            cargo publish -p "$crate" || true
            sleep 30
          done
```

- [ ] **Step 3: Verify workflows parse correctly**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci-rust.yml')); print('OK')"` (or `yq` if available)

If no yaml tool is available, verify by reading the files and checking indentation manually.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci-rust.yml .github/workflows/publish-rust.yml
git commit -m "ci: add CI and publish GitHub Actions workflows"
```

---

### Task 2: Private Key Zeroize

**Files:**
- Modify: `crates/hl-signing/Cargo.toml`
- Modify: `crates/hl-signing/src/private_key.rs`

- [ ] **Step 1: Write a test that PrivateKeySigner can be dropped without panic**

In `crates/hl-signing/src/private_key.rs`, add to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn drop_does_not_panic() {
    let signer = PrivateKeySigner::from_hex(TEST_KEY).unwrap();
    drop(signer);
    // If we reach here, Drop impl did not panic.
}
```

- [ ] **Step 2: Run test to verify it passes (baseline)**

Run: `cargo test -p hl-signing -- drop_does_not_panic -v`
Expected: PASS (no Drop impl yet, default drop is fine)

- [ ] **Step 3: Add zeroize dependency to hl-signing**

In `crates/hl-signing/Cargo.toml`, change:

```toml
# Before:
k256 = { version = "0.13", features = ["ecdsa"], optional = true }

# After:
k256 = { version = "0.13", features = ["ecdsa", "zeroize"], optional = true }
zeroize = { version = "1", optional = true }
```

Change the feature definition:

```toml
# Before:
k256-signer = ["dep:k256"]

# After:
k256-signer = ["dep:k256", "dep:zeroize"]
```

- [ ] **Step 4: Add Drop impl to PrivateKeySigner**

In `crates/hl-signing/src/private_key.rs`, add after the `impl Signer for PrivateKeySigner` block (before `#[cfg(test)]`):

```rust
impl Drop for PrivateKeySigner {
    fn drop(&mut self) {
        // SigningKey::drop() zeros the secret scalar when k256's `zeroize`
        // feature is enabled. No other secret material in this struct.
    }
}
```

- [ ] **Step 5: Run tests to verify everything still passes**

Run: `cargo test -p hl-signing -v`
Expected: All tests pass including `drop_does_not_panic`

- [ ] **Step 6: Commit**

```bash
git add crates/hl-signing/Cargo.toml crates/hl-signing/src/private_key.rs
git commit -m "security: zero private key material on drop via k256/zeroize"
```

---

### Task 3: Error Chain Refactor

**Files:**
- Modify: `crates/hl-types/src/error.rs`
- Modify: `crates/hl-client/src/client.rs`
- Modify: `crates/hl-executor/src/executor.rs`
- Modify: `crates/hl-market/src/market_data.rs`
- Modify: `crates/hl-account/src/account.rs`

- [ ] **Step 1: Write tests for new error behavior**

In `crates/hl-types/src/error.rs`, add these tests to the existing `mod tests` block:

```rust
#[test]
fn http_error_with_source_preserves_chain() {
    let source = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
    let err = HlError::Http {
        message: "connection failed".into(),
        source: Some(Box::new(source)),
    };
    assert!(err.source().is_some());
    assert!(format!("{err}").contains("connection failed"));
}

#[test]
fn http_error_without_source() {
    let err = HlError::Http {
        message: "generic failure".into(),
        source: None,
    };
    assert!(err.source().is_none());
}

#[test]
fn serialization_not_retryable() {
    let err = HlError::Serialization {
        message: "bad json".into(),
        source: None,
    };
    assert!(!err.is_retryable());
}

#[test]
fn config_error_not_retryable() {
    let err = HlError::Config("bad config".into());
    assert!(!err.is_retryable());
}

#[test]
fn config_error_display() {
    let err = HlError::Config("base_delay_ms must be > 0".into());
    assert_eq!(format!("{err}"), "invalid configuration: base_delay_ms must be > 0");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p hl-types -v`
Expected: FAIL — `HlError::Http` is still a tuple variant, and `Config` does not exist

- [ ] **Step 3: Refactor HlError in error.rs**

Replace the entire `HlError` enum and `impl` in `crates/hl-types/src/error.rs`:

```rust
use std::error::Error as StdError;

/// A boxed, thread-safe error source.
type BoxError = Box<dyn StdError + Send + Sync>;

/// Errors that can occur in Hyperliquid operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HlError {
    #[error("Signing error: {message}")]
    Signing {
        message: String,
        #[source]
        source: Option<BoxError>,
    },
    #[error("Serialization error: {message}")]
    Serialization {
        message: String,
        #[source]
        source: Option<BoxError>,
    },
    #[error("HTTP error: {message}")]
    Http {
        message: String,
        #[source]
        source: Option<BoxError>,
    },
    #[error("Timeout: {message}")]
    Timeout {
        message: String,
        #[source]
        source: Option<BoxError>,
    },
    #[error("WebSocket error: {message}")]
    WebSocket {
        message: String,
        #[source]
        source: Option<BoxError>,
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
    #[error("WebSocket reconnect cancelled")]
    WsCancelled,
    #[error("WebSocket reconnect failed after {attempts} attempts")]
    WsReconnectExhausted { attempts: u32 },
    #[error("Invalid configuration: {0}")]
    Config(String),
}
```

Update `is_retryable()` and `retry_after_ms()` — change all match arms from tuple to struct patterns:

```rust
impl HlError {
    pub fn is_retryable(&self) -> bool {
        match self {
            HlError::Http { .. } => true,
            HlError::Timeout { .. } => true,
            HlError::WebSocket { .. } => true,
            HlError::RateLimited { .. } => true,
            HlError::Api { status, .. } => *status >= 500,
            _ => false,
        }
    }

    pub fn retry_after_ms(&self) -> Option<u64> {
        match self {
            HlError::RateLimited { retry_after_ms, .. } => Some(*retry_after_ms),
            _ => None,
        }
    }

    /// Create an Http error without a source.
    pub fn http(message: impl Into<String>) -> Self {
        Self::Http { message: message.into(), source: None }
    }

    /// Create a Timeout error without a source.
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::Timeout { message: message.into(), source: None }
    }

    /// Create a Signing error without a source.
    pub fn signing(message: impl Into<String>) -> Self {
        Self::Signing { message: message.into(), source: None }
    }

    /// Create a Serialization error without a source.
    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Serialization { message: message.into(), source: None }
    }

    /// Create a WebSocket error without a source.
    pub fn websocket(message: impl Into<String>) -> Self {
        Self::WebSocket { message: message.into(), source: None }
    }
}
```

- [ ] **Step 4: Update all existing tests in error.rs**

Update every test that constructs `HlError` tuple variants to use the new struct syntax or convenience constructors. For example:

```rust
// Before:
assert!(HlError::Http("timeout".into()).is_retryable());
// After:
assert!(HlError::http("timeout").is_retryable());

// Before:
assert!(!HlError::Signing("key error".into()).is_retryable());
// After:
assert!(!HlError::signing("key error").is_retryable());

// Before:
assert!(!HlError::Serialization("serde fail".into()).is_retryable());
// After:
assert!(!HlError::serialization("serde fail").is_retryable());

// Before:
let err = HlError::Http("connection refused".into());
assert_eq!(format!("{err}"), "HTTP error: connection refused");
// After:
let err = HlError::http("connection refused");
assert_eq!(format!("{err}"), "HTTP error: connection refused");
```

Apply the same pattern to `Timeout`, `WebSocket` variants throughout the test module.

- [ ] **Step 5: Run hl-types tests**

Run: `cargo test -p hl-types -v`
Expected: All tests pass including the new `source` and `Config` tests

- [ ] **Step 6: Update error construction in hl-client/src/client.rs**

Every `.map_err(|e| HlError::Http(...))` and `.map_err(|e| HlError::Timeout(...))` must change to carry the source. Key changes in `post_once`:

```rust
// Line 205-211: reqwest send error
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

// Line 236-239: JSON parse failure — change from Http to Serialization
.map_err(|e| HlError::Serialization {
    message: "failed to parse exchange response as JSON".into(),
    source: Some(Box::new(e)),
})?;
```

Also update line 61 (client builder error) and line 117 (serialization error) and line 190 (retry exhausted):

```rust
// Line 61:
.map_err(|e| HlError::Http {
    message: format!("Failed to build HTTP client: {e}"),
    source: Some(Box::new(e)),
})?;

// Line 117:
HlError::Serialization {
    message: "payload is not a JSON object".into(),
    source: None,
}

// Line 190:
HlError::http("Retry loop exhausted without error")
```

- [ ] **Step 7: Update error construction in hl-executor, hl-market, hl-account, hl-signing**

Search all crates for `HlError::Http(`, `HlError::Signing(`, `HlError::Serialization(`, `HlError::Timeout(`, `HlError::WebSocket(` and replace with the convenience constructors (`HlError::http(...)`, `HlError::signing(...)`, etc.) or full struct syntax when a source is available.

Run: `grep -rn 'HlError::Http(' crates/` to find all sites. Apply the same pattern for each variant.

- [ ] **Step 8: Run full test suite**

Run: `cargo test`
Expected: All tests pass across all crates

- [ ] **Step 9: Run clippy**

Run: `cargo clippy --all-features --all-targets -- -D warnings`
Expected: Clean

- [ ] **Step 10: Commit**

```bash
git add crates/hl-types/src/error.rs crates/hl-client/src/client.rs \
  crates/hl-executor/src/executor.rs crates/hl-market/src/market_data.rs \
  crates/hl-account/src/account.rs crates/hl-signing/src/
git commit -m "fix: preserve error source chain in HlError, add Config variant

Refactor HlError variants from String to {message, source} structs.
Fix JSON parse failure misclassified as retryable Http (now Serialization).
Add convenience constructors: HlError::http(), signing(), etc."
```

---

### Task 4: Proactive Rate Limiter

**Files:**
- Create: `crates/hl-client/src/rate_limit.rs`
- Modify: `crates/hl-client/src/retry.rs`
- Modify: `crates/hl-client/src/client.rs`
- Modify: `crates/hl-client/src/lib.rs`

- [ ] **Step 1: Write tests for TokenBucket**

Create `crates/hl-client/src/rate_limit.rs`:

```rust
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

/// Token bucket rate limiter.
///
/// Refills tokens at a fixed rate. Callers must `acquire()` before sending
/// a request. If no tokens are available, `acquire()` sleeps until a token
/// is replenished.
pub(crate) struct TokenBucket {
    tokens: AtomicU32,
    max_tokens: u32,
    refill_interval: Duration,
    last_refill: Mutex<Instant>,
}

impl TokenBucket {
    pub(crate) fn new(max_rps: u32) -> Self {
        Self {
            tokens: AtomicU32::new(max_rps),
            max_tokens: max_rps,
            refill_interval: Duration::from_secs(1) / max_rps,
            last_refill: Mutex::new(Instant::now()),
        }
    }

    /// Refill tokens based on elapsed time since last refill.
    fn try_refill(&self) {
        let mut last = self.last_refill.lock().unwrap();
        let elapsed = last.elapsed();
        let new_tokens = (elapsed.as_millis() as u32) / (self.refill_interval.as_millis() as u32);
        if new_tokens > 0 {
            let current = self.tokens.load(Ordering::Relaxed);
            let refilled = (current + new_tokens).min(self.max_tokens);
            self.tokens.store(refilled, Ordering::Relaxed);
            *last = Instant::now();
        }
    }

    /// Acquire a token, sleeping if none are available.
    pub(crate) async fn acquire(&self) {
        loop {
            self.try_refill();
            let current = self.tokens.load(Ordering::Relaxed);
            if current > 0 {
                // Try to decrement. If another task beat us, retry.
                if self
                    .tokens
                    .compare_exchange(current, current - 1, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    return;
                }
            }
            // No token available — wait for one refill interval.
            tokio::time::sleep(self.refill_interval).await;
        }
    }
}

/// Configuration for proactive rate limiting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitConfig {
    /// Maximum requests per second. `None` = unlimited.
    pub max_rps: Option<u32>,
    /// Maximum concurrent in-flight requests. `None` = unlimited.
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

/// Internal rate limiter state held by [`HyperliquidClient`].
pub(crate) struct RateLimiter {
    pub(crate) bucket: Option<TokenBucket>,
    pub(crate) semaphore: Option<Semaphore>,
}

impl RateLimiter {
    pub(crate) fn from_config(config: &RateLimitConfig) -> Self {
        Self {
            bucket: config.max_rps.map(TokenBucket::new),
            semaphore: config.max_concurrent.map(Semaphore::new_for),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn token_bucket_allows_initial_burst() {
        let bucket = TokenBucket::new(5);
        // Should be able to acquire 5 tokens immediately.
        for _ in 0..5 {
            bucket.acquire().await;
        }
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
```

Note: `Semaphore::new_for` should be `Semaphore::new` — fix in the actual code:

```rust
pub(crate) fn from_config(config: &RateLimitConfig) -> Self {
    Self {
        bucket: config.max_rps.map(TokenBucket::new),
        semaphore: config.max_concurrent.map(|n| Semaphore::new(n as usize)),
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p hl-client -- rate_limit -v`
Expected: All 3 tests pass

- [ ] **Step 3: Add RateLimitConfig to client and wire up rate limiter**

In `crates/hl-client/src/client.rs`, add fields to `HyperliquidClient`:

```rust
use crate::rate_limit::{RateLimiter, RateLimitConfig};

pub struct HyperliquidClient {
    http: reqwest::Client,
    base_url: String,
    is_mainnet: bool,
    retry_config: RetryConfig,
    rate_limiter: RateLimiter,
}
```

Add `with_full_config` constructor:

```rust
pub fn with_full_config(
    is_mainnet: bool,
    retry_config: RetryConfig,
    timeout_config: TimeoutConfig,
    rate_limit_config: RateLimitConfig,
) -> Result<Self, HlError> {
    let base_url = Self::base_url_for(is_mainnet).to_string();
    let http = reqwest::Client::builder()
        .timeout(timeout_config.request_timeout)
        .connect_timeout(timeout_config.connect_timeout)
        .tcp_keepalive(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| HlError::Http {
            message: format!("Failed to build HTTP client: {e}"),
            source: Some(Box::new(e)),
        })?;
    let rate_limiter = RateLimiter::from_config(&rate_limit_config);
    Ok(Self { http, base_url, is_mainnet, retry_config, rate_limiter })
}
```

Update existing `with_config` to delegate:

```rust
pub fn with_config(
    is_mainnet: bool,
    retry_config: RetryConfig,
    timeout_config: TimeoutConfig,
) -> Result<Self, HlError> {
    Self::with_full_config(is_mainnet, retry_config, timeout_config, RateLimitConfig::default())
}
```

Wire rate limiter in `post_with_retry`:

```rust
async fn post_with_retry(
    &self,
    url: &str,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, HlError> {
    // Concurrency gate
    let _permit = match &self.rate_limiter.semaphore {
        Some(sem) => Some(sem.acquire().await.map_err(|_| HlError::http("semaphore closed"))?),
        None => None,
    };

    // Rate limit gate
    if let Some(bucket) = &self.rate_limiter.bucket {
        bucket.acquire().await;
    }

    // ... existing retry loop unchanged
```

- [ ] **Step 4: Update lib.rs exports**

In `crates/hl-client/src/lib.rs`, add:

```rust
pub mod rate_limit;
pub use rate_limit::RateLimitConfig;
```

- [ ] **Step 5: Run full test suite**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/hl-client/src/rate_limit.rs crates/hl-client/src/client.rs \
  crates/hl-client/src/lib.rs crates/hl-client/src/retry.rs
git commit -m "feat: add proactive token-bucket rate limiter and concurrency gate

New RateLimitConfig (default: 10 rps, 5 concurrent). Applied before
retry loop to prevent thundering herd on 429. Also adds TCP keepalive."
```

---

### Task 5: Tracing Spans

**Files:**
- Modify: `crates/hl-executor/src/executor.rs`
- Modify: `crates/hl-client/src/client.rs`
- Modify: `crates/hl-market/src/market_data.rs`
- Modify: `crates/hl-account/src/account.rs`

- [ ] **Step 1: Add tracing spans to hl-client**

In `crates/hl-client/src/client.rs`, update the two `#[tracing::instrument]` attributes:

```rust
// Before:
#[tracing::instrument(skip_all)]
pub async fn post_action(

// After:
#[tracing::instrument(skip(self, action, signature), fields(endpoint = "exchange"))]
pub async fn post_action(

// Before:
#[tracing::instrument(skip_all)]
pub async fn post_info(

// After:
#[tracing::instrument(skip(self, request), fields(endpoint = "info"))]
pub async fn post_info(
```

- [ ] **Step 2: Add tracing spans to hl-executor**

In `crates/hl-executor/src/executor.rs`, add `#[tracing::instrument]` to each public method:

```rust
#[tracing::instrument(skip(self, order), fields(asset = order.asset, is_buy = order.is_buy))]
pub async fn place_order(
    &self,
    order: OrderWire,
    vault: Option<&str>,
) -> Result<OrderResponse, HlError> {

#[tracing::instrument(skip(self), fields(asset, oid))]
pub async fn cancel_order(
    &self,
    asset: u32,
    oid: u64,
    vault: Option<&str>,
) -> Result<HlActionResponse, HlError> {

#[tracing::instrument(skip(self))]
pub async fn place_trigger_order(
    &self,
    symbol: &str,
    side: Side,
    size: Decimal,
    trigger_price: Decimal,
    tpsl: Tpsl,
    vault: Option<&str>,
) -> Result<OrderResponse, HlError> {

#[tracing::instrument(skip(self), fields(vault, amount = %amount))]
pub async fn transfer_to_vault(
    &self,
    vault: &str,
    amount: Decimal,
) -> Result<HlActionResponse, HlError> {
```

- [ ] **Step 3: Add tracing spans to hl-market**

In `crates/hl-market/src/market_data.rs`, add `#[tracing::instrument(skip(self))]` to each public method on `MarketData`:

```rust
#[tracing::instrument(skip(self))]
pub async fn candles(&self, coin: &str, interval: &str, limit: usize) -> ...

#[tracing::instrument(skip(self))]
pub async fn orderbook(&self, coin: &str) -> ...

#[tracing::instrument(skip(self))]
pub async fn mid_price(&self, coin: &str) -> ...

#[tracing::instrument(skip(self))]
pub async fn funding_rates(&self) -> ...

#[tracing::instrument(skip(self))]
pub async fn asset_info(&self) -> ...
```

- [ ] **Step 4: Add tracing spans to hl-account**

In `crates/hl-account/src/account.rs`, add `#[tracing::instrument(skip(self))]` to each public method on `Account`:

```rust
#[tracing::instrument(skip(self))]
pub async fn state(&self, address: &str) -> ...

#[tracing::instrument(skip(self))]
pub async fn positions(&self, address: &str) -> ...

#[tracing::instrument(skip(self))]
pub async fn fills(&self, address: &str) -> ...
```

- [ ] **Step 5: Run full test suite and clippy**

Run: `cargo test && cargo clippy --all-features --all-targets -- -D warnings`
Expected: All pass, clean clippy

- [ ] **Step 6: Commit**

```bash
git add crates/hl-executor/src/executor.rs crates/hl-client/src/client.rs \
  crates/hl-market/src/market_data.rs crates/hl-account/src/account.rs
git commit -m "feat: add tracing spans to all public SDK methods

Orders carry asset/side fields. Transport carries endpoint field.
Market/account methods carry coin/address fields. No secrets logged."
```

---

### Task 6: Graceful Shutdown

**Files:**
- Modify: `crates/hl-client/Cargo.toml`
- Modify: `crates/hl-client/src/client.rs`
- Modify: `crates/hl-client/src/lib.rs`

- [ ] **Step 1: Ensure tokio-util is a non-optional dependency**

In `crates/hl-client/Cargo.toml`, `tokio-util` is currently optional (ws-only). Add it as a required dependency. Change:

```toml
# Before (in [dependencies]):
tokio-util = { version = "0.7", optional = true }

# After — add a separate non-optional entry:
tokio-util = "0.7"
```

And update the `ws` feature to not include tokio-util (it's now always present):

```toml
ws = ["tokio-tungstenite", "futures-util", "rand"]
```

- [ ] **Step 2: Write test for shutdown behavior**

Add to `crates/hl-client/src/client.rs` test module:

```rust
#[test]
fn shutdown_token_is_not_cancelled_by_default() {
    let client = HyperliquidClient::testnet().unwrap();
    assert!(!client.shutdown_token().is_cancelled());
}

#[test]
fn shutdown_token_can_be_cancelled() {
    let client = HyperliquidClient::testnet().unwrap();
    let token = client.shutdown_token();
    token.cancel();
    assert!(token.is_cancelled());
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p hl-client -- shutdown -v`
Expected: FAIL — `shutdown_token` method does not exist yet

- [ ] **Step 4: Add CancellationToken to HyperliquidClient**

In `crates/hl-client/src/client.rs`:

```rust
use tokio_util::sync::CancellationToken;

pub struct HyperliquidClient {
    http: reqwest::Client,
    base_url: String,
    is_mainnet: bool,
    retry_config: RetryConfig,
    rate_limiter: RateLimiter,
    shutdown: CancellationToken,
}
```

Add `shutdown_token()` method:

```rust
/// Returns a clone of the shutdown token.
///
/// Cancel this token to reject new requests. In-flight requests will
/// complete normally.
pub fn shutdown_token(&self) -> CancellationToken {
    self.shutdown.clone()
}
```

Update all constructors to include `shutdown: CancellationToken::new()`.

Update `post_with_retry` to check shutdown:

```rust
async fn post_with_retry(
    &self,
    url: &str,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, HlError> {
    // Shutdown check
    if self.shutdown.is_cancelled() {
        return Err(HlError::Rejected {
            reason: "client is shutting down".into(),
        });
    }

    // Concurrency gate
    let _permit = match &self.rate_limiter.semaphore { ... };

    // Rate limit gate
    if let Some(bucket) = &self.rate_limiter.bucket { ... }

    // Retry loop
    for attempt in 0..=self.retry_config.max_retries {
        if attempt > 0 {
            let delay = ...;

            // Cancel-aware sleep
            tokio::select! {
                _ = tokio::time::sleep(delay) => {},
                _ = self.shutdown.cancelled() => {
                    return Err(HlError::Rejected {
                        reason: "client shutdown during retry backoff".into(),
                    });
                },
            }
        }
        // ... post_once unchanged
    }
    ...
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add crates/hl-client/Cargo.toml crates/hl-client/src/client.rs crates/hl-client/src/lib.rs
git commit -m "feat: add graceful shutdown via CancellationToken

New requests rejected after cancel. In-flight requests complete.
Retry backoff is cancel-aware via tokio::select!."
```

---

### Task 7: Config Validation

**Files:**
- Modify: `crates/hl-client/src/retry.rs`
- Modify: `crates/hl-client/src/rate_limit.rs`
- Modify: `crates/hl-client/src/client.rs`

- [ ] **Step 1: Write validation tests**

In `crates/hl-client/src/retry.rs`, add to the test module:

```rust
#[test]
fn validate_default_config_ok() {
    assert!(RetryConfig::default().validate().is_ok());
}

#[test]
fn validate_zero_base_delay_fails() {
    let config = RetryConfig { base_delay_ms: 0, ..Default::default() };
    assert!(config.validate().is_err());
}

#[test]
fn validate_zero_backoff_factor_fails() {
    let config = RetryConfig { backoff_factor: 0, ..Default::default() };
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
```

In `crates/hl-client/src/rate_limit.rs`, add to the test module:

```rust
#[test]
fn validate_default_rate_limit_ok() {
    assert!(RateLimitConfig::default().validate().is_ok());
}

#[test]
fn validate_zero_rps_fails() {
    let config = RateLimitConfig { max_rps: Some(0), max_concurrent: None };
    assert!(config.validate().is_err());
}

#[test]
fn validate_zero_concurrent_fails() {
    let config = RateLimitConfig { max_rps: None, max_concurrent: Some(0) };
    assert!(config.validate().is_err());
}

#[test]
fn validate_unlimited_ok() {
    let config = RateLimitConfig { max_rps: None, max_concurrent: None };
    assert!(config.validate().is_ok());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p hl-client -- validate -v`
Expected: FAIL — `validate()` methods don't exist yet

- [ ] **Step 3: Implement validate methods**

In `crates/hl-client/src/retry.rs`:

```rust
impl RetryConfig {
    pub fn validate(&self) -> Result<(), HlError> {
        if self.base_delay_ms == 0 {
            return Err(HlError::Config("base_delay_ms must be > 0".into()));
        }
        if self.backoff_factor == 0 {
            return Err(HlError::Config("backoff_factor must be > 0".into()));
        }
        Ok(())
    }
}

impl TimeoutConfig {
    pub fn validate(&self) -> Result<(), HlError> {
        if self.request_timeout.is_zero() {
            return Err(HlError::Config("request_timeout must be > 0".into()));
        }
        if self.connect_timeout.is_zero() {
            return Err(HlError::Config("connect_timeout must be > 0".into()));
        }
        if self.connect_timeout > self.request_timeout {
            return Err(HlError::Config("connect_timeout must be <= request_timeout".into()));
        }
        Ok(())
    }
}
```

In `crates/hl-client/src/rate_limit.rs`:

```rust
impl RateLimitConfig {
    pub fn validate(&self) -> Result<(), HlError> {
        if let Some(rps) = self.max_rps {
            if rps == 0 {
                return Err(HlError::Config("max_rps must be > 0 or None".into()));
            }
        }
        if let Some(n) = self.max_concurrent {
            if n == 0 {
                return Err(HlError::Config("max_concurrent must be > 0 or None".into()));
            }
        }
        Ok(())
    }
}
```

- [ ] **Step 4: Wire validation into client construction**

In `crates/hl-client/src/client.rs`, at the top of `with_full_config`:

```rust
pub fn with_full_config(
    is_mainnet: bool,
    retry_config: RetryConfig,
    timeout_config: TimeoutConfig,
    rate_limit_config: RateLimitConfig,
) -> Result<Self, HlError> {
    retry_config.validate()?;
    timeout_config.validate()?;
    rate_limit_config.validate()?;
    // ... rest of construction
}
```

- [ ] **Step 5: Run full test suite and clippy**

Run: `cargo test && cargo clippy --all-features --all-targets -- -D warnings`
Expected: All pass, clean clippy

- [ ] **Step 6: Commit**

```bash
git add crates/hl-client/src/retry.rs crates/hl-client/src/rate_limit.rs \
  crates/hl-client/src/client.rs
git commit -m "feat: validate config at construction time

RetryConfig, TimeoutConfig, RateLimitConfig all validated in
with_full_config(). Invalid values fail fast with HlError::Config."
```
