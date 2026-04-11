# Spot Trading & Cross-Chain Send Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `send_asset` cross-chain action and spot order placement to close the last feature gaps with the official Hyperliquid SDK.

**Architecture:** `send_asset` follows the existing EIP-712 user-signed action pattern in `transfer.rs`. Spot trading extends `AssetMetaCache` with spot token indices and adds a new `executor/spot.rs` module that reuses the existing order submission pipeline.

**Tech Stack:** Rust, serde_json, rust_decimal, hl-signing (EIP-712), hl-client (HttpTransport)

---

### Task 1: Add `send_asset` method to transfer.rs

**Files:**
- Modify: `crates/hl-executor/src/executor/transfer.rs`

- [ ] **Step 1: Write the `send_asset` method**

Add after the `spot_send` method in `transfer.rs`:

```rust
    /// Send assets cross-chain with DEX routing.
    ///
    /// Unlike [`spot_send`](Self::spot_send) (L1-only) and
    /// [`usdc_transfer`](Self::usdc_transfer) (USDC-only), this action routes
    /// assets across chains via the Hyperliquid bridge.
    ///
    /// Uses EIP-712 user-signed-action signing.
    #[tracing::instrument(skip(self))]
    pub async fn send_asset(
        &self,
        destination: &str,
        asset: &str,
        amount: Decimal,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        validate_eth_address(destination)?;
        let chain = self.chain_name();
        let nonce = self.next_nonce();
        let action = serde_json::json!({
            "type": "sendAsset",
            "hyperliquidChain": chain,
            "signatureChainId": SIGNATURE_CHAIN_ID,
            "destination": destination,
            "asset": asset,
            "amount": amount.to_string(),
            "time": nonce,
        });

        let types = vec![
            hl_signing::EIP712Field::new("hyperliquidChain", "string"),
            hl_signing::EIP712Field::new("destination", "string"),
            hl_signing::EIP712Field::new("asset", "string"),
            hl_signing::EIP712Field::new("amount", "string"),
            hl_signing::EIP712Field::new("time", "uint64"),
        ];

        let signature = hl_signing::sign_user_signed_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            &types,
            "HyperliquidTransaction:SendAsset",
            self.client.is_mainnet(),
        )?;

        let result = self
            .client
            .post_action(action, &signature, nonce, vault)
            .await?;

        Self::check_and_parse_response(result, "sendAsset")
    }
```

- [ ] **Step 2: Add mock test for send_asset**

Add to the `#[cfg(test)] mod tests` block at the bottom of `transfer.rs`:

```rust
    #[tokio::test]
    async fn send_asset_success() {
        let executor = test_executor(vec![ok_response()]);
        let result = executor
            .send_asset(
                "0x0000000000000000000000000000000000000002",
                "USDC",
                Decimal::from(100),
                None,
            )
            .await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[tokio::test]
    async fn send_asset_rejects_invalid_address() {
        let executor = test_executor(vec![]);
        let result = executor
            .send_asset("not-an-address", "USDC", Decimal::from(100), None)
            .await;
        assert!(matches!(result, Err(HlError::InvalidAddress(_))));
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p hl-executor -- send_asset`
Expected: 2 tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/hl-executor/src/executor/transfer.rs
git commit -m "feat: add send_asset cross-chain routing action (#138)"
```

---

### Task 2: Extend AssetMetaCache with spot token mappings

**Files:**
- Modify: `crates/hl-executor/src/meta_cache.rs`

- [ ] **Step 1: Write failing tests for spot cache**

Add to the `#[cfg(test)] mod tests` block in `meta_cache.rs`:

