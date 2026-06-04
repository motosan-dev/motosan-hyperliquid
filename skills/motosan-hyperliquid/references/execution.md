# Order Execution

Requires signing — create an `OrderExecutor` with a private key or any custom `Signer`.

```rust
use hl_client::HyperliquidClient;
use hl_executor::OrderExecutor;
use hl_signing::PrivateKeySigner;

let client = HyperliquidClient::mainnet()?;
let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
let address = signer.address().to_string();
let executor = OrderExecutor::from_client(client, Box::new(signer), address).await?;
```

Use `OrderExecutor::new(Arc<dyn HttpTransport>, ...)` when sharing a transport, or `with_meta_cache` to avoid loading metadata from the network.

## Place Limit Order

Orders are built with the typed `OrderWire` builder. `limit_buy` / `limit_sell` take `Decimal` price and size; `build()` validates both are positive.

```rust
use hl_client::HyperliquidClient;
use hl_types::{OrderWire, Tif};
use rust_decimal::Decimal;
use std::str::FromStr;

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
```

Use `limit_sell(..)` for sells and `.reduce_only(true)` for reduce-only orders. Write helpers auto-attach a CLOID if one is missing, making retried POSTs idempotent.

### Time-in-Force

- `Tif::Gtc` — good til cancelled
- `Tif::Ioc` — immediate or cancel
- `Tif::Alo` — add-liquidity-only / post-only

## Trigger Orders

`place_trigger_order` resolves the asset, rounds price/size to Hyperliquid wire rules, attaches a CLOID, and validates the built order.

```rust
use hl_types::{Side, Tpsl};
use rust_decimal::Decimal;
use std::str::FromStr;

let response = executor
    .place_trigger_order(
        "BTC",
        Side::Sell,
        Decimal::from_str("0.001")?,
        Decimal::from_str("86000.0")?,
        Tpsl::Sl,
        None,
    )
    .await?;
```

`Tpsl::Sl` = stop-loss, `Tpsl::Tp` = take-profit.

## Builder Codes

Attach builder info to order actions:

```rust
let builder = Some(("0x1111111111111111111111111111111111111111", 10));
let response = executor.place_order_with_builder(order, builder, None).await?;
let responses = executor.bulk_order_with_builder(vec![order_a, order_b], builder, None).await?;

let trigger = executor
    .place_trigger_order_with_builder("BTC", Side::Sell, size, trigger_px, Tpsl::Tp, builder, None)
    .await?;
```

Builder fee is in **tenths of a basis point** (`10` = 1 bp = 0.01%). The user must have approved at least that `maxFeeRate` via `approve_builder_fee`.

## Bulk Orders and OCO / TP-SL Grouping

```rust
use hl_types::Grouping;

// Independent orders (default grouping = "na").
let responses = executor.bulk_order(vec![order_a, order_b], None).await?;

// Parent entry first, then reduce-only take-profit / stop-loss children.
let bracket = executor
    .bulk_order_grouped(vec![parent, take_profit, stop_loss], Grouping::NormalTpsl, None)
    .await?;

// Position-level TP/SL attachment.
let position_bracket = executor
    .bulk_order_grouped(vec![take_profit, stop_loss], Grouping::PositionTpsl, None)
    .await?;
```

`Grouping::NormalTpsl` links a parent order at index 0 with TP/SL children as an OCO bracket. The SDK preserves caller order; it does not auto-construct or reorder bracket legs.

## Cancel and Modify

```rust
executor.cancel_order(asset_index, oid, None).await?;
executor.cancel_by_cloid("BTC", cloid, None).await?;
executor.bulk_cancel(vec![cancel_a, cancel_b], None).await?;
executor.bulk_cancel_by_cloid(vec![cloid_cancel], None).await?;

executor.modify_order(oid, replacement_order, None).await?;
executor.bulk_modify(vec![modify_a, modify_b], None).await?;
```

## Market, Scale, TWAP, Spot

```rust
let open = executor.market_open("BTC", Side::Buy, size, None, None).await?;
let close = executor.market_close("BTC", Some(size), None, None).await?; // size is unsigned magnitude
let ladder = executor
    .place_scale_order("BTC", true, total_size, start_px, end_px, 5, Tif::Gtc, None)
    .await?;
let twap = executor
    .place_twap_order("BTC", true, size, duration_secs, false, true, None)
    .await?;
executor.cancel_twap("BTC", twap_id, None).await?;

let spot_asset = executor.meta_cache().spot_asset_index("PURR").expect("PURR listed");
let spot_order = OrderWire::limit_buy(spot_asset, px, size).tif(Tif::Gtc).build()?;
let spot = executor.place_spot_order(spot_order, None).await?;
```

`market_close` derives the close side from the live position. Do not encode direction in the sign of `size`.

## Transfers and Admin Actions

```rust
use rust_decimal::Decimal;

executor.usdc_transfer(destination, Decimal::from(10), None).await?;
executor.withdraw(destination, Decimal::from(10), None).await?; // withdraw3
executor.spot_send(destination, "PURR:0x...", Decimal::from(1), None).await?;
executor.send_asset(destination, "USDC", Decimal::from(10), None).await?;
executor.class_transfer(Decimal::from(10), true, None).await?; // spot -> perp

executor.deposit_to_vault(vault, Decimal::from(100)).await?;
executor.withdraw_from_vault(vault, Decimal::from(50)).await?;
executor.vault_transfer(vault, false, Decimal::from(25)).await?;
```

`vault_transfer` sends the `usd` field as integer micro-units (6 decimals), matching the exchange wire format. `transfer_to_vault` remains a backward-compatible alias for `deposit_to_vault`.

Admin/sub-account helpers include `update_leverage`, `update_isolated_margin`, `schedule_cancel`, `claim_rewards`, `approve_agent`, `approve_builder_fee`, `set_referrer`, and sub-account create/modify/transfer methods.

## Action Expiry (Replay Protection)

`expiresAfter` on L1 actions is supported via `motosan-wallet-core` 0.5.3.

```rust
use std::time::{SystemTime, UNIX_EPOCH};

let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
executor.set_expires_after(Some(now_ms + 60_000)); // valid for about 60s
let response = executor.place_order(order, None).await?;

assert_eq!(executor.expires_after(), Some(now_ms + 60_000));
executor.set_expires_after(None); // clear; default is no expiry
```

The expiry timestamp is folded into the signed L1 action hash and sent in the `/exchange` body. It applies to L1 actions such as orders, cancels, leverage changes, and vault transfers.

It does **not** apply to EIP-712 user-signed actions (`usdSend`, `withdraw3`, `spotSend`, `sendAsset`, agent/builder/sub-account actions), matching the official Python SDK.

## Vault Parameter

Most trading methods accept a final `vault: Option<&str>` for vault-delegated trading:

```rust
let response = executor.place_order(order, Some("0xVaultAddress")).await?;
```

## Asset Index Lookup

The executor loads and caches exchange metadata on initialization:

```rust
let cache = executor.meta_cache();
let btc_idx = cache.asset_index("BTC");
let sz_dec = cache.sz_decimals("BTC");
```

Symbols are normalized before lookup (`"BTC-PERP"` → `"BTC"`).

## Position Reconciliation

`reconcile_positions` is a free function that compares local positions to exchange `clearinghouseState` and reports stale, missing, or diverged positions.

```rust
use hl_executor::reconcile_positions;

let report = reconcile_positions(executor.client(), address, local_positions).await?;
for action in &report.actions {
    println!("{:?}", action);
}
```

## Nonce Management

Nonces are generated automatically with a monotonic per-executor counter based on system time. You normally do not manage nonces manually.
