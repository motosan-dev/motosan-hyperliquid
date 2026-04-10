# motosan-hyperliquid

> A modular Rust SDK for the Hyperliquid L1 exchange -- market data, account queries, EIP-712 signing, and order execution.

## Why This Exists

Hyperliquid's API returns string-encoded numerics, uses a custom EIP-712 signing scheme, and has undocumented edge cases in its wire format. This SDK handles all of that so you can focus on trading logic instead of protocol plumbing.

## Crate Map

| Crate | Description |
|-------|-------------|
| [`hl-types`](crates/hl-types/) | Shared domain types -- orders, positions, candles, errors, signatures |
| [`hl-signing`](crates/hl-signing/) | EIP-712 signing via the `Signer` trait, with a built-in `PrivateKeySigner` |
| [`hl-client`](crates/hl-client/) | HTTP client with automatic retry, rate-limit handling, and optional WebSocket support |
| [`hl-market`](crates/hl-market/) | Market data queries -- candles, orderbook, funding rates, asset metadata |
| [`hl-account`](crates/hl-account/) | Account state queries -- positions, fills, vaults, agent approvals |
| [`hl-executor`](crates/hl-executor/) | Order execution -- place/cancel orders, trigger orders, position reconciliation |

## Quick Start

Add the crates you need to your `Cargo.toml`:

```toml
[dependencies]
hl-client = { path = "sdks/motosan-hyperliquid/crates/hl-client" }
hl-market = { path = "sdks/motosan-hyperliquid/crates/hl-market" }
```

Fetch the BTC orderbook in five lines:

```rust
use hl_client::HyperliquidClient;
use hl_market::MarketData;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = HyperliquidClient::mainnet()?;
    let market = MarketData::from_client(client);

    let book = market.orderbook("BTC").await?;
    println!("Best bid: {:?}, Best ask: {:?}", book.bids[0], book.asks[0]);
    Ok(())
}
```

## Installation

**Prerequisites**: Rust 1.70+, Cargo

This SDK is organized as a Cargo workspace. Each crate can be depended on individually:

```toml
# Market data only (read-only, no signing needed)
hl-client = { path = "sdks/motosan-hyperliquid/crates/hl-client" }
hl-market = { path = "sdks/motosan-hyperliquid/crates/hl-market" }

# Account queries
hl-account = { path = "sdks/motosan-hyperliquid/crates/hl-account" }

# Full trading (signing + execution)
hl-signing = { path = "sdks/motosan-hyperliquid/crates/hl-signing" }
hl-executor = { path = "sdks/motosan-hyperliquid/crates/hl-executor" }
```

## Usage

### Shared Client (Recommended)

Create a single `HyperliquidClient` wrapped in `Arc` and share it across all consumers. This reuses one connection pool and avoids redundant TLS handshakes:

```rust
use std::sync::Arc;
use hl_client::{HttpTransport, HyperliquidClient};
use hl_market::MarketData;
use hl_account::Account;

let client = Arc::new(HyperliquidClient::mainnet()?);
let transport: Arc<dyn HttpTransport> = client;

let market = MarketData::new(transport.clone());
let account = Account::new(transport.clone());

// Both share the same underlying HTTP client
let book = market.orderbook("BTC").await?;
let state = account.state("0xYourAddress").await?;
```

Each consumer struct also provides a `from_client()` convenience constructor that wraps a `HyperliquidClient` in `Arc` for you. This is fine when you only need a single consumer:

```rust
let client = HyperliquidClient::mainnet()?;
let market = MarketData::from_client(client); // wraps in Arc internally
```

### Query Market Data

```rust
use hl_client::HyperliquidClient;
use hl_market::MarketData;

let client = HyperliquidClient::mainnet()?;
let market = MarketData::from_client(client);

// Fetch the last 10 hourly candles for ETH
let candles = market.candles("ETH", "1h", 10).await?;
for c in &candles {
    println!("{}: O={} H={} L={} C={} V={}", c.timestamp, c.open, c.high, c.low, c.close, c.volume);
}

// Get the mid-price
let mid = market.mid_price("BTC").await?;
println!("BTC mid-price: {mid}");

// Fetch funding rates for all perpetuals
let rates = market.funding_rates().await?;
```

### Check Account State

