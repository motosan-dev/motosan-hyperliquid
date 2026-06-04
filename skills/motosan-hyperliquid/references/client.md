# Client

## Create Client

```rust
use hl_client::HyperliquidClient;

let mainnet = HyperliquidClient::mainnet()?;
let testnet = HyperliquidClient::testnet()?;
```

## Custom Configuration

```rust
use hl_client::{HyperliquidClient, RateLimitConfig, RetryConfig, TimeoutConfig};
use std::time::Duration;

let client = HyperliquidClient::with_full_config(
    true, // is_mainnet
    RetryConfig { max_retries: 5, base_delay_ms: 1000, backoff_factor: 2 },
    TimeoutConfig {
        request_timeout: Duration::from_secs(60),
        connect_timeout: Duration::from_secs(15),
    },
    RateLimitConfig::default(),
)?;
```

`with_config(is_mainnet, retry, timeout)` uses default rate limits. Constructors validate config and return `HlError::Config` on invalid values.

## Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `is_mainnet` | — | Target mainnet (`true`) or testnet (`false`). |
| `RetryConfig::max_retries` | `3` | Maximum retry attempts on transient failures. |
| `RetryConfig::base_delay_ms` | `500` | Base delay before first retry. |
| `RetryConfig::backoff_factor` | `2` | Exponential backoff multiplier. |
| `TimeoutConfig::request_timeout` | `30s` | Overall HTTP request timeout. |
| `TimeoutConfig::connect_timeout` | `10s` | TCP connection timeout. |
| `RateLimitConfig` | default token bucket / concurrency gate | Proactive client-side rate limiting. |

Retry applies to HTTP 429, HTTP 5xx, connection errors, and timeouts.

## Info API

```rust
let resp = client.post_info(serde_json::json!({
    "type": "l2Book",
    "coin": "BTC",
})).await?;
```

## Exchange API

```rust
let resp = client.post_action(
    action,
    &signature,
    nonce,
    None,          // vault_address
    expires_after, // Option<u64>; unix epoch ms, sent as expiresAfter
).await?;
```

When `expires_after` is `Some`, the same value must have been included in the L1 action signature hash (`sign_l1_action_with_expiry`). `OrderExecutor::set_expires_after` handles this automatically.

## Client Order ID

```rust
let cloid = HyperliquidClient::generate_cloid(); // 0x + 32 hex chars
```

## Graceful Shutdown

```rust
let token = client.shutdown_token();
token.cancel(); // new requests fail and retry backoffs are interrupted
```

## WebSocket (feature: `ws`)

```toml
hl-client = { version = "0.3.0", features = ["ws"] }
```

Raw JSON:

```rust
use hl_client::HyperliquidWs;

let mut ws = HyperliquidWs::mainnet();
ws.connect().await?;
ws.subscribe(serde_json::json!({"type": "l2Book", "coin": "BTC"})).await?;

while let Some(msg) = ws.next_message().await {
    println!("{:?}", msg?);
}
```

Typed subscriptions:

```rust
use hl_client::{HyperliquidWs, Subscription};

let mut ws = HyperliquidWs::mainnet();
ws.connect().await?;
ws.subscribe_typed(Subscription::L2Book { coin: "BTC".into() }).await?;
// or ws.subscribe_l2_book("BTC").await?;

while let Some(msg) = ws.next_typed_message().await {
    println!("{:?}", msg?);
}
```

WebSocket includes heartbeat, auto-reconnect with backoff/jitter, and subscription replay after reconnecting.
