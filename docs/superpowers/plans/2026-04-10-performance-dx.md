# Performance & DX Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve developer experience with Decimal-accepting builders, validated build(), expanded prelude, new examples, and optimized meta cache lookups.

**Architecture:** 4 independent tasks touching hl-types (builder), hl-executor (cache + call sites), motosan-hyperliquid (prelude), and examples/. Each task produces a self-contained commit.

**Tech Stack:** Rust, rust_decimal

**Spec:** `docs/superpowers/specs/2026-04-10-performance-dx-design.md`

---

## File Structure

| File | Responsibility | Task |
|------|---------------|------|
| `crates/hl-types/src/order.rs` | Modify: builder accepts Decimal, build() validates, Side::from_is_buy | 1 |
| `crates/hl-executor/src/executor/orders.rs` | Modify: update build() call sites to handle Result | 1 |
| `crates/hl-executor/src/executor/modify.rs` | Modify: update if any build() calls exist | 1 |
| `crates/hl-executor/tests/live_test.rs` | Modify: update build() calls | 1 |
| `crates/hl-executor/src/meta_cache.rs` | Modify: add _normalized lookup variants | 2 |
| `crates/hl-executor/src/executor/mod.rs` | Modify: use _normalized in resolve_asset | 2 |
| `crates/motosan-hyperliquid/src/prelude.rs` | Modify: expand re-exports | 3 |
| `examples/ws_stream.rs` | Create: WebSocket typed streaming example | 4 |
| `examples/trigger_order.rs` | Create: trigger order example | 4 |

---

### Task 1: Builder Accepts Decimal + Validates + Side::from_is_buy

**Files:**
- Modify: `crates/hl-types/src/order.rs`
- Modify: `crates/hl-executor/src/executor/orders.rs`
- Modify: `crates/hl-executor/tests/live_test.rs`

- [ ] **Step 1: Change builder methods to accept Decimal**

Read `crates/hl-types/src/order.rs`. Change the 4 builder constructors on `OrderWire`:

```rust
// Before:
pub fn limit_buy(asset: u32, limit_px: impl Into<String>, sz: impl Into<String>) -> OrderWireBuilder

// After:
pub fn limit_buy(asset: u32, limit_px: Decimal, sz: Decimal) -> OrderWireBuilder
```

Inside each constructor, convert to String: `limit_px: limit_px.to_string()`, `sz: sz.to_string()`.

Apply the same change to `limit_sell`, `trigger_buy`, `trigger_sell`. For trigger variants, also store `trigger_px` as `Decimal` parameter and convert: `trigger_px: trigger_px.to_string()`.

Update the doc example on `limit_buy` to use Decimal:
```rust
/// ```
/// use hl_types::{OrderWire, Tif, Decimal};
/// use std::str::FromStr;
///
/// let order = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from_str("0.001").unwrap())
///     .tif(Tif::Gtc)
///     .build()
///     .unwrap();
///
/// assert!(order.is_buy);
/// ```
```

- [ ] **Step 2: Change build() to return Result**

Change `build()` in `OrderWireBuilder`:

```rust
pub fn build(self) -> Result<OrderWire, HlError> {
    // Validate price
    let px: Decimal = self.limit_px.parse()
        .map_err(|_| HlError::Parse(format!("invalid price: {}", self.limit_px)))?;
    if px <= Decimal::ZERO {
        return Err(HlError::Parse(format!("price must be positive, got: {}", self.limit_px)));
    }

    // Validate size
    let sz: Decimal = self.sz.parse()
        .map_err(|_| HlError::Parse(format!("invalid size: {}", self.sz)))?;
    if sz <= Decimal::ZERO {
        return Err(HlError::Parse(format!("size must be positive, got: {}", self.sz)));
    }

    Ok(OrderWire {
        asset: self.asset,
        is_buy: self.is_buy,
        limit_px: self.limit_px,
        sz: self.sz,
        reduce_only: self.reduce_only,
        order_type: self.order_type,
        cloid: self.cloid,
    })
}
```

Add `use std::str::FromStr;` and `use rust_decimal::Decimal;` at the top of order.rs if not already present.

- [ ] **Step 3: Add Side::from_is_buy**

In the `impl Side` block in order.rs, add:

```rust
/// Create a side from a boolean (true = Buy, false = Sell).
pub fn from_is_buy(is_buy: bool) -> Self {
    if is_buy { Side::Buy } else { Side::Sell }
}
```

- [ ] **Step 4: Add validation tests**

Add to the `#[cfg(test)] mod tests` block in order.rs:

