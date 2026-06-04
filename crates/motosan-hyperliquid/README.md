# motosan-hyperliquid

Unified Rust SDK for the [Hyperliquid DEX](https://hyperliquid.xyz) — a single facade crate that re-exports all workspace sub-crates behind feature flags.

## Quick Start

Published release:

```toml
[dependencies]
motosan-hyperliquid = "0.3.0"
```

Release 0.3.0 includes builder codes, OCO grouping, vault withdraw, frontend open orders, and action expiry.

All features are enabled by default through `full`. To pick only what you need:

```toml
[dependencies]
motosan-hyperliquid = { version = "0.3.0", default-features = false, features = ["market", "account"] }
```

## Usage

```rust,no_run
use motosan_hyperliquid::prelude::*;
use std::str::FromStr;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Arc::new(HyperliquidClient::mainnet()?);
    let transport: Arc<dyn HttpTransport> = client;

    // Market data (no signing required)
    let market = MarketData::new(transport.clone());
    let mid = market.mid_price("BTC").await?;
    println!("BTC mid: {mid}");

    // Trading (requires a private key)
    let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
    let address = signer.address().to_string();
    let executor = OrderExecutor::new(transport, Box::new(signer), address).await?;

    let btc = executor.meta_cache().asset_index("BTC").unwrap();
    let order = OrderWire::limit_buy(btc, Decimal::from_str("90000")?, Decimal::new(1, 3))
        .tif(Tif::Gtc)
        .cloid(HyperliquidClient::generate_cloid())
        .build()?;
    let resp = executor.place_order(order, None).await?;
    println!("Order: {:?}", resp.status);

    Ok(())
}
```

## Recent APIs

```rust,no_run
use motosan_hyperliquid::prelude::*;
# async fn example(executor: OrderExecutor, order: OrderWire, parent: OrderWire, tp: OrderWire, sl: OrderWire) -> Result<(), Box<dyn std::error::Error>> {
// L1 action expiry / replay protection.
executor.set_expires_after(Some(1_717_000_000_000));
executor.set_expires_after(None);

// Builder codes.
executor
    .place_order_with_builder(order, Some(("0x1111111111111111111111111111111111111111", 10)), None)
    .await?;

// OCO / TP-SL bracket grouping.
executor
    .bulk_order_grouped(vec![parent, tp, sl], Grouping::NormalTpsl, None)
    .await?;
# Ok(()) }
```

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `full` | Yes | Enables all features below. |
| `market` | Yes | `MarketData` — candles, orderbook, funding rates. |
| `account` | Yes | `Account` — positions, fills, open orders, frontend order view, fees, vaults. |
| `executor` | Yes | `OrderExecutor` — place/cancel/modify orders, triggers, grouped orders, TWAP, spot, transfers. |
| `signing` | Yes | `PrivateKeySigner`, `Signer`, EIP-712 signing helpers. |
| `ws` | Yes | `HyperliquidWs`, typed subscriptions and messages. |

## Sub-Crates

| Crate | Description |
|-------|-------------|
| [`hl-types`](https://crates.io/crates/hl-types) | Domain types (orders, positions, errors, `Grouping`). |
| [`hl-client`](https://crates.io/crates/hl-client) | REST + WebSocket transport. |
| [`hl-signing`](https://crates.io/crates/hl-signing) | EIP-712 signing. |
| [`hl-market`](https://crates.io/crates/hl-market) | Market data queries. |
| [`hl-account`](https://crates.io/crates/hl-account) | Account state queries. |
| [`hl-executor`](https://crates.io/crates/hl-executor) | Order execution. |

## License

MIT
