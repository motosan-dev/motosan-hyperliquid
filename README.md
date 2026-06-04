# motosan-hyperliquid

> Modular Rust SDK for the Hyperliquid L1 exchange — market data, account queries, EIP-712 signing, WebSocket feeds, and order execution.

## Status

Latest published Rust release: **0.3.0** (MSRV **Rust 1.91+**).

Release 0.3.0 adds builder-code order actions, OCO / TP-SL grouped bulk orders, vault withdrawals with correct micro-unit encoding, richer account queries (`frontend_open_orders`, `order_status_by_cloid`, `fills_by_time`), and L1 action expiry / replay protection with `OrderExecutor::set_expires_after(...)`.

## Why This Exists

Hyperliquid's API returns string-encoded numerics, uses custom EIP-712 signing, and has strict wire-format edge cases. This SDK handles numeric parsing, signing, rounding, idempotent client order IDs, retry/rate-limit behavior, and typed responses so trading code can focus on strategy.

## Crate Map

| Crate | Description |
|-------|-------------|
| [`motosan-hyperliquid`](crates/motosan-hyperliquid/) | Facade crate — re-exports the sub-crates behind feature flags (`market`, `account`, `executor`, `signing`, `ws`, `full`). |
| [`hl-types`](crates/hl-types/) | Shared domain types — orders, positions, candles, errors, signatures, `Grouping`. |
| [`hl-signing`](crates/hl-signing/) | EIP-712 signing via the `Signer` trait, `PrivateKeySigner`, L1 action signing with optional expiry. |
| [`hl-client`](crates/hl-client/) | HTTP client with retry, rate limiting, concurrency gate, graceful shutdown, and optional WebSocket support. |
| [`hl-market`](crates/hl-market/) | Market data queries — candles, orderbook, funding rates, asset metadata. |
| [`hl-account`](crates/hl-account/) | Account state queries — positions, fills, open orders, vaults, fees, funding, staking, frontend order view. |
| [`hl-executor`](crates/hl-executor/) | Execution — place/cancel/modify orders, trigger orders, grouped orders, spot, TWAP, transfers, admin actions. |

## Installation

Published release:

```toml
[dependencies]
motosan-hyperliquid = "0.3.0" # `full` feature enabled by default
```

Pick individual crates:

```toml
[dependencies]
hl-client   = "0.3.0"
hl-market   = "0.3.0"
hl-account  = "0.3.0"
hl-signing  = "0.3.0"
hl-executor = "0.3.0"
hl-types    = "0.3.0"
```

Enable WebSocket support when using `hl-client` directly:

```toml
hl-client = { version = "0.3.0", features = ["ws"] }
```

## Quick Start

Fetch the BTC orderbook:

```rust
use hl_client::HyperliquidClient;
use hl_market::MarketData;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = HyperliquidClient::mainnet()?;
    let market = MarketData::from_client(client);

    let book = market.orderbook("BTC").await?;
    println!("Best bid: {:?}, best ask: {:?}", book.bids[0], book.asks[0]);
    Ok(())
}
```

Place a limit order:

```rust,no_run
use hl_client::HyperliquidClient;
use hl_executor::OrderExecutor;
use hl_signing::PrivateKeySigner;
use hl_types::{OrderWire, Tif};
use rust_decimal::Decimal;
use std::str::FromStr;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = HyperliquidClient::mainnet()?;
let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
let address = signer.address().to_string();

let executor = OrderExecutor::from_client(client, Box::new(signer), address).await?;
let btc_idx = executor.meta_cache().asset_index("BTC").expect("BTC in universe");

let order = OrderWire::limit_buy(
    btc_idx,
    Decimal::from_str("90000.0")?,
    Decimal::from_str("0.001")?,
)
.tif(Tif::Gtc)
.cloid(HyperliquidClient::generate_cloid())
.build()?;

let response = executor.place_order(order, None).await?;
println!("Order {}: status={}", response.order_id, response.status);
# Ok(()) }
```

## Recent Execution Features

### Action Expiry

Attach an `expiresAfter` unix epoch timestamp in milliseconds to subsequent L1 actions. The timestamp is included in the signed action hash and `/exchange` body.

