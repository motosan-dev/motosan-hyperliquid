# Exchange Actions — Design Spec

> Sub-project 1 of Phase 2 (High-Value Features). Add batch orders, bulk cancel, cancel by CLOID, order amendment, leverage adjustment, market order helpers, and unified symbol-based API to the executor.

## Context

The SDK currently supports single `place_order`, `cancel_order`, `place_trigger_order`, and `transfer_to_vault`. Compared to the official `hyperliquid-rust-sdk`, we are missing 10+ exchange actions that are critical for production trading strategies (grid trading, portfolio rebalance, risk management).

Reference: https://github.com/hyperliquid-dex/hyperliquid-rust-sdk

## Scope

10 new public methods on `OrderExecutor`, plus internal refactoring (extract `send_signed_action` helper, split executor into multiple files).

Out of scope: WebSocket user events (Sub-project 2), USDC transfer / approve agent / schedule cancel (Sub-project 3), spot trading.

---

## New Methods

### Orders

| Method | Signature | Description |
|--------|-----------|-------------|
| `bulk_order` | `(orders: Vec<OrderWire>, vault: Option<&str>) -> Result<Vec<OrderResponse>, HlError>` | Place multiple orders in a single signed action |
| `market_open` | `(symbol: &str, side: Side, size: Decimal, slippage: Option<Decimal>, vault: Option<&str>) -> Result<OrderResponse, HlError>` | Market buy/sell with automatic slippage-adjusted IOC limit price |
| `market_close` | `(symbol: &str, size: Option<Decimal>, slippage: Option<Decimal>, vault: Option<&str>) -> Result<OrderResponse, HlError>` | Close position. `size=None` closes full position. Queries current position size internally. |
| `place_order_by_symbol` | `(symbol: &str, order: OrderWire, vault: Option<&str>) -> Result<OrderResponse, HlError>` | Like `place_order` but resolves asset index from symbol |

### Cancel

| Method | Signature | Description |
|--------|-----------|-------------|
| `bulk_cancel` | `(cancels: Vec<CancelRequest>, vault: Option<&str>) -> Result<HlActionResponse, HlError>` | Cancel multiple orders by asset + OID |
| `cancel_by_cloid` | `(symbol: &str, cloid: &str, vault: Option<&str>) -> Result<HlActionResponse, HlError>` | Cancel by client order ID |
| `bulk_cancel_by_cloid` | `(cancels: Vec<CancelByCloidRequest>, vault: Option<&str>) -> Result<HlActionResponse, HlError>` | Bulk cancel by CLOID |

### Modify

| Method | Signature | Description |
|--------|-----------|-------------|
| `modify_order` | `(oid: u64, new_order: OrderWire, vault: Option<&str>) -> Result<OrderResponse, HlError>` | Atomic order amendment (not cancel+replace) |
| `bulk_modify` | `(modifies: Vec<ModifyRequest>, vault: Option<&str>) -> Result<Vec<OrderResponse>, HlError>` | Batch order amendments |

### Leverage & Margin

| Method | Signature | Description |
|--------|-----------|-------------|
| `update_leverage` | `(symbol: &str, leverage: u32, is_cross: bool, vault: Option<&str>) -> Result<HlActionResponse, HlError>` | Set leverage for an asset (cross or isolated) |
| `update_isolated_margin` | `(symbol: &str, amount: Decimal, vault: Option<&str>) -> Result<HlActionResponse, HlError>` | Adjust isolated margin amount |

### Unified API

All new methods accept `symbol: &str` (human-readable coin name like "BTC") and resolve to asset index internally via `meta_cache`. This eliminates the asymmetry where `place_order` requires a numeric asset index but `place_trigger_order` accepts a symbol string.

`place_order_by_symbol` is the convenience wrapper for users who want the same pattern on the existing limit order flow.

---

## New Types

Added to `crates/hl-types/src/order.rs`:

```rust
/// Cancel request by asset index + order ID.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CancelRequest {
    pub asset: u32,
    pub oid: u64,
}

/// Cancel request by asset index + client order ID.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CancelByCloidRequest {
    pub asset: u32,
    pub cloid: String,
}

/// Request to amend an existing order in-place.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModifyRequest {
    pub oid: u64,
    pub order: OrderWire,
}
```

Each type gets a `new()` constructor, matching the pattern established for all other public types.

---

## File Structure

### Executor split