```rust
    fn test_cache_with_spot() -> AssetMetaCache {
        AssetMetaCache::from_maps_with_spot(
            [("BTC".to_string(), 0), ("ETH".to_string(), 1)].into(),
            [("BTC".to_string(), 5), ("ETH".to_string(), 8)].into(),
            [("PURR".to_string(), 10000), ("USDC".to_string(), 10001)].into(),
            [("PURR".to_string(), 0), ("USDC".to_string(), 6)].into(),
        )
    }

    #[test]
    fn spot_asset_index_exact() {
        let cache = test_cache_with_spot();
        assert_eq!(cache.spot_asset_index("PURR"), Some(10000));
        assert_eq!(cache.spot_asset_index("USDC"), Some(10001));
    }

    #[test]
    fn spot_asset_index_case_insensitive() {
        let cache = test_cache_with_spot();
        assert_eq!(cache.spot_asset_index("purr"), Some(10000));
    }

    #[test]
    fn spot_asset_index_not_found() {
        let cache = test_cache_with_spot();
        assert_eq!(cache.spot_asset_index("DOGE"), None);
    }

    #[test]
    fn spot_sz_decimals_lookup() {
        let cache = test_cache_with_spot();
        assert_eq!(cache.spot_sz_decimals("PURR"), Some(0));
        assert_eq!(cache.spot_sz_decimals("USDC"), Some(6));
    }

    #[test]
    fn spot_does_not_overlap_perp() {
        let cache = test_cache_with_spot();
        // BTC is a perp asset, not a spot token
        assert_eq!(cache.spot_asset_index("BTC"), None);
        // PURR is a spot token, not a perp asset
        assert_eq!(cache.asset_index("PURR"), None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p hl-executor -- spot_asset`
Expected: FAIL — `from_maps_with_spot`, `spot_asset_index`, `spot_sz_decimals` not found

- [ ] **Step 3: Add spot fields and methods to AssetMetaCache**

Update the struct and add methods:

```rust
#[derive(Clone, Debug)]
pub struct AssetMetaCache {
    coin_to_index: HashMap<String, u32>,
    coin_to_sz_decimals: HashMap<String, u32>,
    spot_token_to_index: HashMap<String, u32>,
    spot_token_to_sz_decimals: HashMap<String, u32>,
}
```

Update `load()` to also fetch spot meta:

```rust
    pub async fn load(client: &dyn HttpTransport) -> Result<Self, HlError> {
        let meta = client
            .post_info(serde_json::json!({"type": "meta"}))
            .await?;
        let universe = meta["universe"]
            .as_array()
            .ok_or_else(|| HlError::Parse("meta response missing universe".into()))?;

        let mut coin_to_index = HashMap::new();
        let mut coin_to_sz_decimals = HashMap::new();
        for (i, asset) in universe.iter().enumerate() {
            if let Some(name) = asset["name"].as_str() {
                coin_to_index.insert(name.to_uppercase(), i as u32);
                if let Some(sz_dec) = asset["szDecimals"].as_u64() {
                    coin_to_sz_decimals.insert(name.to_uppercase(), sz_dec as u32);
                }
            }
        }

        // Load spot token metadata
        let mut spot_token_to_index = HashMap::new();
        let mut spot_token_to_sz_decimals = HashMap::new();
        if let Ok(spot_meta) = client
            .post_info(serde_json::json!({"type": "spotMeta"}))
            .await
        {
            if let Some(tokens) = spot_meta["tokens"].as_array() {
                for token in tokens {
                    if let (Some(name), Some(index)) =
                        (token["name"].as_str(), token["index"].as_u64())
                    {
                        spot_token_to_index.insert(name.to_uppercase(), index as u32);
                        if let Some(sz_dec) = token["szDecimals"].as_u64() {
                            spot_token_to_sz_decimals
                                .insert(name.to_uppercase(), sz_dec as u32);
                        }
                    }
                }
            }
        }

        Ok(Self {
            coin_to_index,
            coin_to_sz_decimals,
            spot_token_to_index,
            spot_token_to_sz_decimals,
        })
    }
```

Update `from_maps()` to remain backward-compatible (empty spot maps):

```rust
    pub fn from_maps(
        coin_to_index: HashMap<String, u32>,
        coin_to_sz_decimals: HashMap<String, u32>,
    ) -> Self {
        Self {
            coin_to_index,
            coin_to_sz_decimals,
            spot_token_to_index: HashMap::new(),
            spot_token_to_sz_decimals: HashMap::new(),
        }
    }

    /// Create a cache with both perp and spot maps (useful for testing).
    pub fn from_maps_with_spot(
        coin_to_index: HashMap<String, u32>,
        coin_to_sz_decimals: HashMap<String, u32>,
        spot_token_to_index: HashMap<String, u32>,
        spot_token_to_sz_decimals: HashMap<String, u32>,
    ) -> Self {
        Self {
            coin_to_index,
            coin_to_sz_decimals,
            spot_token_to_index,
            spot_token_to_sz_decimals,
        }
    }
```

