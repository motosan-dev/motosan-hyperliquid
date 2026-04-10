# Additional Actions & Info Queries Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add remaining exchange actions (USDC transfer, class transfer, approve agent, schedule cancel, claim rewards) and info queries (open orders, order status, funding history, user funding, historical orders).

**Architecture:** Two independent work streams: (1) exchange actions in new `executor/transfer.rs` and `executor/admin.rs` files, all using the existing `send_signed_action` helper; (2) info queries as new methods on the existing `Account` struct. Move `transfer_to_vault` from `orders.rs` to `transfer.rs`.

**Tech Stack:** Rust, serde_json, rust_decimal, tracing

**Spec:** `docs/superpowers/specs/2026-04-10-additional-actions-design.md`

---

## File Structure

| File | Responsibility | Task |
|------|---------------|------|
| `crates/hl-executor/src/executor/transfer.rs` | Create: usdc_transfer, class_transfer, transfer_to_vault (moved) | 1 |
| `crates/hl-executor/src/executor/admin.rs` | Create: approve_agent, schedule_cancel, claim_rewards | 2 |
| `crates/hl-executor/src/executor/orders.rs` | Modify: remove transfer_to_vault | 1 |
| `crates/hl-executor/src/executor/mod.rs` | Modify: add `pub mod transfer; pub mod admin;` | 1, 2 |
| `crates/hl-account/src/account.rs` | Modify: add 5 info query methods | 3 |
| `crates/hl-executor/tests/live_test.rs` | Modify: add live tests | 4 |
| `crates/hl-account/tests/live_test.rs` | Modify: add live tests | 4 |

---

### Task 1: Transfer Methods (usdc_transfer, class_transfer, move transfer_to_vault)

**Files:**
- Create: `crates/hl-executor/src/executor/transfer.rs`
- Modify: `crates/hl-executor/src/executor/orders.rs` (remove transfer_to_vault)
- Modify: `crates/hl-executor/src/executor/mod.rs` (add `pub mod transfer;`)

- [ ] **Step 1: Create transfer.rs**

Create `crates/hl-executor/src/executor/transfer.rs`:

```rust
use rust_decimal::Decimal;
use hl_types::*;

use super::OrderExecutor;

impl OrderExecutor {
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
        let result = self.send_signed_action(action, None).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("transfer_to_vault response: {e}")))
    }

    /// Send USDC to another address on Hyperliquid.
    #[tracing::instrument(skip(self))]
    pub async fn usdc_transfer(
        &self,
        destination: &str,
        amount: Decimal,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({
            "type": "usdSend",
            "hyperliquidChain": if self.client.is_mainnet() { "Mainnet" } else { "Testnet" },
            "signatureChainId": "0xa4b1",
            "destination": destination,
            "amount": amount.to_string(),
            "time": self.next_nonce(),
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("usdc_transfer response: {e}")))
    }

    /// Transfer USDC between spot and perp accounts.
    ///
    /// `to_perp`: `true` moves from spot to perp, `false` moves from perp to spot.
    /// Amount is in USDC.
    #[tracing::instrument(skip(self))]
    pub async fn class_transfer(
        &self,
        usdc_amount: Decimal,
        to_perp: bool,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        // Convert to micro-units (multiply by 1,000,000)
        let usdc_micro = (usdc_amount * Decimal::from(1_000_000))
            .to_string()
            .parse::<u64>()
            .map_err(|e| HlError::Parse(format!("class_transfer amount conversion: {e}")))?;

        let action = serde_json::json!({
            "type": "spotUser",
            "classTransfer": {
                "usdc": usdc_micro,
                "toPerp": to_perp,
            }
        });
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("class_transfer response: {e}")))
    }
}
```

- [ ] **Step 2: Remove transfer_to_vault from orders.rs**

In `crates/hl-executor/src/executor/orders.rs`, find and remove the entire `transfer_to_vault` method (and its `#[tracing::instrument]` attribute). The method is now in `transfer.rs`.

- [ ] **Step 3: Add module declaration**

In `crates/hl-executor/src/executor/mod.rs`, add `pub mod transfer;` alongside the other module declarations.

- [ ] **Step 4: Verify**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: All pass, clean. `transfer_to_vault` still works (just moved, not deleted from public API).

- [ ] **Step 5: Commit**

```bash
git add crates/hl-executor/src/executor/
git commit -m "feat: add usdc_transfer and class_transfer, move transfer_to_vault to transfer.rs"
```

---

### Task 2: Admin Methods (approve_agent, schedule_cancel, claim_rewards)

**Files:**
- Create: `crates/hl-executor/src/executor/admin.rs`
- Modify: `crates/hl-executor/src/executor/mod.rs` (add `pub mod admin;`)

