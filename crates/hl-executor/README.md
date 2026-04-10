# hl-executor

> Order execution for Hyperliquid -- place/cancel orders, trigger orders (stop-loss, take-profit), and position reconciliation.

## Overview

`hl-executor` provides `OrderExecutor`, a standalone order execution engine that handles signing, nonce management, and response parsing. It also includes `reconcile_positions` for detecting drift between your local state and the exchange.

## Usage

### Create an Executor

```rust
use hl_client::HyperliquidClient;
use hl_signing::PrivateKeySigner;
use hl_executor::OrderExecutor;

let client = HyperliquidClient::mainnet()?;
let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
let address = signer.address().to_string();

// Loads asset metadata from the exchange on creation
let executor = OrderExecutor::new(client, Box::new(signer), address).await?;
```

### Place a Limit Order

```rust
use hl_types::{OrderWire, Tif};
use hl_client::HyperliquidClient;

let order = OrderWire::limit_buy(0, "90000.0", "0.001") // BTC -- use meta_cache.asset_index("BTC")
    .tif(Tif::Gtc)
    .cloid(HyperliquidClient::generate_cloid())
    .build();

let resp = executor.place_order(order, None).await?;
println!("Order {}: status={}, filled={}/{}",
    resp.order_id, resp.status, resp.filled_size, resp.requested_size);
```

Time-in-force options: `"Gtc"` (good-till-cancel), `"Ioc"` (immediate-or-cancel), `"Alo"` (add-liquidity-only).

### Place a Trigger Order (Stop-Loss / Take-Profit)

```rust
let resp = executor.place_trigger_order(
    "BTC",          // symbol
    "sell",         // side (opposite of your position)
    0.001,          // size
    85000.0,        // trigger price
    "sl",           // "sl" for stop-loss, "tp" for take-profit
    None,           // vault
).await?;
```

### Cancel an Order

```rust
let result = executor.cancel_order(
    0,         // asset index
    123456789, // exchange order ID
    None,      // vault
).await?;
```

### Transfer USDC to a Vault

```rust
executor.transfer_to_vault("0xVaultAddress", 1000.0).await?;
```

### Asset Meta Cache

The executor caches asset metadata (coin name to index mapping) so you do not need to look up asset IDs manually:

```rust
let cache = executor.meta_cache();
let btc_idx = cache.asset_index("BTC");    // Some(0)
let sz_dec = cache.sz_decimals("BTC");     // Some(5)
```

### Position Reconciliation

Detect drift between your local tracking and the exchange:

```rust
use hl_executor::{reconcile_positions, LocalPosition};

let local = vec![
    LocalPosition {
        id: "pos-1".into(),
        coin: "BTC".into(),
        side: "long".into(),
        size: 0.5,
    },
];

let report = reconcile_positions(executor.client(), "0xYourAddress", &local).await?;

for action in &report.actions {
    match action {
        ReconcileAction::ClosedStale { id, market } =>
            println!("STALE: {id} on {market} -- close locally"),
        ReconcileAction::AddedMissing { market, side, size, .. } =>
            println!("MISSING: {side} {size} on {market} -- add locally"),
        ReconcileAction::Updated { market, old_size, new_size, .. } =>
            println!("DRIFT: {market} {old_size} -> {new_size}"),
    }
}
```

## Nonce Management

Nonces are generated automatically using a monotonically increasing counter based on the system clock. You do not need to manage nonces manually. The counter guarantees uniqueness even if the system clock jumps backward.

## License

MIT
