# Performance & DX Polish — Design Spec

> Phase 3: Small targeted improvements to developer experience, API ergonomics, and performance.

## Scope

5 independent improvements in a single spec:

1. OrderWireBuilder accepts Decimal + validates on build
2. Prelude expansion + Side::from_is_buy
3. New examples (WebSocket streaming, trigger orders)
4. Meta cache internal optimization (eliminate redundant to_uppercase)
5. OrderWire String fields preserved for wire compat

---

## 1. OrderWireBuilder Accepts Decimal + Validates

### Current Problem

Builder methods accept `&str` for price and size:
```rust
OrderWire::limit_buy(0, "90000.0", "0.001")  // stringly typed
```

Then `place_order` parses them back with `Decimal::from_str`. If the user passes `"banana"`, it silently becomes `Decimal::ZERO` at submission time.

### Change

Builder methods accept `Decimal`:
```rust
OrderWire::limit_buy(0, dec!(90000), dec!(0.001))
```

Internally, the builder stores `Decimal` and converts to `String` in `build()`. The `OrderWire` struct fields (`limit_px: String`, `sz: String`) stay unchanged — they are the wire format.

`build()` changes return type from `OrderWire` to `Result<OrderWire, HlError>` and validates:
- price > 0
- size > 0

### Migration Impact

Breaking change on builder methods (accept `Decimal` instead of `&str`). Since `build()` now returns `Result`, all call sites need `.build()?` or `.build().unwrap()`. Pre-1.0, acceptable.

---

## 2. Prelude Expansion + Side::from_is_buy

### Missing from prelude

Add to `crates/motosan-hyperliquid/src/prelude.rs`:
- `Tif`, `Tpsl`, `PositionSide`, `OrderStatus`
- `HlActionResponse`
- `CancelRequest`, `CancelByCloidRequest`, `ModifyRequest`
- `normalize_coin`

### New method

```rust
impl Side {
    pub fn from_is_buy(is_buy: bool) -> Self {
        if is_buy { Side::Buy } else { Side::Sell }
    }
}
```

---

## 3. New Examples

### `examples/ws_stream.rs`

Demonstrates typed WebSocket usage:
- Connect to testnet
- Subscribe to L2Book("BTC") + UserFills(address)
- Loop `next_typed_message`, match on `WsMessage` variants
- Print updates for 10 seconds then disconnect

Requires `ws` feature. No private key needed (read-only subscriptions).

### `examples/trigger_order.rs`

Demonstrates stop-loss / take-profit:
- Connect to testnet with signing
- Place a trigger order (stop-loss at a far price)
- Print the response
- Cancel it

Requires `HYPERLIQUID_TESTNET_KEY`.

---

## 4. Meta Cache Optimization

### Current Problem

Every `asset_index("BTC")` and `sz_decimals("BTC")` call does `.to_uppercase()` — heap allocation on every lookup, even when the input is already uppercase (which it always is when coming from `resolve_asset` → `normalize_symbol`).

### Change

Add `pub(crate) fn asset_index_normalized(&self, coin: &str) -> Option<u32>` that skips uppercase conversion. `resolve_asset` in `executor/mod.rs` calls this instead of `asset_index`.

Public `asset_index(&self, coin: &str)` stays unchanged (still does uppercase, remains case-insensitive for external callers).

Same for `sz_decimals_normalized`.

---

## 5. Files Changed

| File | Changes |
|------|---------|
| `crates/hl-types/src/order.rs` | Builder methods accept Decimal, build() returns Result |
| `crates/hl-types/src/order.rs` | Side::from_is_buy |
| `crates/hl-executor/src/executor/orders.rs` | Update build() call sites |
| `crates/hl-executor/src/executor/modify.rs` | Update build() call sites (if any) |
| `crates/hl-executor/src/executor/mod.rs` | Use asset_index_normalized |
| `crates/hl-executor/src/meta_cache.rs` | Add _normalized variants |
| `crates/hl-executor/tests/live_test.rs` | Update build() calls |
| `crates/motosan-hyperliquid/src/prelude.rs` | Expand re-exports |
| `examples/ws_stream.rs` | New |
| `examples/trigger_order.rs` | New |

---

## Testing

- Existing unit tests updated for `build()` → `build()?`
- New tests: `build_rejects_zero_price`, `build_rejects_zero_size`, `build_rejects_negative_price`
- `Side::from_is_buy` test
- Examples verified with `cargo build --examples`