```rust
use hl_client::HyperliquidClient;
use hl_account::Account;

let client = HyperliquidClient::mainnet()?;
let account = Account::from_client(client);

let state = account.state("0xYourAddress").await?;
println!("Equity: {}, Margin available: {}", state.equity, state.margin_available);

for pos in &state.positions {
    println!("{}: size={} entry={} pnl={}", pos.coin, pos.size, pos.entry_px, pos.unrealized_pnl);
}

let fills = account.fills("0xYourAddress").await?;
```

### Place an Order

```rust
use hl_client::HyperliquidClient;
use hl_signing::PrivateKeySigner;
use hl_executor::OrderExecutor;
use hl_types::{OrderWire, OrderTypeWire, LimitOrderType};

let client = HyperliquidClient::mainnet()?;
let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
let address = signer.address().to_string();

let executor = OrderExecutor::from_client(client, Box::new(signer), address).await?;

let order = OrderWire {
    asset: 0, // BTC index
    is_buy: true,
    limit_px: "90000.0".to_string(),
    sz: "0.001".to_string(),
    reduce_only: false,
    order_type: OrderTypeWire {
        limit: Some(LimitOrderType { tif: "Gtc".to_string() }),
        trigger: None,
    },
    cloid: Some(HyperliquidClient::generate_cloid()),
};

let response = executor.place_order(order, None).await?;
println!("Order {}: status={}", response.order_id, response.status);
```

### Configuration

#### Client Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `is_mainnet` | `bool` | -- | Target mainnet (`true`) or testnet (`false`) |
| `retry_config.max_retries` | `u32` | `3` | Maximum retry attempts on transient failures |
| `retry_config.base_delay_ms` | `u64` | `500` | Base delay before first retry |
| `retry_config.backoff_factor` | `u32` | `2` | Exponential backoff multiplier |
| `timeout_config.request_timeout` | `Duration` | `30s` | Overall HTTP request timeout |
| `timeout_config.connect_timeout` | `Duration` | `10s` | TCP connection timeout |

```rust
use hl_client::{HyperliquidClient, RetryConfig, TimeoutConfig};
use std::time::Duration;

let client = HyperliquidClient::with_config(
    true, // mainnet
    RetryConfig { max_retries: 5, base_delay_ms: 1000, backoff_factor: 2 },
    TimeoutConfig {
        request_timeout: Duration::from_secs(60),
        connect_timeout: Duration::from_secs(15),
    },
)?;
```

#### WebSocket (opt-in)

Enable the `ws` feature on `hl-client` for WebSocket support:

```toml
hl-client = { path = "...", features = ["ws"] }
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

## Architecture

```
hl-types          (no dependencies -- pure data types)
    |
hl-signing        (depends on hl-types)
    |
hl-client         (depends on hl-types)
   / \
hl-market  hl-account   (depend on hl-client + hl-types)
       \   /
    hl-executor          (depends on hl-client + hl-signing + hl-types)
```

The dependency graph is intentionally layered. You can use `hl-market` for read-only market data without pulling in signing or execution dependencies.

## Error Handling

All crates use `hl_types::HlError` as the unified error type:

| Variant | Retryable | Description |
|---------|-----------|-------------|
| `Http` | Yes | Network / connection failure |
| `RateLimited` | Yes | HTTP 429 with `retry_after_ms` |
| `Api` | 5xx only | Non-success HTTP status |
| `Signing` | No | EIP-712 signing failure |
| `Serialization` | No | JSON / msgpack encoding error |
| `InvalidAddress` | No | Malformed Ethereum address |
| `Parse` | No | Unexpected response format |

The client's built-in retry logic handles retryable errors automatically. You can check `error.is_retryable()` for custom retry strategies.

## Examples

Runnable example programs live in [`examples/`](examples/):

| File | Description |
|------|-------------|
| [`shared_client.rs`](examples/shared_client.rs) | Share one client across market data + account queries via `Arc` |
| [`query_market.rs`](examples/query_market.rs) | Fetch candles and orderbook |
| [`check_account.rs`](examples/check_account.rs) | Query positions and fills |
| [`place_order.rs`](examples/place_order.rs) | Sign and submit a limit order |

Run an example:

```bash
cd sdks/motosan-hyperliquid
cargo run --example query_market
```

## License

MIT