Add spot lookup methods:

```rust
    /// Resolve a spot token name to its asset index.
    ///
    /// The token name is uppercased before lookup.
    pub fn spot_asset_index(&self, token: &str) -> Option<u32> {
        self.spot_token_to_index.get(&token.to_uppercase()).copied()
    }

    /// Look up the size-decimal precision for a spot token.
    pub fn spot_sz_decimals(&self, token: &str) -> Option<u32> {
        self.spot_token_to_sz_decimals
            .get(&token.to_uppercase())
            .copied()
    }

    /// Resolve a spot token name without uppercasing (pre-normalized input).
    pub(crate) fn spot_asset_index_normalized(&self, token: &str) -> Option<u32> {
        self.spot_token_to_index.get(token).copied()
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p hl-executor -- meta_cache`
Expected: All existing + new tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/hl-executor/src/meta_cache.rs
git commit -m "feat: extend AssetMetaCache with spot token mappings (#139)"
```

---

### Task 3: Add `resolve_spot_asset` and `pub mod spot` to executor

**Files:**
- Modify: `crates/hl-executor/src/executor/mod.rs`
- Create: `crates/hl-executor/src/executor/spot.rs` (empty placeholder)

- [ ] **Step 1: Add `resolve_spot_asset` to mod.rs**

Add after `resolve_asset()`:

```rust
    /// Normalize a symbol string and look up its spot token index in the meta cache.
    pub(crate) fn resolve_spot_asset(&self, symbol: &str) -> Result<u32, HlError> {
        let token = normalize_symbol(symbol);
        self.meta_cache
            .spot_asset_index_normalized(&token)
            .ok_or_else(|| {
                HlError::Validation(format!(
                    "Spot token '{}' not found in exchange universe",
                    symbol
                ))
            })
    }
```

- [ ] **Step 2: Add `pub mod spot` to the module declarations**

Add after `pub mod sub_account;`:

```rust
/// Spot token order placement.
pub mod spot;
```

- [ ] **Step 3: Create empty spot.rs**

Create `crates/hl-executor/src/executor/spot.rs`:

```rust
use rust_decimal::Decimal;

use hl_types::{HlActionResponse, HlError, OrderResponse, OrderWire, Side};

use super::OrderExecutor;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p hl-executor`
Expected: Compiles (warnings about unused imports are fine for now)

- [ ] **Step 5: Commit**

```bash
git add crates/hl-executor/src/executor/mod.rs crates/hl-executor/src/executor/spot.rs
git commit -m "feat: add resolve_spot_asset and spot module scaffold (#139)"
```

---

### Task 4: Implement spot order methods

**Files:**
- Modify: `crates/hl-executor/src/executor/spot.rs`

- [ ] **Step 1: Implement `place_spot_order` and `bulk_spot_order`**

These reuse the existing `order` action pipeline — same wire format, just with spot asset indices:

```rust
use std::str::FromStr;

use rust_decimal::Decimal;

use hl_types::{HlError, OrderResponse, OrderStatus, OrderWire, Side};

use super::orders::{determine_status, order_to_json};
use super::response::{parse_bulk_order_response_with_fallbacks, parse_order_response};
use super::OrderExecutor;

impl OrderExecutor {
    /// Place a spot limit order.
    ///
    /// The `OrderWire` must have its `asset` field set to a **spot token index**
    /// (from [`AssetMetaCache::spot_asset_index`]). Use
    /// [`spot_market_open`](Self::spot_market_open) for market orders.
    #[tracing::instrument(skip(self, order), fields(asset = order.asset, is_buy = order.is_buy))]
    pub async fn place_spot_order(
        &self,
        order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let fallback_price = Decimal::from_str(&order.limit_px).unwrap_or(Decimal::ZERO);
        let fallback_size = Decimal::from_str(&order.sz).unwrap_or(Decimal::ZERO);

        let order_json = order_to_json(&order)?;
        let action = serde_json::json!({
            "type": "order",
            "orders": [order_json],
            "grouping": "na"
        });

        let result = self.send_signed_action(action, vault).await?;
        let (order_id, fill_price, fill_size) =
            parse_order_response(&result, fallback_price, fallback_size)?;
        let status = determine_status(fill_size, fallback_size, &order_id);

        Ok(OrderResponse::new(
            order_id,
            if fill_size > Decimal::ZERO {
                Some(fill_price)
            } else {
                None
            },
            fill_size,
            fallback_size,
            status,
        ))
    }

