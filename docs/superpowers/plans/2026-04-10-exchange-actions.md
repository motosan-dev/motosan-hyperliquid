# Exchange Actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add batch orders, bulk cancel, cancel by CLOID, order amendment, leverage adjustment, and market order helpers to the executor, plus refactor executor into split files with a shared `send_signed_action` helper.

**Architecture:** Split `executor.rs` (single 479-line file) into a module directory (`executor/mod.rs`, `orders.rs`, `cancel.rs`, `modify.rs`, `leverage.rs`, `response.rs`). Extract `send_signed_action` and `resolve_asset` private helpers to DRY up the nonce→sign→post→status-check pipeline. Add new types (`CancelRequest`, `CancelByCloidRequest`, `ModifyRequest`) to `hl-types`. All new methods accept `symbol: &str` and resolve to asset index internally.

**Tech Stack:** Rust, tokio, serde_json, rust_decimal, tracing

**Spec:** `docs/superpowers/specs/2026-04-10-exchange-actions-design.md`

---

## File Structure

| File | Responsibility | Task |
|------|---------------|------|
| `crates/hl-types/src/order.rs` | Modify: add CancelRequest, CancelByCloidRequest, ModifyRequest | 1 |
| `crates/hl-executor/src/executor/mod.rs` | Create: struct def, constructors, helpers, accessors | 2 |
| `crates/hl-executor/src/executor/response.rs` | Create: parse_order_response, parse_bulk_order_response | 2 |
| `crates/hl-executor/src/executor/orders.rs` | Create: place_order, place_order_by_symbol, bulk_order, place_trigger_order, market_open, market_close | 3, 5 |
| `crates/hl-executor/src/executor/cancel.rs` | Create: cancel_order, bulk_cancel, cancel_by_cloid, bulk_cancel_by_cloid | 4 |
| `crates/hl-executor/src/executor/modify.rs` | Create: modify_order, bulk_modify | 6 |
| `crates/hl-executor/src/executor/leverage.rs` | Create: update_leverage, update_isolated_margin | 7 |
| `crates/hl-executor/src/lib.rs` | Modify: update module path and re-exports | 2 |
| `crates/hl-executor/tests/live_test.rs` | Modify: add live tests | 8 |

---

### Task 1: Add New Types to hl-types

**Files:**
- Modify: `crates/hl-types/src/order.rs`

- [ ] **Step 1: Add CancelRequest, CancelByCloidRequest, ModifyRequest types**

Append to `crates/hl-types/src/order.rs` (before the `#[cfg(test)]` block if one exists, otherwise at the end of the file after `TriggerOrderType`):

```rust
/// Request to cancel an order by asset index and server-assigned order ID.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CancelRequest {
    /// Asset index.
    pub asset: u32,
    /// Server-assigned order ID.
    pub oid: u64,
}

impl CancelRequest {
    /// Create a new cancel request.
    pub fn new(asset: u32, oid: u64) -> Self {
        Self { asset, oid }
    }
}

/// Request to cancel an order by asset index and client order ID.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CancelByCloidRequest {
    /// Asset index.
    pub asset: u32,
    /// Client-specified order ID.
    pub cloid: String,
}

impl CancelByCloidRequest {
    /// Create a new cancel-by-CLOID request.
    pub fn new(asset: u32, cloid: impl Into<String>) -> Self {
        Self { asset, cloid: cloid.into() }
    }
}

/// Request to amend an existing order in-place (atomic modification).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModifyRequest {
    /// Server-assigned order ID of the order to modify.
    pub oid: u64,
    /// New order parameters (replaces the existing order).
    pub order: OrderWire,
}

impl ModifyRequest {
    /// Create a new modify request.
    pub fn new(oid: u64, order: OrderWire) -> Self {
        Self { oid, order }
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p hl-types -v`
Expected: All existing tests pass (new types are additive, no tests needed for plain structs)

- [ ] **Step 3: Commit**

```bash
git add crates/hl-types/src/order.rs
git commit -m "feat: add CancelRequest, CancelByCloidRequest, ModifyRequest types"
```

---

### Task 2: Refactor Executor into Module Directory + Shared Helpers

This task splits the single `executor.rs` file into a module directory without changing any public API. All existing tests must continue to pass.

**Files:**
- Delete: `crates/hl-executor/src/executor.rs`
- Create: `crates/hl-executor/src/executor/mod.rs`
- Create: `crates/hl-executor/src/executor/response.rs`
- Create: `crates/hl-executor/src/executor/orders.rs`
- Create: `crates/hl-executor/src/executor/cancel.rs`
- Modify: `crates/hl-executor/src/lib.rs`

- [ ] **Step 1: Create the executor directory and mod.rs**

Create `crates/hl-executor/src/executor/mod.rs` with the struct definition, constructors, shared helpers, and accessors. This is extracted from the current `executor.rs`:

