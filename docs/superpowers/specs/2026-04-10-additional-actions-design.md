# Additional Actions & Info Queries — Design Spec

> Sub-project 3 of Phase 2 (High-Value Features). Add remaining exchange actions and info queries to close the gap with the official Hyperliquid SDK.

## Context

After Sub-projects 1 and 2, the SDK covers order lifecycle (place/cancel/modify/trigger), leverage, market helpers, and typed WebSocket events. The remaining gaps are account management actions (USDC transfer, agent approval, scheduled cancellation) and read-only queries (open orders, order status, funding history).

## Scope

5 new exchange actions on `OrderExecutor` + 5 new info queries on `Account`.

---

## Exchange Actions (hl-executor)

### New Files

**`executor/transfer.rs`** — Asset transfer methods:

| Method | Signature | Wire Type | Description |
|--------|-----------|-----------|-------------|
| `usdc_transfer` | `(destination: &str, amount: Decimal, vault: Option<&str>) -> Result<HlActionResponse>` | `usdSend` | Send USDC to another address |
| `class_transfer` | `(usdc_amount: Decimal, to_perp: bool, vault: Option<&str>) -> Result<HlActionResponse>` | `spotUser` | Transfer USDC between spot and perp accounts |

Also move existing `transfer_to_vault` from `orders.rs` to `transfer.rs` for better organization.

**`executor/admin.rs`** — Account management methods:

| Method | Signature | Wire Type | Description |
|--------|-----------|-----------|-------------|
| `approve_agent` | `(agent_address: &str, agent_name: Option<&str>, vault: Option<&str>) -> Result<HlActionResponse>` | `approveAgent` | Approve a trading agent for this account |
| `schedule_cancel` | `(time: Option<u64>, vault: Option<&str>) -> Result<HlActionResponse>` | `scheduleCancel` | Schedule cancellation of all orders at a future time. `None` = cancel the schedule. |
| `claim_rewards` | `(vault: Option<&str>) -> Result<HlActionResponse>` | `claimRewards` | Claim earned rewards |

### Wire Formats

```json
// usdc_transfer
{"type": "usdSend", "hyperliquidChain": "Mainnet", "signatureChainId": "0xa4b1", "destination": "0x...", "amount": "100.0", "time": <nonce>}

// class_transfer
{"type": "spotUser", "classTransfer": {"usdc": 1000000, "toPerp": true}}

// approve_agent (uses sign_user_signed_action, NOT sign_l1_action)
{"type": "approveAgent", "signatureChainId": "0xa4b1", "hyperliquidChain": "Mainnet", "agentAddress": "0x...", "agentName": "MyBot", "nonce": <timestamp>}

// schedule_cancel
{"type": "scheduleCancel", "time": 1700000000000}

// claim_rewards
{"type": "claimRewards"}
```

Note: `approve_agent` requires EIP-712 typed-data signing (not the L1 action signing used by most methods). It uses `sign_user_signed_action` from `hl-signing`. The `usdc_transfer` also uses different signing — it is a user-signed action with typed data fields. Both will need special handling in their implementations.

For simplicity in this first pass: `schedule_cancel` and `claim_rewards` use standard `send_signed_action`. `usdc_transfer` and `approve_agent` need manual nonce + signing + post, similar to how `transfer_to_vault` originally worked but with `sign_user_signed_action`.

`class_transfer` uses standard `send_signed_action` — the `spotUser` action type goes through the normal L1 action flow. The `usdc` amount is in micro-units (multiply by 1,000,000).

---

## Info Queries (hl-account)

### New Methods on `Account`

| Method | Signature | Info Type | Description |
|--------|-----------|-----------|-------------|
| `open_orders` | `(address: &str) -> Result<Vec<serde_json::Value>>` | `openOrders` | All open orders for an address |
| `order_status` | `(address: &str, oid: u64) -> Result<serde_json::Value>` | `orderStatus` | Status of a specific order |
| `funding_history` | `(coin: &str, start_time: u64, end_time: Option<u64>) -> Result<Vec<serde_json::Value>>` | `fundingHistory` | Historical funding rates for a coin |
| `user_funding` | `(address: &str, start_time: u64, end_time: Option<u64>) -> Result<Vec<serde_json::Value>>` | `userFunding` | User's funding payment history |
| `historical_orders` | `(address: &str) -> Result<Vec<serde_json::Value>>` | `historicalOrders` | All historical orders for an address |

All return `serde_json::Value` or `Vec<serde_json::Value>` — these response formats are complex and evolving, full typing is not worth the maintenance cost.

### Wire Formats

```json
// open_orders
{"type": "openOrders", "user": "0x..."}

// order_status
{"type": "orderStatus", "user": "0x...", "oid": 12345}

// funding_history
{"type": "fundingHistory", "coin": "BTC", "startTime": 1700000000000}
// optionally: "endTime": 1700100000000

// user_funding
{"type": "userFunding", "user": "0x...", "startTime": 1700000000000}

// historical_orders
{"type": "historicalOrders", "user": "0x..."}
```

---

## Testing Strategy

### Unit Tests
- Action JSON construction for each new exchange action
- Info query JSON construction for each new query

### Live Tests (feature-gated)
- `live_open_orders` — query open orders (should return array, possibly empty)
- `live_order_status` — place a resting order, query its status, then cancel
- `live_schedule_cancel` — schedule cancel then unschedule (time=None)

No live tests for `approve_agent` (creates real agent), `claim_rewards` (has side effects), `usdc_transfer` (moves real funds), `class_transfer` (moves funds between accounts).

---

## File Structure

| File | Responsibility | Changes |
|------|---------------|---------|
| `crates/hl-executor/src/executor/transfer.rs` | Create: usdc_transfer, class_transfer, transfer_to_vault (moved) | New |
| `crates/hl-executor/src/executor/admin.rs` | Create: approve_agent, schedule_cancel, claim_rewards | New |
| `crates/hl-executor/src/executor/orders.rs` | Modify: remove transfer_to_vault (moved to transfer.rs) | Modify |
| `crates/hl-executor/src/executor/mod.rs` | Modify: add `pub mod transfer; pub mod admin;` | Modify |
| `crates/hl-account/src/account.rs` | Modify: add 5 info query methods | Modify |
| `crates/hl-executor/tests/live_test.rs` | Modify: add live tests | Modify |
| `crates/hl-account/tests/live_test.rs` | Modify: add live tests | Modify |

---

## Migration Impact

- `transfer_to_vault` moves from `orders.rs` to `transfer.rs` — no public API change (it's still a method on `OrderExecutor`)
- All new methods are additive — no breaking changes
