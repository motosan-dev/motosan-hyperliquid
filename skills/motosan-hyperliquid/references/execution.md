# Order Execution

Requires signing — needs a private key.

```rust
use hl_client::HyperliquidClient;
use hl_signing::PrivateKeySigner;
use hl_executor::OrderExecutor;

let client = HyperliquidClient::mainnet()?;
let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
let address = signer.address().to_string();

let executor = OrderExecutor::from_client(client, Box::new(signer), address).await?;
```

## Place Limit Order

Orders are built with the typed `OrderWire` builder. `limit_buy`/`limit_sell` take
`Decimal` price and size; `build()` validates that both are positive.

```rust
use hl_types::{OrderWire, Tif};
use rust_decimal::Decimal;
use std::str::FromStr;

// BTC index — look it up from the meta cache (see "Asset Index Lookup" below).
let btc_idx = executor.meta_cache().asset_index("BTC").expect("BTC in universe");

let order = OrderWire::limit_buy(btc_idx, Decimal::from_str("90000.0")?, Decimal::from_str("0.001")?)
    .tif(Tif::Gtc)
    .cloid(HyperliquidClient::generate_cloid())
    .build()?;

let response = executor.place_order(order, None).await?;
println!("Order {}: status={}", response.order_id, response.status);
```

Use `limit_sell(..)` for sells, and `.reduce_only(true)` on the builder for reduce-only orders.

### Time-in-Force Options

`Tif` is an enum:

- `Tif::Gtc` — Good til cancelled
- `Tif::Ioc` — Immediate or cancel
- `Tif::Alo` — Add liquidity only (post-only)

## Place Trigger Order (Stop-Loss / Take-Profit)

Trigger orders go through the `place_trigger_order` helper, which resolves the asset,
rounds the price to wire rules, and attaches a `cloid`. `Side` and `Tpsl` are enums.

```rust
use hl_types::{Side, Tpsl};
use rust_decimal::Decimal;
use std::str::FromStr;

let size = Decimal::from_str("0.001")?;
let trigger_px = Decimal::from_str("86000.0")?;

// A stop-loss sell triggered at $86,000.
let response = executor
    .place_trigger_order("BTC", Side::Sell, size, trigger_px, Tpsl::Sl, None)
    .await?;
println!("Trigger order {}: status={}", response.order_id, response.status);
```

`Tpsl::Sl` = stop-loss, `Tpsl::Tp` = take-profit. The final argument is an optional vault address.

## Cancel Order

`cancel_order` takes the asset index, the exchange order id, and an optional vault:

```rust
executor.cancel_order(asset_index, order_id, None).await?;
```

## Vault Parameter

Pass a vault address (last argument) for vault-delegated trading:

```rust
let response = executor.place_order(order, Some("0xVaultAddress")).await?;
```

## Position Reconciliation

`reconcile_positions` is a free function (not a method) that compares your local
positions against the exchange's `clearinghouseState` and reports stale, missing, or
diverged positions for you to apply:

```rust
use hl_executor::reconcile_positions;

// `transport: Arc<dyn HttpTransport>`, `address: &str`, `local: &[LocalPosition]`
let report = reconcile_positions(transport.as_ref(), address, local).await?;
```

## Asset Index Lookup

The executor caches asset metadata on initialization. Resolve a symbol to its asset
index with `executor.meta_cache().asset_index("BTC")`, or use the `hl-market` crate.
