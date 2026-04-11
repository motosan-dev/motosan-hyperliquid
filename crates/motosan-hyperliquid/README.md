# motosan-hyperliquid

Unified Rust SDK for the [Hyperliquid DEX](https://hyperliquid.xyz) — a single crate that re-exports all sub-crates behind feature flags.

## Quick Start

```toml
[dependencies]
motosan-hyperliquid = "0.1"
```

All features are enabled by default (`full`). To pick only what you need:

```toml
[dependencies]
motosan-hyperliquid = { version = "0.1", default-features = false, features = ["market", "executor"] }
```

## Usage

```rust,no_run
use motosan_hyperliquid::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Market data (no signing required)
    let client = HyperliquidClient::mainnet()?;
    let market = MarketData::from_client(client.clone());
    let mid = market.mid_price("BTC").await?;
    println!("BTC mid: {mid}");

    // Trading (requires a private key)
    let signer = PrivateKeySigner::from_hex("0x...")?;
    let address = signer.address().to_string();
    let executor = OrderExecutor::from_client(client, Box::new(signer), address).await?;

    let btc = executor.meta_cache().asset_index("BTC").unwrap();
    let order = OrderWire::limit_buy(btc, Decimal::from(60000), Decimal::new(1, 3)).build()?;
    let resp = executor.place_order(order, None).await?;
    println!("Order: {:?}", resp.status);

    Ok(())
}
```

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `full` | Yes | Enables all features below |
| `market` | Yes | `MarketData` — candles, orderbook, funding rates |
| `account` | Yes | `Account` — positions, fills, fees, vaults |
| `executor` | Yes | `OrderExecutor` — place/cancel/modify orders, TWAP, spot |
| `signing` | Yes | `PrivateKeySigner`, EIP-712 signing |
| `ws` | Yes | `HyperliquidWs` — WebSocket subscriptions |

## Sub-Crates

| Crate | Description |
|-------|-------------|
| [`hl-types`](https://crates.io/crates/hl-types) | Domain types (orders, positions, errors) |
| [`hl-client`](https://crates.io/crates/hl-client) | REST + WebSocket transport |
| [`hl-signing`](https://crates.io/crates/hl-signing) | EIP-712 signing |
| [`hl-market`](https://crates.io/crates/hl-market) | Market data queries |
| [`hl-account`](https://crates.io/crates/hl-account) | Account state queries |
| [`hl-executor`](https://crates.io/crates/hl-executor) | Order execution |

## License

MIT