```rust,no_run
use std::time::{SystemTime, UNIX_EPOCH};
# use hl_executor::OrderExecutor;
# async fn example(executor: OrderExecutor) -> Result<(), Box<dyn std::error::Error>> {
let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
executor.set_expires_after(Some(now_ms + 60_000)); // reject if processed after ~60s
// executor.place_order(order, None).await?;
executor.set_expires_after(None); // clear
# Ok(()) }
```

`expiresAfter` applies to L1 actions (orders, cancels, leverage, vault transfers, etc.). User-signed EIP-712 actions such as `usdSend`, `withdraw3`, `spotSend`, `sendAsset`, agent approval, builder approval, and sub-account actions are unaffected.

### Builder Codes and OCO Grouping

```rust,no_run
use hl_types::Grouping;
# use hl_executor::OrderExecutor;
# use hl_types::OrderWire;
# async fn example(executor: OrderExecutor, parent: OrderWire, tp: OrderWire, sl: OrderWire) -> Result<(), Box<dyn std::error::Error>> {
// Builder fee is in tenths of a basis point: 10 = 1 bp = 0.01%.
let _resp = executor
    .place_order_with_builder(parent.clone(), Some(("0x1111111111111111111111111111111111111111", 10)), None)
    .await?;

// Parent entry at index 0, followed by reduce-only TP/SL children.
let _bracket = executor
    .bulk_order_grouped(vec![parent, tp, sl], Grouping::NormalTpsl, None)
    .await?;
# Ok(()) }
```

### Vault Transfers

```rust,no_run
use rust_decimal::Decimal;
# use hl_executor::OrderExecutor;
# async fn example(executor: OrderExecutor) -> Result<(), Box<dyn std::error::Error>> {
executor.deposit_to_vault("0xVaultAddress", Decimal::from(100)).await?;
executor.withdraw_from_vault("0xVaultAddress", Decimal::from(50)).await?;
# Ok(()) }
```

## Account Queries

```rust,no_run
use hl_account::Account;
use hl_client::HyperliquidClient;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let account = Account::from_client(HyperliquidClient::mainnet()?);
let address = "0xYourAddress";

let state = account.state(address).await?;
let fills = account.fills(address).await?;
let recent_fills = account.fills_by_time(address, 1_717_000_000_000, None, false).await?;
let frontend_orders = account.frontend_open_orders(address, None).await?;
let status = account.order_status_by_cloid(address, "0x0123456789abcdef0123456789abcdef").await?;
# Ok(()) }
```

## Architecture

```text
hl-types          (pure data types, no network deps)
    |
hl-signing        (depends on hl-types)
    |
hl-client         (depends on hl-types)
   / \
hl-market  hl-account   (depend on hl-client + hl-types)
       \   /
    hl-executor          (depends on hl-client + hl-signing + hl-types)
```

The dependency graph is intentionally layered: read-only market data does not pull in signing or execution.

## Error Handling

All crates use `hl_types::HlError`:

| Variant | Retryable | Description |
|---------|-----------|-------------|
| `Http` / `Timeout` / `WebSocket` | Yes | Transport or timeout failure |
| `RateLimited` | Yes | HTTP 429 with `retry_after_ms` |
| `Api` | 5xx only | Non-success HTTP status |
| `Signing` | No | EIP-712 signing failure |
| `Serialization` | No | JSON / msgpack encoding error |
| `InvalidAddress` | No | Malformed Ethereum address |
| `Validation` / `Config` | No | Bad inputs or invalid client configuration |
| `Parse` | No | Unexpected response format |
| `Rejected` | No | Exchange rejected an action |

## Examples

Runnable example programs live in [`examples/`](examples/):

| File | Description |
|------|-------------|
| [`shared_client.rs`](examples/shared_client.rs) | Share one client across market data + account queries via `Arc`. |
| [`query_market.rs`](examples/query_market.rs) | Fetch candles and orderbook. |
| [`check_account.rs`](examples/check_account.rs) | Query positions and fills. |
| [`place_order.rs`](examples/place_order.rs) | Sign and submit a limit order. |
| [`trigger_order.rs`](examples/trigger_order.rs) | Place stop-loss / take-profit trigger orders. |
| [`ws_stream.rs`](examples/ws_stream.rs) | Typed WebSocket subscriptions. |

Run an example:

```bash
cargo run --example query_market
```

## License

MIT