```rust
#[test]
fn build_validates_positive_price() {
    let result = OrderWire::limit_buy(0, Decimal::ZERO, Decimal::ONE).build();
    assert!(result.is_err());
}

#[test]
fn build_validates_positive_size() {
    let result = OrderWire::limit_buy(0, Decimal::ONE, Decimal::ZERO).build();
    assert!(result.is_err());
}

#[test]
fn build_validates_negative_price() {
    let result = OrderWire::limit_buy(0, Decimal::from(-1), Decimal::ONE).build();
    assert!(result.is_err());
}

#[test]
fn build_success() {
    let result = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from_str("0.001").unwrap()).build();
    assert!(result.is_ok());
    let order = result.unwrap();
    assert_eq!(order.limit_px, "90000");
    assert_eq!(order.sz, "0.001");
}

#[test]
fn side_from_is_buy() {
    assert_eq!(Side::from_is_buy(true), Side::Buy);
    assert_eq!(Side::from_is_buy(false), Side::Sell);
}
```

- [ ] **Step 5: Update all build() call sites across the codebase**

Search for `.build()` in hl-executor and hl-types. Every `.build()` becomes `.build()?` or `.build().unwrap()` (in tests). Key files:
- `crates/hl-executor/src/executor/orders.rs` — `market_open`, `market_close` use builders
- `crates/hl-executor/tests/live_test.rs` — all test order construction
- `crates/hl-types/src/order.rs` — existing tests within the file

Run `grep -rn '\.build()' crates/` to find all sites.

For tests, use `.build().unwrap()`. For production code, use `.build()?`.

Also update all builder call sites from string to Decimal:
```rust
// Before:
OrderWire::limit_buy(btc_idx, "1.0", "0.001")

// After:
OrderWire::limit_buy(btc_idx, Decimal::from(1), Decimal::from_str("0.001").unwrap())
```

In test code, `Decimal::from(1)` is fine for whole numbers. For decimals, use `Decimal::from_str("0.001").unwrap()`.

In production code (orders.rs market_open/market_close), the values are already `Decimal` — just pass them directly.

- [ ] **Step 6: Verify**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: All pass, clean.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: builder accepts Decimal, build() validates, add Side::from_is_buy

Breaking: limit_buy/limit_sell/trigger_buy/trigger_sell now accept Decimal.
Breaking: build() returns Result<OrderWire, HlError> with price/size validation."
```

---

### Task 2: Meta Cache Optimization

**Files:**
- Modify: `crates/hl-executor/src/meta_cache.rs`
- Modify: `crates/hl-executor/src/executor/mod.rs`

- [ ] **Step 1: Add _normalized variants to meta_cache**

Read `crates/hl-executor/src/meta_cache.rs`. Add after the existing `asset_index` and `sz_decimals` methods:

```rust
/// Resolve a pre-normalized (uppercased) coin name to its asset index.
///
/// Unlike [`asset_index`], this does NOT uppercase the input.
/// Use this when you have already normalized the coin name.
pub(crate) fn asset_index_normalized(&self, coin: &str) -> Option<u32> {
    self.coin_to_index.get(coin).copied()
}

