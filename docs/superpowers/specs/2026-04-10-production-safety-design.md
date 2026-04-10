# Production Safety — Design Spec

> Harden motosan-hyperliquid for real-money trading: CI/CD, key zeroing, error chains, rate limiting, tracing, graceful shutdown, config validation.

## Context

The SDK's type system and architecture are solid after 18 merged issues. What is missing is the production safety infrastructure that protects real funds: automated testing gates, secret material handling, observable error chains, and controlled degradation under load or shutdown.

## Scope

Seven work items, ordered by priority:

1. CI/CD pipeline
2. Private key zeroize
3. Error chain refactor
4. Proactive rate limiter
5. Tracing spans
6. Graceful shutdown
7. Config validation + TCP keepalive

Out of scope: new trading features (batch orders, spot, WS user events), performance optimizations, DX improvements. Those are separate follow-up work.

---

## 1. CI/CD Pipeline

### ci-rust.yml

- **Trigger**: push to `main`, PRs targeting `main`
- **Matrix**: Rust stable + MSRV 1.75
- **Steps**:
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --all-features --all-targets -- -D warnings`
  3. `cargo test` (default features, unit tests only)
  4. `cargo test --all-features` excluding `live-test` feature (no testnet in CI)

### publish-rust.yml

- **Trigger**: push tag `rust-v*` OR `workflow_dispatch`
- **Steps**:
  1. Full validation (fmt + clippy + test)
  2. Publish crates in dependency order with 30s delay between each:
     `hl-types` → `hl-signing` → `hl-client` → `hl-market` → `hl-account` → `hl-executor` → `motosan-hyperliquid`
- **Secret**: `CARGO_REGISTRY_TOKEN`

### Decisions

- Live tests are **not** run in CI — testnet is unreliable and should not block merges.
- MSRV 1.75 is tested in the matrix to prevent accidental use of newer features.

---

## 2. Private Key Zeroize

### Problem

`PrivateKeySigner` holds a `k256::ecdsa::SigningKey`. When dropped, the 32-byte secret key remains in memory until the allocator reuses the page.

### Solution

Enable `k256`'s `zeroize` feature. `SigningKey` already implements `Zeroize + Drop` when that feature is on.

**hl-signing/Cargo.toml**:
```toml
k256 = { version = "0.13", features = ["ecdsa", "zeroize"], optional = true }
zeroize = { version = "1", optional = true }