    /// Place multiple spot orders atomically.
    #[tracing::instrument(skip(self, orders), fields(count = orders.len()))]
    pub async fn bulk_spot_order(
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
            order_jsons.push(order_to_json(order)?);
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
        let parsed = parse_bulk_order_response_with_fallbacks(&result, &fallbacks)?;

        let mut responses = Vec::with_capacity(parsed.len());
        for (i, (order_id, fill_price, fill_size)) in parsed.into_iter().enumerate() {
            let (_, fallback_size) = fallbacks
                .get(i)
                .copied()
                .unwrap_or((Decimal::ZERO, Decimal::ZERO));
            let status = determine_status(fill_size, fallback_size, &order_id);
            responses.push(OrderResponse::new(
                order_id,
                if fill_size > Decimal::ZERO {
                    Some(fill_price)
                } else {
                    None
                },
                fill_size,
                fallback_size,
                status,
            ));
        }

        Ok(responses)
    }

    /// Market buy or sell a spot token with slippage tolerance.
    ///
    /// Fetches the current mid-price from the L2 orderbook and places an IOC
    /// limit order adjusted by `slippage`. Default slippage is 5%.
    #[tracing::instrument(skip(self))]
    pub async fn spot_market_open(
        &self,
        symbol: &str,
        is_buy: bool,
        size: Decimal,
        slippage: Option<Decimal>,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let asset_idx = self.resolve_spot_asset(symbol)?;
        let coin = super::normalize_symbol(symbol);

        let resp = self
            .client
            .post_info(serde_json::json!({"type": "l2Book", "coin": coin}))
            .await?;
        let mid = hl_types::parse_mid_price_from_l2book(&resp)?;

        let slippage = slippage.unwrap_or(Decimal::new(5, 2)); // 5%
        let limit_price = if is_buy {
            mid * (Decimal::ONE + slippage)
        } else {
            mid * (Decimal::ONE - slippage)
        };

        let order = if is_buy {
            OrderWire::limit_buy(asset_idx, limit_price, size)
        } else {
            OrderWire::limit_sell(asset_idx, limit_price, size)
        };
        let order = order.tif(hl_types::Tif::Ioc).build()?;

        self.place_spot_order(order, vault).await
    }