```
crates/hl-executor/src/
  executor/
    mod.rs              -- struct def, constructors, send_signed_action, resolve_asset, accessors
    orders.rs           -- place_order, place_order_by_symbol, bulk_order, market_open, market_close, place_trigger_order
    cancel.rs           -- cancel_order, bulk_cancel, cancel_by_cloid, bulk_cancel_by_cloid
    modify.rs           -- modify_order, bulk_modify
    leverage.rs         -- update_leverage, update_isolated_margin
    response.rs         -- parse_order_response, parse_bulk_order_response (extracted from current executor.rs)
  meta_cache.rs         -- unchanged
  reconcile.rs          -- unchanged
  lib.rs                -- updated re-exports
```

### Internal helpers (in `executor/mod.rs`)

**`send_signed_action`** — DRY up the nonce → sign → post → status-check pipeline:

```rust
async fn send_signed_action(
    &self,
    action: serde_json::Value,
    vault: Option<&str>,
) -> Result<serde_json::Value, HlError> {
    let nonce = self.next_nonce();
    let sig = sign_l1_action(
        self.signer.as_ref(), &self.address, &action,
        nonce, self.client.is_mainnet(), vault,
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
```

**`resolve_asset`** — unified symbol → asset index lookup:

```rust
fn resolve_asset(&self, symbol: &str) -> Result<u32, HlError> {
    let coin = normalize_symbol(symbol);
    self.meta_cache.asset_index(&coin).ok_or_else(|| {
        HlError::Parse(format!("Asset '{}' not found in exchange universe", symbol))
    })
}
```

All existing methods (`place_order`, `cancel_order`, `place_trigger_order`, `transfer_to_vault`) are refactored to use these helpers. Public API signatures remain unchanged.

---

## Wire Format Reference

Action JSON formats matching the official Hyperliquid API:

### bulk_order

```json
{ "type": "order", "orders": [{...}, {...}], "grouping": "na" }
```

### bulk_cancel

```json
{ "type": "cancel", "cancels": [{"a": 0, "o": 12345}, ...] }
```

### cancel_by_cloid

```json
{ "type": "cancelByCloid", "cancels": [{"asset": 0, "cloid": "0x..."}] }
```

### batch_modify

```json
{ "type": "batchModify", "modifies": [{"oid": 12345, "order": {...}}] }
```

### update_leverage

```json
{ "type": "updateLeverage", "asset": 0, "isCross": true, "leverage": 10 }
```

### update_isolated_margin

```json
{ "type": "updateIsolatedMargin", "asset": 0, "isBuy": true, "ntli": 1000000 }
```

### Market orders

Market orders are implemented as IOC limit orders with a slippage-adjusted price:
- Buy: `limit_px = mid_price * (1 + slippage)` — default slippage 5%
- Sell: `limit_px = mid_price * (1 - slippage)` — default slippage 5%
- `market_close` first queries `clearinghouseState` to get current position size when `size=None`

---

## Testing Strategy

### Unit Tests (~20 tests)

**Action JSON construction** — verify each method produces correct wire format:
- `bulk_order` → `"order"` type with orders array
- `bulk_cancel` → `"cancel"` type with cancels array
- `cancel_by_cloid` → `"cancelByCloid"` type
- `modify_order` → `"batchModify"` type
- `update_leverage` → `"updateLeverage"` type with asset/isCross/leverage
- `update_isolated_margin` → `"updateIsolatedMargin"` type

**Response parsing**:
- `bulk_order` — parse multiple statuses (filled + resting mix)
- `modify_order` — parse amendment response

**Slippage calculation**:
- Buy: `mid=90000, slippage=5%` → `limit_px=94500`
- Sell: `mid=90000, slippage=5%` → `limit_px=85500`
- Custom slippage override

**resolve_asset**:
- Covered by existing meta_cache tests

### Live Tests (feature-gated, +4 tests)

- `live_bulk_order_and_cancel` — place 2 resting orders at extreme prices, bulk cancel
- `live_modify_order` — place resting order, modify price, verify OID unchanged
- `live_cancel_by_cloid` — place with cloid, cancel by cloid
- `live_update_leverage` — set to 5x, set back to 10x

No live tests for `market_open` / `market_close` — these would actually fill.

### Mock Transport Tests

- `market_close` with `size=None` — mock `post_info` returning position data, verify correct size used

---

## Migration Impact

- All existing public API signatures unchanged
- Internal refactoring (split files, extract helpers) is transparent to consumers
- New types added to `hl-types` — no breaking changes (additive only)
- New methods added to `OrderExecutor` — no breaking changes (additive only)
- `hl-executor` does NOT gain new crate dependencies