[features]
k256-signer = ["k256", "zeroize"]
```

**private_key.rs** — add explicit `Drop` impl with a comment explaining that `SigningKey` handles its own zeroing:

```rust
impl Drop for PrivateKeySigner {
    fn drop(&mut self) {
        // SigningKey::drop() zeros the secret scalar (via k256/zeroize feature).
        // No other secret material in this struct.
    }
}
```

### Verification

- `PrivateKeySigner` must **not** derive `Debug` (already the case).
- `from_hex` accepts `&str` — callers should use `secrecy::Secret<String>` at their own API boundary if needed. The SDK does not enforce this.

---

## 3. Error Chain Refactor

### Problem

All `HlError` variants wrap errors as `String`, severing the `std::error::Error::source()` chain. A user cannot `downcast_ref` to inspect the original `reqwest::Error` or `serde_json::Error`.

Additionally, JSON parse failure in `client.rs` is misclassified as `HlError::Http` (retryable), when it should be a non-retryable serialization error.

### Solution

Change `String`-only variants to carry an optional boxed source error. This keeps `hl-types` free of `reqwest` dependency.

**New variant shape** (for Http, Timeout, Serialization, Signing):

```rust
#[error("HTTP error: {message}")]
Http {
    message: String,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
},
```

**New variant** for configuration errors:

```rust
#[error("invalid configuration: {0}")]
Config(String),
```

**Migration**: ~15-20 construction sites change from `HlError::Http(msg)` to `HlError::Http { message: msg, source: Some(Box::new(e)) }`. All `match` arms change from `HlError::Http(msg)` to `HlError::Http { message, .. }`.

**Bug fix**: In `client.rs`, JSON response parse failure changes from `HlError::Http` to `HlError::Serialization` with the `serde_json::Error` as source. This makes it non-retryable (correct behavior — the server returned non-JSON, retrying won't help).

### `is_retryable()` update

- `Http { .. }` — retryable (network failure)
- `Timeout { .. }` — retryable
- `Serialization { .. }` — **not** retryable (was incorrectly retryable before when misclassified as Http)
- `Config(..)` — not retryable
- All others unchanged.

---

## 4. Proactive Rate Limiter

### Problem

No global rate limiting. Multiple components sharing one client can burst concurrent requests, all get 429'd, and all retry simultaneously (thundering herd).

### Solution

Token bucket + concurrency semaphore inside `HyperliquidClient`, applied in `post_with_retry` before the retry loop.

**Config**:

```rust
pub struct RateLimitConfig {
    /// Maximum requests per second. None = unlimited.
    pub max_rps: Option<u32>,
    /// Maximum concurrent in-flight requests. None = unlimited.
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
```

**Implementation**: No external dependency. `TokenBucket` uses `AtomicU32` + `Mutex<Instant>` for refill tracking. `Semaphore` from `tokio::sync`.

**Placement in request flow**:

```
post_with_retry entry
  → check shutdown token
  → acquire concurrency permit (held for entire request lifecycle including retries)
  → acquire rate limit token
  → retry loop
    → post_once
    → on 429: respect Retry-After (no re-acquire needed)
    → on retry backoff: cancel-aware sleep
```

**Key decisions**:
- Rate limiter is acquired **once** before retry loop, not per-attempt.
- Concurrency permit is held across retries — prevents other requests from piling up while one is retrying.
- Shared via `Arc` naturally since `HyperliquidClient` is `Arc`-shared.

**API compatibility**: Existing `with_config(is_mainnet, retry, timeout)` keeps its 3-param signature and uses `RateLimitConfig::default()` internally. A new `with_full_config(is_mainnet, retry, timeout, rate_limit)` is added for users who need custom rate limiting. `mainnet()` and `testnet()` convenience constructors use all defaults.

---

## 5. Tracing Spans

### Problem

Only `post_action` and `post_info` have `#[tracing::instrument(skip_all)]`, discarding all context. An order submission cannot be correlated with its HTTP request in logs.

### Solution

Add `#[tracing::instrument]` with **key business fields** (not payloads or secrets) to:

**hl-executor** (write operations):
- `place_order` — fields: `asset`, `side`, `px`, `sz`
- `cancel_order` — fields: `asset`, `oid`
- `place_trigger_order` — fields: `symbol`, `side`, `size`, `tpsl`
- `transfer_to_vault` — fields: `vault`, `amount`

**hl-client** (transport):
- `post_action` — field: `endpoint = "exchange"`
- `post_info` — field: `endpoint = "info"`

**hl-market / hl-account** (read operations):
- `candles`, `orderbook`, `mid_price`, `funding_rates`, `asset_info` — field: `coin`
- `state`, `positions`, `fills` — field: `address`

### Result trace example

```
place_order{asset=0 side=buy px=90000 sz=0.001}
  └── post_action{endpoint=exchange}
```

### What is NOT instrumented

- Signature/private key fields — security risk
- Full JSON payloads — too noisy
- Internal parse functions — CPU-bound, span overhead not justified
- No mandatory tracing subscriber — SDK emits spans, user collects

---

## 6. Graceful Shutdown

### Problem

`OrderExecutor` has no shutdown mechanism. If a process receives SIGTERM during `place_order`, the HTTP request may complete on the exchange side but the response is lost — a "ghost order" the caller never learns about.

### Solution

Add `CancellationToken` to `HyperliquidClient` (transport layer). **Do not** cancel in-flight requests. Only block new requests from entering.

**Behavior**:
- Token cancelled → new calls to `post_with_retry` immediately return `Err(HlError::Rejected { reason: "client is shutting down" })`
- Already in-flight requests (inside `post_once`) → run to completion (receive response, learn order ID)
- In retry backoff sleep → cancel-aware `tokio::select!`, abort retry early

**API**:

```rust
impl HyperliquidClient {
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }
}
```

**User usage**:

```rust
let client = Arc::new(HyperliquidClient::mainnet()?);
let token = client.shutdown_token();

tokio::signal::ctrl_c().await?;
token.cancel();
// In-flight requests complete. New requests rejected.
```

### What is NOT done

- No "wait for all in-flight" — user controls this with `tokio::time::timeout`.
- No separate shutdown logic in `OrderExecutor` — it calls `post_action` which hits the transport-layer check.
- No mandatory CancellationToken — default token is never cancelled (opt-in).

---

## 7. Config Validation + TCP Keepalive

### TCP Keepalive

Hardcode `tcp_keepalive(Duration::from_secs(30))` and `pool_idle_timeout(Duration::from_secs(90))` in client builder. Not user-configurable — these are universally reasonable defaults. Prevents silent dead connections from intermediate proxies/NATs.

### Config Validation

Validate all config structs at `HyperliquidClient` construction time. Fail fast with `HlError::Config`.

**RetryConfig**:
- `base_delay_ms > 0`
- `backoff_factor > 0`

**TimeoutConfig**:
- `request_timeout > 0`
- `connect_timeout > 0`
- `connect_timeout <= request_timeout`

**RateLimitConfig**:
- `max_rps > 0` (if Some)
- `max_concurrent > 0` (if Some)

Each config struct gets a `validate() -> Result<(), HlError>` method. Called in `with_config` / `with_full_config` before building the reqwest client.

---

## Files Changed (Estimated)

| File | Changes |
|------|---------|
| `.github/workflows/ci-rust.yml` | New file |
| `.github/workflows/publish-rust.yml` | New file |
| `crates/hl-types/src/error.rs` | Refactor variants to carry `source`, add `Config` variant |
| `crates/hl-client/src/client.rs` | Rate limiter, shutdown token, TCP keepalive, config validation, fix JSON parse error classification |
| `crates/hl-client/src/transport.rs` | Update `HttpTransport` trait if needed for shutdown |
| `crates/hl-client/Cargo.toml` | Add `tokio-util` dep (for CancellationToken) |
| `crates/hl-signing/Cargo.toml` | Add `zeroize` dep, enable k256/zeroize |
| `crates/hl-signing/src/private_key.rs` | Add `Drop` impl |
| `crates/hl-executor/src/executor.rs` | Add tracing::instrument, update HlError construction sites |
| `crates/hl-market/src/market_data.rs` | Add tracing::instrument, update HlError construction sites |
| `crates/hl-account/src/account.rs` | Add tracing::instrument, update HlError construction sites |

---

## Testing Strategy

- **CI/CD**: Verify workflows run on a test PR before merge.
- **Zeroize**: Unit test that `PrivateKeySigner` can be dropped without panic. Cannot directly verify memory zeroing in a unit test — rely on k256's own test suite.
- **Error chain**: Unit tests that `err.source()` returns `Some` for wrapped errors. Test `is_retryable()` returns `false` for `Serialization`.
- **Rate limiter**: Unit test with mock transport — submit N requests concurrently, verify no more than `max_concurrent` are in-flight simultaneously.
- **Tracing**: Integration test with `tracing-test` crate — verify spans are emitted with expected fields.
- **Shutdown**: Unit test — cancel token, verify next `post_action` returns `Rejected`.
- **Config validation**: Unit tests for each invalid config combination.