    /// Cancel a spot order by exchange order ID.
    #[tracing::instrument(skip(self))]
    pub async fn cancel_spot_order(
        &self,
        asset_idx: u32,
        oid: u64,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({
            "type": "cancel",
            "cancels": [{"a": asset_idx, "o": oid}]
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("cancel_spot_order response: {e}")))
    }
}
```

- [ ] **Step 2: Check that `order_to_json`, `determine_status`, `parse_order_response`, `parse_bulk_order_response_with_fallbacks` are accessible**

These are currently private in `orders.rs` and `response.rs`. Make them `pub(crate)`:

In `crates/hl-executor/src/executor/orders.rs`, change:
```rust
fn order_to_json(order: &OrderWire) -> Result<serde_json::Value, HlError> {
```
to:
```rust
pub(crate) fn order_to_json(order: &OrderWire) -> Result<serde_json::Value, HlError> {
```

and:
```rust
fn determine_status(fill_size: Decimal, requested_size: Decimal, order_id: &str) -> OrderStatus {
```
to:
```rust
pub(crate) fn determine_status(fill_size: Decimal, requested_size: Decimal, order_id: &str) -> OrderStatus {
```

In `crates/hl-executor/src/executor/response.rs`, verify `parse_order_response` and `parse_bulk_order_response_with_fallbacks` are `pub(crate)` (they likely already are).

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p hl-executor`
Expected: Compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/hl-executor/src/executor/spot.rs crates/hl-executor/src/executor/orders.rs crates/hl-executor/src/executor/response.rs
git commit -m "feat: implement spot order placement methods (#139)"
```

---

### Task 5: Add mock tests for spot order methods

**Files:**
- Modify: `crates/hl-executor/src/executor/spot.rs`

- [ ] **Step 1: Add test module with mock tests**

Append to `spot.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use hl_test_utils::test_executor;
    use std::str::FromStr;

    fn ok_resting_response(oid: u64) -> serde_json::Value {
        serde_json::json!({
            "status": "ok",
            "response": {
                "type": "order",
                "data": {
                    "statuses": [{"resting": {"oid": oid}}]
                }
            }
        })
    }

    fn test_executor_with_spot(responses: Vec<serde_json::Value>) -> OrderExecutor {
        use crate::meta_cache::AssetMetaCache;
        use hl_client::HttpTransport;
        use std::sync::Arc;

        let mut perp_idx = std::collections::HashMap::new();
        perp_idx.insert("BTC".to_string(), 0u32);
        let mut spot_idx = std::collections::HashMap::new();
        spot_idx.insert("PURR".to_string(), 10000u32);
        spot_idx.insert("USDC".to_string(), 10001u32);
        let cache = AssetMetaCache::from_maps_with_spot(
            perp_idx,
            Default::default(),
            spot_idx,
            Default::default(),
        );
        OrderExecutor::with_meta_cache(
            Arc::new(hl_test_utils::MockTransport::new(responses)),
            hl_test_utils::test_signer(),
            "0x0000000000000000000000000000000000000001".to_string(),
            cache,
        )
    }

    #[tokio::test]
    async fn place_spot_order_success() {
        let executor = test_executor_with_spot(vec![ok_resting_response(9999)]);
        let order = OrderWire::limit_buy(10000, Decimal::from(1), Decimal::from(100))
            .build()
            .unwrap();
        let result = executor.place_spot_order(order, None).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.order_id, "9999");
    }

    #[tokio::test]
    async fn bulk_spot_order_empty() {
        let executor = test_executor_with_spot(vec![]);
        let result = executor.bulk_spot_order(vec![], None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn resolve_spot_asset_unknown_fails() {
        let executor = test_executor_with_spot(vec![]);
        let result = executor.resolve_spot_asset("UNKNOWN_TOKEN");
        assert!(matches!(result, Err(HlError::Validation(_))));
    }

    #[tokio::test]
    async fn cancel_spot_order_success() {
        let executor = test_executor_with_spot(vec![
            serde_json::json!({"status": "ok", "response": {"type": "cancel"}}),
        ]);
        let result = executor.cancel_spot_order(10000, 9999, None).await;
        assert!(result.is_ok());
    }
}
```

- [ ] **Step 2: Run all tests**

Run: `cargo test -p hl-executor -- spot`
Expected: All spot tests PASS

- [ ] **Step 3: Run full test suite**

Run: `cargo fmt --all && cargo clippy --all-features --all-targets -- -D warnings && cargo test --all-features -- --skip live_`
Expected: All tests PASS, clippy clean

- [ ] **Step 4: Commit**

```bash
git add crates/hl-executor/src/executor/spot.rs
git commit -m "test: add mock tests for spot order methods (#139)"
```

---

### Task 6: Final verification and push

- [ ] **Step 1: Run full test suite**

Run: `cargo test --all-features -- --skip live_`
Expected: All tests PASS (452+ tests)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --all-features --all-targets -- -D warnings`
Expected: Zero warnings

- [ ] **Step 3: Verify no regressions**

Run:
```bash
grep -rn '"0xa4b1"' crates/hl-executor/src/ --include="*.rs" | grep -v test | grep -v SIGNATURE_CHAIN_ID
grep -c 'validate_eth_address' crates/hl-executor/src/executor/mod.rs
grep -c 'Validation' crates/hl-types/src/error.rs
grep -c 'side: String' crates/hl-client/src/ws/types.rs crates/hl-types/src/account.rs
```
Expected: No hardcoded chain IDs, validate_eth_address present, Validation variant present, zero side: String

- [ ] **Step 4: Push**

```bash
git push origin main
```
