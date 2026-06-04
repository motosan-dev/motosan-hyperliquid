# hl-executor

> Signed execution for Hyperliquid — place/cancel/modify orders, trigger orders, grouped TP/SL brackets, spot, TWAP, transfers, admin actions, and position reconciliation.

## Overview

`hl-executor` provides `OrderExecutor`, a standalone execution engine that handles signing, nonce management, asset metadata lookup, wire rounding, CLOID idempotency, and response parsing.

## Create an Executor

```rust
use hl_client::HyperliquidClient;
use hl_executor::OrderExecutor;
use hl_signing::PrivateKeySigner;

let client = HyperliquidClient::mainnet()?;
let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
let address = signer.address().to_string();

// Loads asset metadata from the exchange.
let executor = OrderExecutor::from_client(client, Box::new(signer), address).await?;
```

Use `OrderExecutor::new(Arc<dyn HttpTransport>, ...)` when sharing a transport, or `with_meta_cache` when you already have an `AssetMetaCache`.

## Place a Limit Order

```rust
use hl_client::HyperliquidClient;
use hl_types::{OrderWire, Tif};
use rust_decimal::Decimal;
use std::str::FromStr;

let btc = executor.meta_cache().asset_index("BTC").expect("BTC in universe");
let order = OrderWire::limit_buy(btc, Decimal::from_str("90000.0")?, Decimal::from_str("0.001")?)
    .tif(Tif::Gtc)
    .cloid(HyperliquidClient::generate_cloid())
    .build()?;

let resp = executor.place_order(order, None).await?;
println!("Order {}: status={}, filled={}/{}", resp.order_id, resp.status, resp.filled_size, resp.requested_size);
```

`build()` validates positive price/size. Order write helpers auto-attach a CLOID if missing.

## Trigger Orders

```rust
use hl_types::{Side, Tpsl};
use rust_decimal::Decimal;

let resp = executor
    .place_trigger_order("BTC", Side::Sell, Decimal::new(1, 3), Decimal::from(86_000), Tpsl::Sl, None)
    .await?;
```

## Builder Codes

```rust
let builder = Some(("0x1111111111111111111111111111111111111111", 10)); // 10 = 1 bp
let resp = executor.place_order_with_builder(order, builder, None).await?;
let many = executor.bulk_order_with_builder(vec![order_a, order_b], builder, None).await?;
```

The builder fee is in tenths of a basis point and must be pre-approved with `approve_builder_fee`.

## Grouped / OCO Orders

```rust
use hl_types::Grouping;

// Parent entry at index 0, followed by reduce-only TP/SL children.
let responses = executor
    .bulk_order_grouped(vec![parent, take_profit, stop_loss], Grouping::NormalTpsl, None)
    .await?;
```

`bulk_order` keeps the default independent grouping (`Grouping::Na`).

## Cancel and Modify

```rust
executor.cancel_order(asset_index, oid, None).await?;
executor.cancel_by_cloid("BTC", cloid, None).await?;
executor.bulk_cancel(cancels, None).await?;
executor.bulk_cancel_by_cloid(cloid_cancels, None).await?;

executor.modify_order(oid, replacement_order, None).await?;
executor.bulk_modify(modifies, None).await?;
```

## Market, Scale, TWAP, Spot

```rust
use hl_types::{OrderWire, Side, Tif};

let open = executor.market_open("BTC", Side::Buy, size, None, None).await?;
let close = executor.market_close("BTC", Some(size), None, None).await?;
let scale = executor
    .place_scale_order("BTC", true, total_size, low_px, high_px, 5, Tif::Gtc, None)
    .await?;
let twap = executor
    .place_twap_order("BTC", true, size, duration_secs, false, true, None)
    .await?;
executor.cancel_twap("BTC", twap_id, None).await?;

let spot_asset = executor.meta_cache().spot_asset_index("PURR").expect("PURR listed");
let spot_order = OrderWire::limit_buy(spot_asset, px, size).tif(Tif::Gtc).build()?;
let spot = executor.place_spot_order(spot_order, None).await?;
```

`market_close` derives the close side from the live position; `size` is an unsigned magnitude.

## Vault Transfers

```rust
use rust_decimal::Decimal;

executor.deposit_to_vault("0xVaultAddress", Decimal::from(100)).await?;
executor.withdraw_from_vault("0xVaultAddress", Decimal::from(50)).await?;
executor.vault_transfer("0xVaultAddress", false, Decimal::from(25)).await?;
```

`vault_transfer` encodes `usd` as integer micro-units (6 decimals). `transfer_to_vault` remains an alias for `deposit_to_vault`.

## Action Expiry

`set_expires_after` attaches an `expiresAfter` unix epoch timestamp in milliseconds to subsequent L1 actions. The expiry is included in the signed action hash and `/exchange` body.

```rust
use std::time::{SystemTime, UNIX_EPOCH};

let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
executor.set_expires_after(Some(now_ms + 60_000));
// executor.place_order(order, None).await?;
executor.set_expires_after(None);
```

Applies to L1 actions (orders, cancels, leverage, vault transfers). User-signed EIP-712 actions (`usdSend`, `withdraw3`, `spotSend`, `sendAsset`, agent/builder/sub-account) are unaffected.

## Asset Meta Cache

```rust
let cache = executor.meta_cache();
let btc_idx = cache.asset_index("BTC");
let sz_dec = cache.sz_decimals("BTC");
```

Symbols are normalized before lookup (`"BTC-PERP"` → `"BTC"`).

## Position Reconciliation

```rust
use hl_executor::{reconcile_positions, LocalPosition};
use hl_types::PositionSide;
use rust_decimal::Decimal;

let local = vec![LocalPosition {
    id: "pos-1".into(),
    coin: "BTC".into(),
    side: PositionSide::Long,
    size: Decimal::new(5, 1),
}];

let report = reconcile_positions(executor.client(), "0xYourAddress", &local).await?;
```

## Nonce Management

Nonces are generated automatically using a monotonic per-executor counter based on the system clock.

## License

MIT