```rust
pub mod cancel;
pub mod orders;
pub mod response;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use hl_client::{HttpTransport, HyperliquidClient};
use hl_signing::{sign_l1_action, Signer};
use hl_types::*;

use crate::meta_cache::AssetMetaCache;

/// Normalize a market symbol to its base coin name.
pub(crate) fn normalize_symbol(symbol: &str) -> String {
    normalize_coin(symbol).to_uppercase()
}

/// The fill-size threshold ratio used to distinguish "filled" from "partial".
pub(crate) const FILL_THRESHOLD: rust_decimal::Decimal =
    rust_decimal::Decimal::from_parts(99, 0, 0, false, 2); // 0.99

/// Standalone order executor for the Hyperliquid L1.
pub struct OrderExecutor {
    pub(crate) client: Arc<dyn HttpTransport>,
    pub(crate) signer: Box<dyn Signer>,
    pub(crate) address: String,
    pub(crate) meta_cache: AssetMetaCache,
    pub(crate) nonce: AtomicU64,
}

impl OrderExecutor {
    /// Create a new executor, loading the asset meta cache from the exchange.
    pub async fn new(
        client: Arc<dyn HttpTransport>,
        signer: Box<dyn Signer>,
        address: String,
    ) -> Result<Self, HlError> {
        let meta_cache = AssetMetaCache::load(client.as_ref()).await?;
        Ok(Self {
            client, signer, address, meta_cache,
            nonce: AtomicU64::new(0),
        })
    }

    /// Convenience constructor that wraps a [`HyperliquidClient`] in an `Arc`.
    pub async fn from_client(
        client: HyperliquidClient,
        signer: Box<dyn Signer>,
        address: String,
    ) -> Result<Self, HlError> {
        Self::new(Arc::new(client), signer, address).await
    }

    /// Create an executor with a pre-built meta cache (avoids the network call).
    pub fn with_meta_cache(
        client: Arc<dyn HttpTransport>,
        signer: Box<dyn Signer>,
        address: String,
        meta_cache: AssetMetaCache,
    ) -> Self {
        Self {
            client, signer, address, meta_cache,
            nonce: AtomicU64::new(0),
        }
    }

    /// Convenience constructor with meta cache that wraps a [`HyperliquidClient`] in an `Arc`.
    pub fn from_client_with_meta_cache(
        client: HyperliquidClient,
        signer: Box<dyn Signer>,
        address: String,
        meta_cache: AssetMetaCache,
    ) -> Self {
        Self::with_meta_cache(Arc::new(client), signer, address, meta_cache)
    }

    /// Generate a monotonically increasing nonce.
    pub(crate) fn next_nonce(&self) -> u64 {
        loop {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before UNIX epoch")
                .as_millis() as u64;
            let prev = self.nonce.load(Ordering::Acquire);
            let next = std::cmp::max(now_ms, prev + 1);
            match self.nonce.compare_exchange_weak(prev, next, Ordering::Release, Ordering::Acquire) {
                Ok(_) => return next,
                Err(_) => continue,
            }
        }
    }

    /// Sign an action and submit it to the exchange. Returns the raw response JSON.
    ///
    /// This is the core pipeline shared by all exchange methods:
    /// nonce → sign → post → status check.
    pub(crate) async fn send_signed_action(
        &self,
        action: serde_json::Value,
        vault: Option<&str>,
    ) -> Result<serde_json::Value, HlError> {
        let nonce = self.next_nonce();
        let sig = sign_l1_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            nonce,
            self.client.is_mainnet(),
            vault,
        )?;
        let result = self.client.post_action(action, &sig, nonce, vault).await?;
        let status = result.get("status").and_then(|s| s.as_str()).unwrap_or("unknown");
        if status != "ok" {
            return Err(HlError::Rejected {
                reason: format!("Exchange rejected action: {}", result),
            });
        }
        Ok(result)
    }

    /// Resolve a human-readable symbol to its exchange asset index.
    pub(crate) fn resolve_asset(&self, symbol: &str) -> Result<u32, HlError> {
        let coin = normalize_symbol(symbol);
        self.meta_cache.asset_index(&coin).ok_or_else(|| {
            HlError::Parse(format!("Asset '{}' not found in exchange universe", symbol))
        })
    }

    /// Borrow the underlying HTTP transport.
    pub fn client(&self) -> &dyn HttpTransport {
        self.client.as_ref()
    }

    /// Return the wallet address used for signing.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Borrow the asset meta cache.
    pub fn meta_cache(&self) -> &AssetMetaCache {
        &self.meta_cache
    }
}
```

- [ ] **Step 2: Create response.rs**

Create `crates/hl-executor/src/executor/response.rs` — extract `parse_order_response` from the old executor.rs:

