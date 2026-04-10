# Client

## Create Client

```rust
use hl_client::HyperliquidClient;

let client = HyperliquidClient::mainnet()?;    // mainnet
let client = HyperliquidClient::testnet()?;    // testnet
```

## Custom Configuration

```rust
use hl_client::{HyperliquidClient, RetryConfig, TimeoutConfig};
use std::time::Duration;

let client = HyperliquidClient::with_config(
    true, // is_mainnet
    RetryConfig { max_retries: 5, base_delay_ms: 1000, backoff_factor: 2 },
    TimeoutConfig {
        request_timeout: Duration::from_secs(60),
        connect_timeout: Duration::from_secs(15),
    },
)?;
```

## Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `is_mainnet` | `bool` | -- | Target mainnet (`true`) or testnet (`false`) |
| `retry_config.max_retries` | `u32` | `3` | Maximum retry attempts on transient failures |
| `retry_config.base_delay_ms` | `u64` | `500` | Base delay before first retry |
| `retry_config.backoff_factor` | `u32` | `2` | Exponential backoff multiplier |
| `timeout_config.request_timeout` | `Duration` | `30s` | Overall HTTP request timeout |
| `timeout_config.connect_timeout` | `Duration` | `10s` | TCP connection timeout |

Retry applies to: HTTP 429 (rate limit), 5xx (server error), connection/timeout errors.
Backoff: `base_delay * backoff_factor^(attempt-1)`.

## Client Order ID

```rust
let cloid = HyperliquidClient::generate_cloid();
```

## WebSocket (feature: `ws`)

```toml
hl-client = { version = "0.1.0", features = ["ws"] }
```

```rust
use hl_client::HyperliquidWs;

let mut ws = HyperliquidWs::mainnet();
ws.connect().await?;
ws.subscribe(serde_json::json!({"type": "l2Book", "coin": "BTC"})).await?;

while let Some(msg) = ws.next_message().await {
    println!("{:?}", msg?);
}
```

WebSocket includes auto-reconnect and heartbeat.
