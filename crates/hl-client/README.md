# hl-client

> Hyperliquid REST and WebSocket client with automatic retry, exponential backoff, and rate-limit handling.

## Overview

`hl-client` provides `HyperliquidClient` for REST API communication with the Hyperliquid exchange. It handles:

- **Automatic retry** with exponential backoff for transient failures (network errors, 5xx, 429)
- **Rate-limit awareness** -- respects `Retry-After` headers on 429 responses
- **Configurable timeouts** for both TCP connection and overall request duration
- **Client order ID generation** (`generate_cloid`) for idempotent order submission

An optional `ws` feature adds `HyperliquidWs` for WebSocket subscriptions with auto-reconnect and heartbeat.

## Usage

### Create a Client

```rust
use hl_client::HyperliquidClient;

// Default configuration
let client = HyperliquidClient::mainnet()?;
let client = HyperliquidClient::testnet()?;

// Custom retry config
use hl_client::RetryConfig;
let client = HyperliquidClient::with_retry_config(
    true,
    RetryConfig { max_retries: 5, base_delay_ms: 1000, backoff_factor: 2 },
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
    None, // vault_address
).await?;
```

### WebSocket (opt-in)

Enable with `features = ["ws"]` in your `Cargo.toml`:

```rust
use hl_client::HyperliquidWs;

let mut ws = HyperliquidWs::mainnet();
ws.connect().await?;
ws.subscribe(serde_json::json!({"type": "l2Book", "coin": "BTC"})).await?;

while let Some(msg) = ws.next_message().await {
    match msg {
        Ok(data) => println!("{data}"),
        Err(e) => eprintln!("Error: {e}"),
    }
}
```

The WebSocket client automatically:
- Sends heartbeat pings every 30 seconds
- Reconnects with exponential backoff and jitter on disconnection
- Re-sends all subscriptions after reconnecting

## Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `RetryConfig::max_retries` | `3` | Max retry attempts (excludes initial request) |
| `RetryConfig::base_delay_ms` | `500` | Base delay before first retry |
| `RetryConfig::backoff_factor` | `2` | Multiplier per retry (500ms, 1s, 2s, ...) |
| `TimeoutConfig::request_timeout` | `30s` | Overall request timeout |
| `TimeoutConfig::connect_timeout` | `10s` | TCP connection timeout |

Delay is capped at 30 seconds regardless of backoff factor.

## License

MIT