```rust
use std::str::FromStr;
use rust_decimal::Decimal;
use hl_types::HlError;

/// Parse a single order/fill from a Hyperliquid exchange response.
///
/// Extracts the order ID, average fill price, and total fill size
/// from either "filled" or "resting" status entries.
pub(crate) fn parse_order_response(
    result: &serde_json::Value,
    fallback_price: Decimal,
    fallback_size: Decimal,
) -> Result<(String, Decimal, Decimal), HlError> {
    let status_entry = result
        .get("response")
        .and_then(|r| r.get("data"))
        .and_then(|d| d.get("statuses"))
        .and_then(|s| s.as_array())
        .and_then(|a| a.first());

    parse_single_status(status_entry, fallback_price, fallback_size)
}

/// Parse multiple order statuses from a bulk order response.
///
/// Returns a vec of (order_id, fill_price, fill_size) tuples, one per order.
pub(crate) fn parse_bulk_order_response(
    result: &serde_json::Value,
) -> Result<Vec<(String, Decimal, Decimal)>, HlError> {
    let statuses = result
        .get("response")
        .and_then(|r| r.get("data"))
        .and_then(|d| d.get("statuses"))
        .and_then(|s| s.as_array())
        .ok_or_else(|| HlError::Parse("bulk order: missing statuses array".into()))?;

    let mut results = Vec::with_capacity(statuses.len());
    for entry in statuses {
        let (oid, px, sz) = parse_single_status(Some(entry), Decimal::ZERO, Decimal::ZERO)?;
        results.push((oid, px, sz));
    }
    Ok(results)
}

fn parse_single_status(
    status_entry: Option<&serde_json::Value>,
    fallback_price: Decimal,
    fallback_size: Decimal,
) -> Result<(String, Decimal, Decimal), HlError> {
    if let Some(entry) = status_entry {
        if let Some(filled) = entry.get("filled") {
            let oid = filled
                .get("oid")
                .and_then(|o| o.as_u64())
                .map(|o| o.to_string())
                .unwrap_or_else(|| {
                    let fallback = uuid::Uuid::new_v4().to_string();
                    tracing::warn!(fallback_oid = %fallback, "filled status missing oid, using generated UUID");
                    fallback
                });
            let avg_px = filled.get("avgPx").and_then(|p| p.as_str()).and_then(|s| Decimal::from_str(s).ok());
            let total_sz = filled.get("totalSz").and_then(|s| s.as_str()).and_then(|s| Decimal::from_str(s).ok());
            Ok((oid, avg_px.unwrap_or(fallback_price), total_sz.unwrap_or(fallback_size)))
        } else if let Some(resting) = entry.get("resting") {
            let oid = resting
                .get("oid")
                .and_then(|o| o.as_u64())
                .map(|o| o.to_string())
                .unwrap_or_else(|| {
                    let fallback = uuid::Uuid::new_v4().to_string();
                    tracing::warn!(fallback_oid = %fallback, "resting status missing oid, using generated UUID");
                    fallback
                });
            Ok((oid, fallback_price, Decimal::ZERO))
        } else if let Some(error) = entry.get("error") {
            Err(HlError::Rejected {
                reason: error.as_str().unwrap_or("unknown error").to_string(),
            })
        } else {
            Err(HlError::Parse(format!("unrecognized order status format: {}", entry)))
        }
    } else {
        Err(HlError::Parse("exchange returned ok but statuses array is empty".into()))
    }
}
```

- [ ] **Step 3: Create orders.rs with existing methods**

Create `crates/hl-executor/src/executor/orders.rs` — move `place_order` and `place_trigger_order` here, refactored to use `send_signed_action`:

```rust
use std::str::FromStr;
use rust_decimal::Decimal;
use hl_types::*;

use super::response::parse_order_response;
use super::{OrderExecutor, FILL_THRESHOLD};

impl OrderExecutor {
    /// Build the wire-format JSON for a single order.
    pub(crate) fn order_to_json(order: &OrderWire) -> Result<serde_json::Value, HlError> {
        let mut order_json = serde_json::json!({
            "a": order.asset,
            "b": order.is_buy,
            "p": order.limit_px,
            "s": order.sz,
            "r": order.reduce_only,
            "t": {},
        });

        match &order.order_type {
            OrderTypeWire::Limit(limit) => {
                order_json["t"] = serde_json::json!({ "limit": { "tif": limit.tif.to_string() } });
            }
            OrderTypeWire::Trigger(trigger) => {
                order_json["t"] = serde_json::json!({
                    "trigger": {
                        "triggerPx": trigger.trigger_px,
                        "isMarket": trigger.is_market,
                        "tpsl": trigger.tpsl.to_string(),
                    }
                });
            }
            _ => {
                return Err(HlError::serialization("unknown OrderTypeWire variant"));
            }
        }

        if let Some(ref cloid) = order.cloid {
            order_json["c"] = serde_json::json!(cloid);
        }

        Ok(order_json)
    }

    /// Determine order status from fill size vs requested size.
    pub(crate) fn determine_status(
        fill_size: Decimal,
        requested_size: Decimal,
        order_id: &str,
    ) -> OrderStatus {
        if fill_size >= requested_size * FILL_THRESHOLD {
            OrderStatus::Filled
        } else if fill_size > Decimal::ZERO {
            tracing::warn!(order_id = %order_id, filled = %fill_size, requested = %requested_size, "Partial fill detected");
            OrderStatus::Partial
        } else {
            OrderStatus::Open
        }
    }

    /// Place an order on the Hyperliquid L1.
    #[tracing::instrument(skip(self, order), fields(asset = order.asset, is_buy = order.is_buy))]
    pub async fn place_order(
        &self,
        order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let fallback_price = Decimal::from_str(&order.limit_px).unwrap_or(Decimal::ZERO);
        let fallback_size = Decimal::from_str(&order.sz).unwrap_or(Decimal::ZERO);

        let order_json = Self::order_to_json(&order)?;
        let action = serde_json::json!({
            "type": "order",
            "orders": [order_json],
            "grouping": "na"
        });

        let result = self.send_signed_action(action, vault).await?;
        let (order_id, fill_price, fill_size) = parse_order_response(&result, fallback_price, fallback_size)?;
        let status = Self::determine_status(fill_size, fallback_size, &order_id);

        Ok(OrderResponse::new(
            order_id,
            if fill_size > Decimal::ZERO { Some(fill_price) } else { None },
            fill_size,
            fallback_size,
            status,
        ))
    }

    /// Like `place_order` but resolves the asset index from a symbol string.
    #[tracing::instrument(skip(self, order))]
    pub async fn place_order_by_symbol(
        &self,
        symbol: &str,
        mut order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        order.asset = self.resolve_asset(symbol)?;
        self.place_order(order, vault).await
    }

    /// Place a trigger order (stop-loss or take-profit).
    #[tracing::instrument(skip(self))]
    pub async fn place_trigger_order(
        &self,
        symbol: &str,
        side: Side,
        size: Decimal,
        trigger_price: Decimal,
        tpsl: Tpsl,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let asset_idx = self.resolve_asset(symbol)?;
        let is_buy = side.is_buy();
        let cloid = uuid::Uuid::new_v4().to_string();

        let action = serde_json::json!({
            "type": "order",
            "orders": [{
                "a": asset_idx,
                "b": is_buy,
                "p": trigger_price.to_string(),
                "s": size.to_string(),
                "r": true,
                "t": {
                    "trigger": {
                        "triggerPx": trigger_price.to_string(),
                        "isMarket": true,
                        "tpsl": tpsl.to_string()
                    }
                },
                "c": cloid
            }],
            "grouping": "na"
        });

        let result = self.send_signed_action(action, vault).await?;
        let (order_id, fill_price, fill_size) = parse_order_response(&result, trigger_price, size)?;

        let status = if fill_size < size * FILL_THRESHOLD && fill_size > Decimal::ZERO {
            OrderStatus::Partial
        } else if fill_size == Decimal::ZERO {
            OrderStatus::Open
        } else {
            match tpsl {
                Tpsl::Sl => OrderStatus::TriggerSl,
                Tpsl::Tp => OrderStatus::TriggerTp,
            }
        };

        Ok(OrderResponse::new(
            order_id,
            if fill_size > Decimal::ZERO { Some(fill_price) } else { None },
            fill_size,
            size,
            status,
        ))
    }

    /// Transfer USDC into a vault.
    #[tracing::instrument(skip(self), fields(vault, amount = %amount))]
    pub async fn transfer_to_vault(
        &self,
        vault: &str,
        amount: Decimal,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({
            "type": "vaultTransfer",
            "vaultAddress": vault,
            "isDeposit": true,
            "usd": amount.to_string(),
        });
        // vault transfers do not use the vault parameter for signing
        let result = self.send_signed_action(action, None).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("transfer_to_vault response: {e}")))
    }
}
```