- [ ] **Step 1: Create admin.rs**

Create `crates/hl-executor/src/executor/admin.rs`:

```rust
use hl_types::*;

use super::OrderExecutor;

impl OrderExecutor {
    /// Approve a trading agent for this account.
    ///
    /// The agent can then trade on behalf of this account.
    /// `agent_name` is optional metadata.
    #[tracing::instrument(skip(self))]
    pub async fn approve_agent(
        &self,
        agent_address: &str,
        agent_name: Option<&str>,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let chain = if self.client.is_mainnet() { "Mainnet" } else { "Testnet" };
        let mut action = serde_json::json!({
            "type": "approveAgent",
            "hyperliquidChain": chain,
            "signatureChainId": "0xa4b1",
            "agentAddress": agent_address,
            "nonce": self.next_nonce(),
        });
        if let Some(name) = agent_name {
            action.as_object_mut().unwrap().insert(
                "agentName".to_string(),
                serde_json::Value::String(name.to_string()),
            );
        }
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("approve_agent response: {e}")))
    }

    /// Schedule cancellation of all open orders at a future time.
    ///
    /// `time`: Unix timestamp in milliseconds when orders should be cancelled.
    /// Pass `None` to clear a previously scheduled cancellation.
    #[tracing::instrument(skip(self))]
    pub async fn schedule_cancel(
        &self,
        time: Option<u64>,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let action = if let Some(t) = time {
            serde_json::json!({"type": "scheduleCancel", "time": t})
        } else {
            serde_json::json!({"type": "scheduleCancel", "time": null})
        };
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("schedule_cancel response: {e}")))
    }

    /// Claim earned trading rewards.
    #[tracing::instrument(skip(self))]
    pub async fn claim_rewards(
        &self,
        vault: Option<&str>,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({"type": "claimRewards"});
        let result = self.send_signed_action(action, vault).await?;
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("claim_rewards response: {e}")))
    }
}
```

- [ ] **Step 2: Add module declaration**

In `crates/hl-executor/src/executor/mod.rs`, add `pub mod admin;` alongside the other module declarations.

- [ ] **Step 3: Verify**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: All pass, clean.

- [ ] **Step 4: Commit**

```bash
git add crates/hl-executor/src/executor/admin.rs crates/hl-executor/src/executor/mod.rs
git commit -m "feat: add approve_agent, schedule_cancel, claim_rewards"
```

---

### Task 3: Info Queries (open_orders, order_status, funding_history, user_funding, historical_orders)

**Files:**
- Modify: `crates/hl-account/src/account.rs`

- [ ] **Step 1: Add 5 info query methods to Account**

Read `crates/hl-account/src/account.rs` first. Add these methods to the existing `impl Account` block, after the existing methods:

```rust
    /// Fetch all open orders for an address.
    #[tracing::instrument(skip(self))]
    pub async fn open_orders(
        &self,
        address: &str,
    ) -> Result<Vec<serde_json::Value>, HlError> {
        let payload = serde_json::json!({"type": "openOrders", "user": address});
        let resp = self.client.post_info(payload).await?;
        resp.as_array()
            .cloned()
            .ok_or_else(|| HlError::Parse("expected array for openOrders".into()))
    }

    /// Fetch the status of a specific order by order ID.
    #[tracing::instrument(skip(self))]
    pub async fn order_status(
        &self,
        address: &str,
        oid: u64,
    ) -> Result<serde_json::Value, HlError> {
        let payload = serde_json::json!({"type": "orderStatus", "user": address, "oid": oid});
        self.client.post_info(payload).await
    }

    /// Fetch historical funding rates for a coin.
    ///
    /// `start_time`: Unix timestamp in milliseconds.
    /// `end_time`: Optional upper bound. If `None`, returns up to the current time.
    #[tracing::instrument(skip(self))]
    pub async fn funding_history(
        &self,
        coin: &str,
        start_time: u64,
        end_time: Option<u64>,
    ) -> Result<Vec<serde_json::Value>, HlError> {
        let mut payload = serde_json::json!({
            "type": "fundingHistory",
            "coin": coin,
            "startTime": start_time,
        });
        if let Some(et) = end_time {
            payload.as_object_mut().unwrap().insert(
                "endTime".to_string(),
                serde_json::Value::Number(et.into()),
            );
        }
        let resp = self.client.post_info(payload).await?;
        resp.as_array()
            .cloned()
            .ok_or_else(|| HlError::Parse("expected array for fundingHistory".into()))
    }

    /// Fetch a user's funding payment history.
    ///
    /// `start_time`: Unix timestamp in milliseconds.
    /// `end_time`: Optional upper bound.
    #[tracing::instrument(skip(self))]
    pub async fn user_funding(
        &self,
        address: &str,
        start_time: u64,
        end_time: Option<u64>,
    ) -> Result<Vec<serde_json::Value>, HlError> {
        let mut payload = serde_json::json!({
            "type": "userFunding",
            "user": address,
            "startTime": start_time,
        });
        if let Some(et) = end_time {
            payload.as_object_mut().unwrap().insert(
                "endTime".to_string(),
                serde_json::Value::Number(et.into()),
            );
        }
        let resp = self.client.post_info(payload).await?;
        resp.as_array()
            .cloned()
            .ok_or_else(|| HlError::Parse("expected array for userFunding".into()))
    }

    /// Fetch all historical orders for an address.
    #[tracing::instrument(skip(self))]
    pub async fn historical_orders(
        &self,
        address: &str,
    ) -> Result<Vec<serde_json::Value>, HlError> {
        let payload = serde_json::json!({"type": "historicalOrders", "user": address});
        let resp = self.client.post_info(payload).await?;
        resp.as_array()
            .cloned()
            .ok_or_else(|| HlError::Parse("expected array for historicalOrders".into()))
    }
```

