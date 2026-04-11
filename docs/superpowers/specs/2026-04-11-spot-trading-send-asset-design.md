# Spot Trading & Cross-Chain Send Design

## Overview

Two features to close the last exchange action gaps with the official Hyperliquid SDK:

1. **`send_asset`** — Cross-chain asset routing (EIP-712 user-signed action)
2. **Spot order placement** — Extend executor to support spot token trading

## Feature 1: send_asset

### Wire Format

```json
{
  "type": "sendAsset",
  "hyperliquidChain": "Mainnet",
  "signatureChainId": "0xa4b1",
  "destination": "0x...",
  "asset": "USDC",
  "amount": "100.0",
  "time": 1700000000000
}
```

### EIP-712 Type Array

```rust
vec![
    EIP712Field::new("hyperliquidChain", "string"),
    EIP712Field::new("destination", "string"),
    EIP712Field::new("asset", "string"),
    EIP712Field::new("amount", "string"),
    EIP712Field::new("time", "uint64"),
]
```

Primary type: `"HyperliquidTransaction:SendAsset"`

### Method Signature

```rust
impl OrderExecutor {
    pub async fn send_asset(
        &self,
        destination: &str,
        asset: &str,
        amount: Decimal,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError>
}
```

### Validation

- `validate_eth_address(destination)` before signing
- Uses `SIGNATURE_CHAIN_ID` constant
- Uses `check_and_parse_response` for response handling

### Location

Add to `crates/hl-executor/src/executor/transfer.rs` alongside `spot_send`.

## Feature 2: Spot Order Placement

### Key Insight

Spot and perp orders use the **same** `order` action wire format. The only difference is the asset index — spot tokens have separate indices loaded from `spotMeta`.

### MetaCache Extension

Extend `AssetMetaCache` in `crates/hl-executor/src/meta_cache.rs`:

```rust
impl AssetMetaCache {
    // Existing perp methods unchanged
    pub fn asset_index(&self, coin: &str) -> Option<u32>;
    pub fn sz_decimals(&self, coin: &str) -> Option<u32>;

    // New spot methods
    pub fn spot_asset_index(&self, token: &str) -> Option<u32>;
    pub fn spot_sz_decimals(&self, token: &str) -> Option<u32>;
}
```

The `load()` method fetches both `{"type": "meta"}` and `{"type": "spotMeta"}` during initialization. Two new internal maps: `spot_token_to_index` and `spot_token_to_sz_decimals`.

`from_maps()` gains two additional parameters for spot maps (backward-compatible via a new `from_maps_with_spot()` constructor).

### Spot Order Methods

New file `crates/hl-executor/src/executor/spot.rs`:

```rust
impl OrderExecutor {
    /// Place a spot limit order.
    pub async fn place_spot_order(
        &self,
        order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError>;

    /// Place multiple spot orders atomically.
    pub async fn bulk_spot_order(
        &self,
        orders: Vec<OrderWire>,
        vault: Option<&str>,
    ) -> Result<Vec<OrderResponse>, HlError>;

    /// Market buy/sell a spot token with slippage tolerance.
    pub async fn spot_market_open(
        &self,
        symbol: &str,
        is_buy: bool,
        size: Decimal,
        slippage: Decimal,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError>;

    /// Cancel a spot order by exchange order ID.
    pub async fn cancel_spot_order(
        &self,
        asset_idx: u32,
        oid: u64,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError>;
}
```

### Asset Resolution

Add `resolve_spot_asset()` to `crates/hl-executor/src/executor/mod.rs`:

```rust
pub(crate) fn resolve_spot_asset(&self, symbol: &str) -> Result<u32, HlError> {
    let token = normalize_symbol(symbol);
    self.meta_cache
        .spot_asset_index_normalized(&token)
        .ok_or_else(|| {
            HlError::Validation(format!("Spot token '{}' not found", symbol))
        })
}
```

### What We Don't Build

- No spot-specific `OrderWire` builder variants — reuse existing `limit_buy`/`limit_sell` with spot asset index
- No spot trigger orders or TWAP — these are perp-only features
- No spot-specific position tracking — spot balances are already queryable via `Account::spot_state()`

## File Changes

| File | Change |
|------|--------|
| `executor/transfer.rs` | Add `send_asset()` method |
| `meta_cache.rs` | Add spot token maps + `spot_asset_index()` / `spot_sz_decimals()` + load from spotMeta |
| `executor/spot.rs` | **New file** — spot order placement methods |
| `executor/mod.rs` | Add `pub mod spot`, `resolve_spot_asset()` |

## Testing

- Mock tests for `send_asset` (success, address validation)
- Mock tests for `place_spot_order`, `bulk_spot_order`, `spot_market_open`
- Unit tests for `spot_asset_index` / `spot_sz_decimals` in meta_cache
- Verify spot vs perp index separation (same token name can have different indices)