/// Look up size-decimal precision for a pre-normalized (uppercased) coin name.
pub(crate) fn sz_decimals_normalized(&self, coin: &str) -> Option<u32> {
    self.coin_to_sz_decimals.get(coin).copied()
}
```

- [ ] **Step 2: Use _normalized in resolve_asset**

In `crates/hl-executor/src/executor/mod.rs`, find `resolve_asset`. Change:

```rust
// Before:
pub(crate) fn resolve_asset(&self, symbol: &str) -> Result<u32, HlError> {
    let coin = normalize_symbol(symbol);
    self.meta_cache.asset_index(&coin).ok_or_else(|| {

// After:
pub(crate) fn resolve_asset(&self, symbol: &str) -> Result<u32, HlError> {
    let coin = normalize_symbol(symbol); // already uppercased
    self.meta_cache.asset_index_normalized(&coin).ok_or_else(|| {
```

- [ ] **Step 3: Add test**

In the existing test module in `meta_cache.rs`, add:

```rust
#[test]
fn asset_index_normalized_exact_match() {
    let cache = test_cache();
    assert_eq!(cache.asset_index_normalized("BTC"), Some(0));
}

#[test]
fn asset_index_normalized_wrong_case_fails() {
    let cache = test_cache();
    assert_eq!(cache.asset_index_normalized("btc"), None); // not uppercased
}
```

- [ ] **Step 4: Verify**

Run: `cargo test -p hl-executor -v && cargo clippy --all-targets -- -D warnings`
Expected: All pass, clean.

- [ ] **Step 5: Commit**

```bash
git add crates/hl-executor/
git commit -m "perf: add asset_index_normalized to skip redundant to_uppercase in hot path"
```

---

### Task 3: Prelude Expansion

**Files:**
- Modify: `crates/motosan-hyperliquid/src/prelude.rs`

- [ ] **Step 1: Expand prelude re-exports**

Read `crates/motosan-hyperliquid/src/prelude.rs`. Add the missing types:

```rust
// -- Types (always available) ------------------------------------------------

pub use hl_types::{
    // Existing:
    Decimal, HlAccountState, HlAssetInfo, HlCandle, HlError, HlFill, HlFundingRate, HlOrderbook,
    HlPosition, OrderWire, OrderWireBuilder, Side, Signature,
    // New:
    CancelByCloidRequest, CancelRequest, HlActionResponse, ModifyRequest,
    OrderStatus, PositionSide, Tif, Tpsl, normalize_coin,
};
```

Also add WsMessage and Subscription to the ws section:

```rust
// -- WebSocket ---------------------------------------------------------------

#[cfg(feature = "ws")]
pub use hl_client::{HyperliquidWs, Subscription, WsConfig, WsMessage};
```

- [ ] **Step 2: Verify**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: All pass, clean.

- [ ] **Step 3: Commit**

```bash
git add crates/motosan-hyperliquid/src/prelude.rs
git commit -m "feat: expand prelude with Tif, Tpsl, OrderStatus, CancelRequest, Subscription, etc."
```

---

### Task 4: New Examples

**Files:**
- Create: `examples/ws_stream.rs`
- Create: `examples/trigger_order.rs`

- [ ] **Step 1: Create ws_stream.rs**

Create `examples/ws_stream.rs`:

```rust
//! WebSocket streaming example — subscribe to L2 orderbook and print typed messages.
//!
//! Run: `cargo run --example ws_stream --features ws`

use hl_client::{HyperliquidWs, WsMessage};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ws = HyperliquidWs::testnet();
    ws.connect().await?;

    // Subscribe to BTC orderbook using convenience method
    ws.subscribe_l2_book("BTC").await?;
    println!("Subscribed to BTC L2 book. Listening for 10 seconds...\n");

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);

    loop {
        tokio::select! {
            msg = ws.next_typed_message() => {
                match msg {
                    Some(Ok(WsMessage::L2Book(data))) => {
                        println!("L2Book update: coin={}, time={}", data.coin, data.time);
                    }
                    Some(Ok(WsMessage::SubscriptionResponse)) => {
                        println!("Subscription confirmed.");
                    }
                    Some(Ok(WsMessage::Unknown(_))) => {
                        // Ignore unknown messages
                    }
                    Some(Ok(other)) => {
                        println!("Other message: {:?}", other);
                    }
                    Some(Err(e)) => {
                        eprintln!("Error: {e}");
                        break;
                    }
                    None => {
                        println!("Connection closed.");
                        break;
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                println!("\nDone — 10 seconds elapsed.");
                break;
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Create trigger_order.rs**

Create `examples/trigger_order.rs`:

```rust
//! Trigger order example — place a stop-loss and then cancel it.
//!
//! Run: `HYPERLIQUID_TESTNET_KEY=0x... cargo run --example trigger_order`

use std::str::FromStr;

use hl_client::HyperliquidClient;
use hl_executor::OrderExecutor;
use hl_signing::PrivateKeySigner;
use hl_types::{Side, Tpsl};
use rust_decimal::Decimal;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::var("HYPERLIQUID_TESTNET_KEY")
        .expect("Set HYPERLIQUID_TESTNET_KEY to run this example");

    let client = HyperliquidClient::testnet()?;
    let signer = PrivateKeySigner::from_hex(&key)?;
    let address = signer.address().to_string();

    println!("Address: {address}");

    let executor = OrderExecutor::from_client(client, Box::new(signer), address).await?;

    // Place a stop-loss trigger order at a very low price (will never fire)
    let trigger_price = Decimal::from_str("1000.0")?;
    let size = Decimal::from_str("0.001")?;

    println!("Placing BTC stop-loss at ${trigger_price} (size={size})...");

    let resp = executor
        .place_trigger_order("BTC", Side::Sell, size, trigger_price, Tpsl::Sl, None)
        .await?;

    println!("Order placed: id={}, status={}", resp.order_id, resp.status);

    // Cancel it
    let btc_idx = executor.meta_cache().asset_index("BTC").unwrap();
    if let Ok(oid) = resp.order_id.parse::<u64>() {
        println!("Cancelling order {oid}...");
        let cancel = executor.cancel_order(btc_idx, oid, None).await?;
        println!("Cancel result: status={}", cancel.status);
    }

    Ok(())
}
```

- [ ] **Step 3: Verify examples compile**

Run: `cargo build --examples --features ws`
Expected: Compiles without errors.

Run: `cargo clippy --all-targets --features ws -- -D warnings`
Expected: Clean.

- [ ] **Step 4: Commit**

```bash
git add examples/
git commit -m "docs: add WebSocket streaming and trigger order examples"
```