- [ ] **Step 2: Verify**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: All pass, clean.

- [ ] **Step 3: Commit**

```bash
git add crates/hl-account/src/account.rs
git commit -m "feat: add open_orders, order_status, funding_history, user_funding, historical_orders"
```

---

### Task 4: Live Integration Tests

**Files:**
- Modify: `crates/hl-account/tests/live_test.rs`
- Modify: `crates/hl-executor/tests/live_test.rs`

- [ ] **Step 1: Add info query live tests**

Read `crates/hl-account/tests/live_test.rs` first. Add these tests using the existing `setup()` or `account()` helper pattern:

```rust
#[tokio::test]
async fn live_open_orders() {
    let account = account();
    let address = std::env::var("HYPERLIQUID_TESTNET_ADDRESS")
        .or_else(|_| std::env::var("HYPERLIQUID_TESTNET_KEY").map(|k| {
            let signer = hl_signing::PrivateKeySigner::from_hex(&k).unwrap();
            signer.address().to_string()
        }))
        .expect("HYPERLIQUID_TESTNET_KEY or HYPERLIQUID_TESTNET_ADDRESS must be set");

    let orders = account.open_orders(&address).await;
    assert!(orders.is_ok(), "open_orders failed: {:?}", orders.err());
    // Result is an array (possibly empty if no open orders)
    let orders = orders.unwrap();
    assert!(orders.len() >= 0); // Just verify it's a valid array
}

#[tokio::test]
async fn live_historical_orders() {
    let account = account();
    let address = std::env::var("HYPERLIQUID_TESTNET_ADDRESS")
        .or_else(|_| std::env::var("HYPERLIQUID_TESTNET_KEY").map(|k| {
            let signer = hl_signing::PrivateKeySigner::from_hex(&k).unwrap();
            signer.address().to_string()
        }))
        .expect("HYPERLIQUID_TESTNET_KEY or HYPERLIQUID_TESTNET_ADDRESS must be set");

    let orders = account.historical_orders(&address).await;
    assert!(orders.is_ok(), "historical_orders failed: {:?}", orders.err());
}
```

- [ ] **Step 2: Add executor live test for schedule_cancel**

Read `crates/hl-executor/tests/live_test.rs`. Add:

```rust
/// Schedule cancel then immediately unschedule.
#[tokio::test]
async fn live_schedule_cancel() {
    let (client, signer, address) = setup();
    let executor = OrderExecutor::from_client(client, signer, address)
        .await
        .expect("executor construction failed");

    // Schedule cancel 1 hour from now
    let future_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 3_600_000;

    let resp = executor.schedule_cancel(Some(future_time), None).await;
    assert!(resp.is_ok(), "schedule_cancel failed: {:?}", resp.err());

    // Unschedule
    let resp2 = executor.schedule_cancel(None, None).await;
    assert!(resp2.is_ok(), "unschedule_cancel failed: {:?}", resp2.err());
}
```

- [ ] **Step 3: Verify unit tests pass**

Run: `cargo test -p hl-executor -v && cargo test -p hl-account -v`
Expected: Unit tests pass. Live tests are gated behind `live-test` feature.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: Clean

- [ ] **Step 5: Commit**

```bash
git add crates/hl-executor/tests/live_test.rs crates/hl-account/tests/live_test.rs
git commit -m "test: add live tests for open_orders, historical_orders, schedule_cancel"
```
