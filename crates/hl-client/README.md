# hl-client

> Hyperliquid REST and WebSocket client with retry, exponential backoff, rate limiting, concurrency gating, and optional typed WebSocket support.

## Overview

`hl-client` provides `HyperliquidClient` for REST API communication with Hyperliquid. It handles:

- automatic retry with exponential backoff for transient failures (network errors, 5xx, 429)
- proactive token-bucket rate limiting and a concurrency gate
- `Retry-After` handling for 429 responses
- configurable request/connect timeouts
- graceful shutdown via `CancellationToken`
- client order ID generation (`generate_cloid`) for idempotent order submission
- optional `ws` feature for typed WebSocket subscriptions

## Usage

### Create a Client

```rust
use hl_client::{HyperliquidClient, RateLimitConfig, RetryConfig, TimeoutConfig};
use std::time::Duration;

let client = HyperliquidClient::mainnet()?;
let testnet = HyperliquidClient::testnet()?;

let custom = HyperliquidClient::with_config(
    true,
    RetryConfig { max_retries: 5, base_delay_ms: 1000, backoff_factor: 2 },
    TimeoutConfig {
        request_timeout: Duration::from_secs(60),
        connect_timeout: Duration::from_secs(15),
    },
)?;
```

### Query the Info API

```rust
let response = client.post_info(serde_json::json!({
    "type": "l2Book",
    "coin": "BTC",
})).await?;
```

### Submit a Signed Action

```rust
use hl_types::Signature;

let response = client.post_action(
    action_json,
    &signature,
    nonce,
    None,        // vault_address
    expires_after, // Option<u64>, unix epoch ms; sent as `expiresAfter` when Some
).await?;
```

`expires_after` must match the value included in the L1 signature hash (for example via `hl_signing::sign_l1_action_with_expiry`). `hl-executor` manages this automatically when using `OrderExecutor::set_expires_after`.

## WebSocket (opt-in)

Enable with `features = ["ws"]`:

```toml
hl-client = { version = "0.4.0", features = ["ws"] }
```

Raw subscription:

```rust
use hl_client::HyperliquidWs;

let mut ws = HyperliquidWs::mainnet();
ws.connect().await?;
ws.subscribe(serde_json::json!({"type": "l2Book", "coin": "BTC"})).await?;

while let Some(msg) = ws.next_message().await {
    println!("{:?}", msg?);
}
```

Typed subscription helpers:

```rust
use hl_client::{HyperliquidWs, Subscription};

let mut ws = HyperliquidWs::mainnet();
ws.connect().await?;
ws.subscribe_typed(Subscription::L2Book { coin: "BTC".into() }).await?;

while let Some(msg) = ws.next_typed_message().await {
    println!("{:?}", msg?);
}
```

The WebSocket client sends heartbeat pings, reconnects with exponential backoff and jitter, and re-sends subscriptions after reconnecting.

## Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `RetryConfig::max_retries` | `3` | Max retry attempts (excludes initial request). |
| `RetryConfig::base_delay_ms` | `500` | Base delay before first retry. |
| `RetryConfig::backoff_factor` | `2` | Multiplier per retry. |
| `TimeoutConfig::request_timeout` | `30s` | Overall request timeout. |
| `TimeoutConfig::connect_timeout` | `10s` | TCP connection timeout. |
| `RateLimitConfig` | enabled defaults | Token-bucket rate limiter and concurrency gate. |

Constructors validate configs and return `HlError::Config` for invalid settings.

## License

MIT