- [ ] **Step 4: Create cancel.rs with existing cancel_order**

Create `crates/hl-executor/src/executor/cancel.rs`:

```rust
use hl_types::*;

use super::OrderExecutor;

impl OrderExecutor {
    /// Cancel an order by asset index and exchange order ID.
    #[tracing::instrument(skip(self), fields(asset, oid))]
    pub async fn cancel_order(
        &self,
        asset: u32,
        oid: u64,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({
            "type": "cancel",
            "cancels": [{"a": asset, "o": oid}]
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("cancel_order response: {e}")))
    }
}
```

- [ ] **Step 5: Delete old executor.rs and update lib.rs**

Delete `crates/hl-executor/src/executor.rs`.

Update `crates/hl-executor/src/lib.rs` — change `pub mod executor;` to match the new module directory. The module system will automatically pick up `executor/mod.rs`. No change to the line `pub mod executor;` is needed — Rust resolves both `executor.rs` and `executor/mod.rs` from `pub mod executor;`.

Verify re-exports still work: `pub use executor::OrderExecutor;` should still resolve.

- [ ] **Step 6: Run full test suite**

Run: `cargo test`
Expected: All existing tests pass — public API is unchanged.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: Clean

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor: split executor into module directory with send_signed_action helper

No public API changes. Extract response parsing to response.rs,
order methods to orders.rs, cancel to cancel.rs. Add send_signed_action
and resolve_asset private helpers to DRY up the sign/post pipeline."
```

---

### Task 3: Bulk Order + Place Order By Symbol

**Files:**
- Modify: `crates/hl-executor/src/executor/orders.rs`
- Modify: `crates/hl-executor/src/executor/mod.rs` (if needed for re-exports)

- [ ] **Step 1: Write unit tests for bulk_order response parsing**

Add tests to `crates/hl-executor/src/executor/response.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_bulk_two_resting() {
        let resp = json!({
            "status": "ok",
            "response": {
                "type": "order",
                "data": {
                    "statuses": [
                        {"resting": {"oid": 111}},
                        {"resting": {"oid": 222}}
                    ]
                }
            }
        });
        let results = parse_bulk_order_response(&resp).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "111");
        assert_eq!(results[1].0, "222");
    }

    #[test]
    fn parse_bulk_mixed_filled_resting() {
        let resp = json!({
            "status": "ok",
            "response": {
                "type": "order",
                "data": {
                    "statuses": [
                        {"filled": {"oid": 111, "avgPx": "90000.0", "totalSz": "0.001"}},
                        {"resting": {"oid": 222}}
                    ]
                }
            }
        });
        let results = parse_bulk_order_response(&resp).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "111");
        assert!(results[0].1 > Decimal::ZERO); // filled has price
        assert_eq!(results[1].0, "222");
        assert_eq!(results[1].2, Decimal::ZERO); // resting has zero fill
    }

    #[test]
    fn parse_bulk_with_error_status() {
        let resp = json!({
            "status": "ok",
            "response": {
                "type": "order",
                "data": {
                    "statuses": [
                        {"resting": {"oid": 111}},
                        {"error": "Insufficient margin"}
                    ]
                }
            }
        });
        let result = parse_bulk_order_response(&resp);
        assert!(result.is_err()); // second order failed
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p hl-executor -- response -v`
Expected: All pass (parse_bulk_order_response is already implemented in Task 2)

- [ ] **Step 3: Implement bulk_order**

Add to `crates/hl-executor/src/executor/orders.rs`:

```rust
use super::response::parse_bulk_order_response;

// Add this method to the existing impl OrderExecutor block:

    /// Place multiple orders in a single signed action.
    #[tracing::instrument(skip(self, orders), fields(count = orders.len()))]
    pub async fn bulk_order(
        &self,
        orders: Vec<OrderWire>,
        vault: Option<&str>,
    ) -> Result<Vec<OrderResponse>, HlError> {
        if orders.is_empty() {
            return Ok(vec![]);
        }

        let mut order_jsons = Vec::with_capacity(orders.len());
        let mut fallbacks: Vec<(Decimal, Decimal)> = Vec::with_capacity(orders.len());

        for order in &orders {
            order_jsons.push(Self::order_to_json(order)?);
            fallbacks.push((
                Decimal::from_str(&order.limit_px).unwrap_or(Decimal::ZERO),
                Decimal::from_str(&order.sz).unwrap_or(Decimal::ZERO),
            ));
        }

        let action = serde_json::json!({
            "type": "order",
            "orders": order_jsons,
            "grouping": "na"
        });

        let result = self.send_signed_action(action, vault).await?;
        let parsed = parse_bulk_order_response(&result)?;

        let mut responses = Vec::with_capacity(parsed.len());
        for (i, (order_id, fill_price, fill_size)) in parsed.into_iter().enumerate() {
            let (_, fallback_size) = fallbacks.get(i).copied().unwrap_or((Decimal::ZERO, Decimal::ZERO));
            let status = Self::determine_status(fill_size, fallback_size, &order_id);
            responses.push(OrderResponse::new(
                order_id,
                if fill_size > Decimal::ZERO { Some(fill_price) } else { None },
                fill_size,
                fallback_size,
                status,
            ));
        }

        Ok(responses)
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p hl-executor -v`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add crates/hl-executor/
git commit -m "feat: add bulk_order and place_order_by_symbol methods"
```

---

### Task 4: Cancel Methods (bulk_cancel, cancel_by_cloid, bulk_cancel_by_cloid)

**Files:**
- Modify: `crates/hl-executor/src/executor/cancel.rs`

- [ ] **Step 1: Implement bulk_cancel, cancel_by_cloid, bulk_cancel_by_cloid**

Add to `crates/hl-executor/src/executor/cancel.rs` inside the existing `impl OrderExecutor` block:

```rust
    /// Cancel multiple orders by asset index and order ID.
    #[tracing::instrument(skip(self, cancels), fields(count = cancels.len()))]
    pub async fn bulk_cancel(
        &self,
        cancels: Vec<CancelRequest>,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let cancel_entries: Vec<serde_json::Value> = cancels
            .iter()
            .map(|c| serde_json::json!({"a": c.asset, "o": c.oid}))
            .collect();

        let action = serde_json::json!({
            "type": "cancel",
            "cancels": cancel_entries
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("bulk_cancel response: {e}")))
    }

    /// Cancel an order by symbol and client order ID.
    #[tracing::instrument(skip(self))]
    pub async fn cancel_by_cloid(
        &self,
        symbol: &str,
        cloid: &str,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let asset = self.resolve_asset(symbol)?;
        let action = serde_json::json!({
            "type": "cancelByCloid",
            "cancels": [{"asset": asset, "cloid": cloid}]
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("cancel_by_cloid response: {e}")))
    }

    /// Cancel multiple orders by asset index and client order ID.
    #[tracing::instrument(skip(self, cancels), fields(count = cancels.len()))]
    pub async fn bulk_cancel_by_cloid(
        &self,
        cancels: Vec<CancelByCloidRequest>,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let cancel_entries: Vec<serde_json::Value> = cancels
            .iter()
            .map(|c| serde_json::json!({"asset": c.asset, "cloid": c.cloid}))
            .collect();

        let action = serde_json::json!({
            "type": "cancelByCloid",
            "cancels": cancel_entries
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("bulk_cancel_by_cloid response: {e}")))
    }
```

- [ ] **Step 2: Run tests and clippy**

Run: `cargo test -p hl-executor -v && cargo clippy --all-targets -- -D warnings`
Expected: All pass, clean

- [ ] **Step 3: Commit**

```bash
git add crates/hl-executor/src/executor/cancel.rs
git commit -m "feat: add bulk_cancel, cancel_by_cloid, bulk_cancel_by_cloid"
```

---

### Task 5: Market Order Helpers (market_open, market_close)

**Files:**
- Modify: `crates/hl-executor/src/executor/orders.rs`

- [ ] **Step 1: Write slippage calculation tests**

Add to `crates/hl-executor/src/executor/orders.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slippage_buy_increases_price() {
        let mid = Decimal::from(90000);
        let slippage = Decimal::new(5, 2); // 0.05 = 5%
        let limit = mid * (Decimal::ONE + slippage);
        assert_eq!(limit, Decimal::from(94500));
    }

    #[test]
    fn slippage_sell_decreases_price() {
        let mid = Decimal::from(90000);
        let slippage = Decimal::new(5, 2); // 0.05 = 5%
        let limit = mid * (Decimal::ONE - slippage);
        assert_eq!(limit, Decimal::from(85500));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p hl-executor -- slippage -v`
Expected: PASS

- [ ] **Step 3: Implement market_open and market_close**

Add to the `impl OrderExecutor` block in `crates/hl-executor/src/executor/orders.rs`:

```rust
    /// Default slippage for market orders (5%).
    const DEFAULT_SLIPPAGE: Decimal = Decimal::new(5, 2); // 0.05

    /// Place a market order (IOC limit at slippage-adjusted price).
    ///
    /// Fetches the current mid-price, applies slippage (default 5%), and submits
    /// an IOC limit order. Returns the fill result.
    #[tracing::instrument(skip(self))]
    pub async fn market_open(
        &self,
        symbol: &str,
        side: Side,
        size: Decimal,
        slippage: Option<Decimal>,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let asset_idx = self.resolve_asset(symbol)?;
        let slip = slippage.unwrap_or(Self::DEFAULT_SLIPPAGE);

        // Fetch mid-price from orderbook
        let book_resp = self.client.post_info(serde_json::json!({
            "type": "l2Book",
            "coin": normalize_symbol(symbol)
        })).await?;

        let mid_price = Self::extract_mid_price(&book_resp)?;

        let limit_px = if side.is_buy() {
            mid_price * (Decimal::ONE + slip)
        } else {
            mid_price * (Decimal::ONE - slip)
        };

        let order = OrderWire::limit_buy(asset_idx, &limit_px.to_string(), &size.to_string())
            .tif(Tif::Ioc)
            .cloid(hl_client::HyperliquidClient::generate_cloid())
            .build();

        // Fix side if selling
        let order = if !side.is_buy() {
            let mut o = order;
            o.is_buy = false;
            o
        } else {
            order
        };

        self.place_order(order, vault).await
    }

    /// Close an open position via market order.
    ///
    /// If `size` is `None`, queries the current position and closes it entirely.
    /// Uses IOC limit at slippage-adjusted price (default 5%).
    #[tracing::instrument(skip(self))]
    pub async fn market_close(
        &self,
        symbol: &str,
        size: Option<Decimal>,
        slippage: Option<Decimal>,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let close_size = match size {
            Some(sz) => sz,
            None => {
                // Query current position
                let state_resp = self.client.post_info(serde_json::json!({
                    "type": "clearinghouseState",
                    "user": self.address
                })).await?;
                let coin = normalize_symbol(symbol);
                let position_size = state_resp
                    .get("assetPositions")
                    .and_then(|ap| ap.as_array())
                    .and_then(|positions| {
                        positions.iter().find_map(|p| {
                            let pos = p.get("position")?;
                            let pos_coin = pos.get("coin")?.as_str()?;
                            if pos_coin == coin {
                                pos.get("szi")?.as_str()?.parse::<Decimal>().ok()
                            } else {
                                None
                            }
                        })
                    })
                    .ok_or_else(|| HlError::Parse(format!("No open position found for {}", symbol)))?;
                position_size.abs()
            }
        };

        if close_size == Decimal::ZERO {
            return Err(HlError::Parse(format!("Position size is zero for {}", symbol)));
        }

        // Determine close side: if position is positive (long), sell to close
        let state_resp = self.client.post_info(serde_json::json!({
            "type": "clearinghouseState",
            "user": self.address
        })).await?;
        let coin = normalize_symbol(symbol);
        let position_szi = state_resp
            .get("assetPositions")
            .and_then(|ap| ap.as_array())
            .and_then(|positions| {
                positions.iter().find_map(|p| {
                    let pos = p.get("position")?;
                    let pos_coin = pos.get("coin")?.as_str()?;
                    if pos_coin == coin {
                        pos.get("szi")?.as_str()?.parse::<Decimal>().ok()
                    } else {
                        None
                    }
                })
            })
            .ok_or_else(|| HlError::Parse(format!("No open position found for {}", symbol)))?;

        let close_side = if position_szi > Decimal::ZERO { Side::Sell } else { Side::Buy };

        self.market_open(symbol, close_side, close_size, slippage, vault).await
    }

    /// Extract mid-price from an l2Book response.
    fn extract_mid_price(book_resp: &serde_json::Value) -> Result<Decimal, HlError> {
        let levels = book_resp.get("levels")
            .and_then(|l| l.as_array())
            .ok_or_else(|| HlError::Parse("l2Book missing levels".into()))?;

        if levels.len() < 2 {
            return Err(HlError::Parse("l2Book levels has fewer than 2 entries".into()));
        }

        let best_bid = levels[0].as_array()
            .and_then(|bids| bids.first())
            .and_then(|b| b.get("px"))
            .and_then(|px| px.as_str())
            .and_then(|s| Decimal::from_str(s).ok())
            .ok_or_else(|| HlError::Parse("l2Book missing best bid".into()))?;

        let best_ask = levels[1].as_array()
            .and_then(|asks| asks.first())
            .and_then(|a| a.get("px"))
            .and_then(|px| px.as_str())
            .and_then(|s| Decimal::from_str(s).ok())
            .ok_or_else(|| HlError::Parse("l2Book missing best ask".into()))?;

        Ok((best_bid + best_ask) / Decimal::from(2))
    }
```

Note: `market_close` queries the position twice if `size` is `None` (once for size, once for direction). This can be optimized later but keeps the code simple for now.

- [ ] **Step 4: Run tests**

Run: `cargo test -p hl-executor -v && cargo clippy --all-targets -- -D warnings`
Expected: All pass, clean

- [ ] **Step 5: Commit**

```bash
git add crates/hl-executor/src/executor/orders.rs
git commit -m "feat: add market_open and market_close with auto slippage"
```

---

### Task 6: Order Modification (modify_order, bulk_modify)

**Files:**
- Create: `crates/hl-executor/src/executor/modify.rs`
- Modify: `crates/hl-executor/src/executor/mod.rs`

- [ ] **Step 1: Create modify.rs**

Create `crates/hl-executor/src/executor/modify.rs`:

```rust
use std::str::FromStr;
use rust_decimal::Decimal;
use hl_types::*;

use super::response::{parse_order_response, parse_bulk_order_response};
use super::OrderExecutor;

impl OrderExecutor {
    /// Modify an existing order in-place (atomic amendment).
    ///
    /// Provides the server-assigned `oid` and the new order parameters.
    /// This is a single atomic operation — not cancel+replace.
    #[tracing::instrument(skip(self, new_order), fields(oid))]
    pub async fn modify_order(
        &self,
        oid: u64,
        new_order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let fallback_price = Decimal::from_str(&new_order.limit_px).unwrap_or(Decimal::ZERO);
        let fallback_size = Decimal::from_str(&new_order.sz).unwrap_or(Decimal::ZERO);

        let order_json = Self::order_to_json(&new_order)?;
        let action = serde_json::json!({
            "type": "batchModify",
            "modifies": [{"oid": oid, "order": order_json}]
        });

        let result = self.send_signed_action(action, vault).await?;
        let (order_id, fill_price, fill_size) = parse_order_response(&result, fallback_price, fallback_size)?;
        let status = Self::determine_status(fill_size, fallback_size, &order_id);

        Ok(OrderResponse::new(
            order_id,
            if fill_size > Decimal::ZERO { Some(fill_price) } else { None },
            fill_size,
            fallback_size,
            status,
        ))
    }

    /// Modify multiple orders in a single signed action.
    #[tracing::instrument(skip(self, modifies), fields(count = modifies.len()))]
    pub async fn bulk_modify(
        &self,
        modifies: Vec<ModifyRequest>,
        vault: Option<&str>,
    ) -> Result<Vec<OrderResponse>, HlError> {
        if modifies.is_empty() {
            return Ok(vec![]);
        }

        let mut modify_jsons = Vec::with_capacity(modifies.len());
        let mut fallbacks: Vec<(Decimal, Decimal)> = Vec::with_capacity(modifies.len());

        for m in &modifies {
            let order_json = Self::order_to_json(&m.order)?;
            modify_jsons.push(serde_json::json!({"oid": m.oid, "order": order_json}));
            fallbacks.push((
                Decimal::from_str(&m.order.limit_px).unwrap_or(Decimal::ZERO),
                Decimal::from_str(&m.order.sz).unwrap_or(Decimal::ZERO),
            ));
        }

        let action = serde_json::json!({
            "type": "batchModify",
            "modifies": modify_jsons
        });

        let result = self.send_signed_action(action, vault).await?;
        let parsed = parse_bulk_order_response(&result)?;

        let mut responses = Vec::with_capacity(parsed.len());
        for (i, (order_id, fill_price, fill_size)) in parsed.into_iter().enumerate() {
            let (_, fallback_size) = fallbacks.get(i).copied().unwrap_or((Decimal::ZERO, Decimal::ZERO));
            let status = Self::determine_status(fill_size, fallback_size, &order_id);
            responses.push(OrderResponse::new(
                order_id,
                if fill_size > Decimal::ZERO { Some(fill_price) } else { None },
                fill_size,
                fallback_size,
                status,
            ));
        }

        Ok(responses)
    }
}
```

- [ ] **Step 2: Add modify module to mod.rs**

In `crates/hl-executor/src/executor/mod.rs`, add:

```rust
pub mod modify;
```

- [ ] **Step 3: Run tests and clippy**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: All pass, clean

- [ ] **Step 4: Commit**

```bash
git add crates/hl-executor/src/executor/modify.rs crates/hl-executor/src/executor/mod.rs
git commit -m "feat: add modify_order and bulk_modify for atomic order amendment"
```

---

### Task 7: Leverage & Margin (update_leverage, update_isolated_margin)

**Files:**
- Create: `crates/hl-executor/src/executor/leverage.rs`
- Modify: `crates/hl-executor/src/executor/mod.rs`

- [ ] **Step 1: Create leverage.rs**

Create `crates/hl-executor/src/executor/leverage.rs`:

```rust
use rust_decimal::Decimal;
use hl_types::*;

use super::OrderExecutor;

impl OrderExecutor {
    /// Update leverage for an asset.
    ///
    /// `is_cross` controls the margin mode: `true` for cross-margin, `false` for isolated.
    #[tracing::instrument(skip(self))]
    pub async fn update_leverage(
        &self,
        symbol: &str,
        leverage: u32,
        is_cross: bool,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let asset = self.resolve_asset(symbol)?;
        let action = serde_json::json!({
            "type": "updateLeverage",
            "asset": asset,
            "isCross": is_cross,
            "leverage": leverage
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("update_leverage response: {e}")))
    }

    /// Adjust isolated margin for a position.
    ///
    /// Positive `amount` adds margin, negative removes margin.
    /// Amount is in USDC.
    #[tracing::instrument(skip(self))]
    pub async fn update_isolated_margin(
        &self,
        symbol: &str,
        amount: Decimal,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let asset = self.resolve_asset(symbol)?;
        let is_buy = amount > Decimal::ZERO;
        // Convert to micro-units (multiply by 1_000_000)
        let ntli = (amount.abs() * Decimal::from(1_000_000))
            .to_string()
            .parse::<i64>()
            .map_err(|e| HlError::Parse(format!("margin amount conversion: {e}")))?;

        let action = serde_json::json!({
            "type": "updateIsolatedMargin",
            "asset": asset,
            "isBuy": is_buy,
            "ntli": ntli
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("update_isolated_margin response: {e}")))
    }
}
```

- [ ] **Step 2: Add leverage module to mod.rs**

In `crates/hl-executor/src/executor/mod.rs`, add:

```rust
pub mod leverage;
```

- [ ] **Step 3: Run tests and clippy**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: All pass, clean

- [ ] **Step 4: Commit**

```bash
git add crates/hl-executor/src/executor/leverage.rs crates/hl-executor/src/executor/mod.rs
git commit -m "feat: add update_leverage and update_isolated_margin"
```

---

### Task 8: Live Integration Tests

**Files:**
- Modify: `crates/hl-executor/tests/live_test.rs`

- [ ] **Step 1: Add live tests**

Add to `crates/hl-executor/tests/live_test.rs` (which already has `setup()` and existing live tests):

```rust
/// Place 2 resting orders via bulk_order, then bulk cancel them.
#[tokio::test]
async fn live_bulk_order_and_cancel() {
    let (client, signer, address) = setup();
    let executor = OrderExecutor::from_client(client, signer, address)
        .await
        .expect("executor construction failed");

    let btc_idx = executor.meta_cache().asset_index("BTC").expect("BTC should exist");

    // Two orders at extreme prices — will never fill
    let order1 = hl_types::OrderWire::limit_buy(btc_idx, "1.0", "0.001")
        .tif(hl_types::Tif::Gtc)
        .cloid(hl_client::HyperliquidClient::generate_cloid())
        .build();

    let order2 = hl_types::OrderWire::limit_buy(btc_idx, "2.0", "0.001")
        .tif(hl_types::Tif::Gtc)
        .cloid(hl_client::HyperliquidClient::generate_cloid())
        .build();

    let responses = executor.bulk_order(vec![order1, order2], None).await;
    assert!(responses.is_ok(), "bulk_order failed: {:?}", responses.err());

    let responses = responses.unwrap();
    assert_eq!(responses.len(), 2);

    // Bulk cancel
    let cancels: Vec<hl_types::CancelRequest> = responses
        .iter()
        .map(|r| hl_types::CancelRequest::new(btc_idx, r.order_id.parse().unwrap()))
        .collect();

    let cancel_result = executor.bulk_cancel(cancels, None).await;
    assert!(cancel_result.is_ok(), "bulk_cancel failed: {:?}", cancel_result.err());
}

/// Place an order with CLOID, then cancel by CLOID.
#[tokio::test]
async fn live_cancel_by_cloid() {
    let (client, signer, address) = setup();
    let executor = OrderExecutor::from_client(client, signer, address)
        .await
        .expect("executor construction failed");

    let btc_idx = executor.meta_cache().asset_index("BTC").expect("BTC should exist");
    let cloid = hl_client::HyperliquidClient::generate_cloid();

    let order = hl_types::OrderWire::limit_buy(btc_idx, "1.0", "0.001")
        .tif(hl_types::Tif::Gtc)
        .cloid(cloid.clone())
        .build();

    let resp = executor.place_order(order, None).await;
    assert!(resp.is_ok(), "place_order failed: {:?}", resp.err());

    // Cancel by CLOID
    let cancel = executor.cancel_by_cloid("BTC", &cloid, None).await;
    assert!(cancel.is_ok(), "cancel_by_cloid failed: {:?}", cancel.err());
}

/// Modify a resting order's price.
#[tokio::test]
async fn live_modify_order() {
    let (client, signer, address) = setup();
    let executor = OrderExecutor::from_client(client, signer, address)
        .await
        .expect("executor construction failed");

    let btc_idx = executor.meta_cache().asset_index("BTC").expect("BTC should exist");

    // Place resting order
    let order = hl_types::OrderWire::limit_buy(btc_idx, "1.0", "0.001")
        .tif(hl_types::Tif::Gtc)
        .build();

    let resp = executor.place_order(order, None).await.expect("place_order failed");
    let oid: u64 = resp.order_id.parse().expect("order_id should be numeric");

    // Modify to different price
    let new_order = hl_types::OrderWire::limit_buy(btc_idx, "2.0", "0.001")
        .tif(hl_types::Tif::Gtc)
        .build();

    let modify_resp = executor.modify_order(oid, new_order, None).await;
    assert!(modify_resp.is_ok(), "modify_order failed: {:?}", modify_resp.err());

    // Clean up
    let new_oid: u64 = modify_resp.unwrap().order_id.parse().unwrap();
    let _ = executor.cancel_order(btc_idx, new_oid, None).await;
}

/// Update leverage to 5x and back to 10x.
#[tokio::test]
async fn live_update_leverage() {
    let (client, signer, address) = setup();
    let executor = OrderExecutor::from_client(client, signer, address)
        .await
        .expect("executor construction failed");

    let resp1 = executor.update_leverage("BTC", 5, true, None).await;
    assert!(resp1.is_ok(), "update_leverage to 5x failed: {:?}", resp1.err());

    let resp2 = executor.update_leverage("BTC", 10, true, None).await;
    assert!(resp2.is_ok(), "update_leverage to 10x failed: {:?}", resp2.err());
}
```

- [ ] **Step 2: Run unit tests (not live)**

Run: `cargo test -p hl-executor -v`
Expected: All unit tests pass. Live tests are gated behind `live-test` feature.

- [ ] **Step 3: Update re-exports in lib.rs if needed**

Ensure `crates/hl-executor/src/lib.rs` re-exports the new types:

```rust
pub use executor::OrderExecutor;
// The new methods are on OrderExecutor, so no additional re-exports needed.
// But if CancelRequest etc. need to be public from hl-executor, add:
// (They are already in hl-types, so users import from there)
```

- [ ] **Step 4: Run full test suite**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: All pass, clean

- [ ] **Step 5: Commit**

```bash
git add crates/hl-executor/
git commit -m "test: add live integration tests for bulk order, cancel by cloid, modify, leverage"
```
