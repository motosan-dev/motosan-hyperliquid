# User WebSocket Events Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add typed WebSocket subscriptions, typed message parsing, and convenience subscribe methods to `HyperliquidWs`.

**Architecture:** All changes are in `crates/hl-client/src/ws.rs` (single file). Add `Subscription` enum with serde serialization, `WsMessage` enum with `parse()` dispatch, data structs for each channel, convenience methods, and rename `subscribe()` → `subscribe_raw()`. Existing `next_message()` is unchanged; new `next_typed_message()` wraps it.

**Tech Stack:** Rust, serde, serde_json, tokio

**Spec:** `docs/superpowers/specs/2026-04-10-user-ws-events-design.md`

---

## File Structure

| File | Responsibility | Task |
|------|---------------|------|
| `crates/hl-client/src/ws.rs` | Modify: add Subscription, WsMessage, data structs, methods, tests | 1, 2, 3 |
| `crates/hl-client/src/lib.rs` | Modify: update re-exports | 3 |

---

### Task 1: Subscription Enum + subscribe_typed + convenience methods

**Files:**
- Modify: `crates/hl-client/src/ws.rs`

- [ ] **Step 1: Write serialization tests**

Add these tests to the existing `#[cfg(test)] mod tests` block in `ws.rs`:

```rust
#[test]
fn subscription_all_mids_serialization() {
    let sub = Subscription::AllMids;
    let json = serde_json::to_value(&sub).unwrap();
    assert_eq!(json, serde_json::json!({"type": "allMids"}));
}

#[test]
fn subscription_l2_book_serialization() {
    let sub = Subscription::L2Book { coin: "BTC".into() };
    let json = serde_json::to_value(&sub).unwrap();
    assert_eq!(json, serde_json::json!({"type": "l2Book", "coin": "BTC"}));
}

#[test]
fn subscription_trades_serialization() {
    let sub = Subscription::Trades { coin: "ETH".into() };
    let json = serde_json::to_value(&sub).unwrap();
    assert_eq!(json, serde_json::json!({"type": "trades", "coin": "ETH"}));
}

#[test]
fn subscription_candle_serialization() {
    let sub = Subscription::Candle { coin: "BTC".into(), interval: "1h".into() };
    let json = serde_json::to_value(&sub).unwrap();
    assert_eq!(json, serde_json::json!({"type": "candle", "coin": "BTC", "interval": "1h"}));
}

#[test]
fn subscription_user_fills_serialization() {
    let sub = Subscription::UserFills { user: "0xABC".into() };
    let json = serde_json::to_value(&sub).unwrap();
    assert_eq!(json, serde_json::json!({"type": "userFills", "user": "0xABC"}));
}

#[test]
fn subscription_order_updates_serialization() {
    let sub = Subscription::OrderUpdates { user: "0xDEF".into() };
    let json = serde_json::to_value(&sub).unwrap();
    assert_eq!(json, serde_json::json!({"type": "orderUpdates", "user": "0xDEF"}));
}

#[test]
fn subscription_user_events_serialization() {
    let sub = Subscription::UserEvents { user: "0x123".into() };
    let json = serde_json::to_value(&sub).unwrap();
    assert_eq!(json, serde_json::json!({"type": "userEvents", "user": "0x123"}));
}

#[test]
fn subscription_bbo_serialization() {
    let sub = Subscription::Bbo { coin: "SOL".into() };
    let json = serde_json::to_value(&sub).unwrap();
    assert_eq!(json, serde_json::json!({"type": "bbo", "coin": "SOL"}));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p hl-client --features ws -- subscription -v`
Expected: FAIL — `Subscription` type does not exist

- [ ] **Step 3: Add Subscription enum**

Add BEFORE the `WsConfig` struct in `ws.rs` (around line 21), after the imports:

```rust
use serde::Serialize;

/// Typed subscription for Hyperliquid WebSocket channels.
///
/// Serializes to the JSON format expected by the Hyperliquid WebSocket API.
/// Use with [`HyperliquidWs::subscribe_typed`] or the convenience methods.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[non_exhaustive]
pub enum Subscription {
    // Market data (public channels)
    /// Subscribe to all mid-prices.
    AllMids,
    /// Subscribe to L2 orderbook updates for a coin.
    L2Book { coin: String },
    /// Subscribe to trade events for a coin.
    Trades { coin: String },
    /// Subscribe to candlestick updates for a coin and interval.
    Candle { coin: String, interval: String },
    /// Subscribe to best bid/offer for a coin.
    Bbo { coin: String },

    // User channels (require user address, no auth token)
    /// Subscribe to order status updates.
    OrderUpdates { user: String },
    /// Subscribe to consolidated user events (fills, liquidations).
    UserEvents { user: String },
    /// Subscribe to fill notifications.
    UserFills { user: String },
    /// Subscribe to funding payment notifications.
    UserFundings { user: String },
    /// Subscribe to non-funding ledger updates (deposits, withdrawals).
    UserNonFundingLedgerUpdates { user: String },
    /// Subscribe to notifications.
    Notification { user: String },
    /// Subscribe to general web data.
    WebData2 { user: String },
}
```

- [ ] **Step 4: Rename subscribe → subscribe_raw and add subscribe_typed**

In `ws.rs`, rename the existing `subscribe` method to `subscribe_raw`:

```rust
/// Subscribe to a channel using raw JSON. Prefer [`subscribe_typed`] for
/// type-safe subscriptions; use this as an escape hatch for unsupported channels.
pub async fn subscribe_raw(&mut self, subscription: serde_json::Value) -> Result<(), HlError> {
    let msg = serde_json::json!({
        "method": "subscribe",
        "subscription": subscription,
    });
    self.subscriptions.push(msg.clone());
    self.send_raw(&msg).await
}

/// Subscribe to a channel using a typed [`Subscription`].
///
/// The subscription is remembered and re-sent automatically on reconnect.
pub async fn subscribe_typed(&mut self, sub: Subscription) -> Result<(), HlError> {
    let value = serde_json::to_value(&sub)
        .map_err(|e| HlError::serialization(format!("failed to serialize subscription: {e}")))?;
    self.subscribe_raw(value).await
}
```

- [ ] **Step 5: Add convenience methods**

Add these methods to the `impl HyperliquidWs` block:

```rust
// ── Market data convenience methods ─────────────────────

/// Subscribe to all mid-price updates.
pub async fn subscribe_all_mids(&mut self) -> Result<(), HlError> {
    self.subscribe_typed(Subscription::AllMids).await
}

/// Subscribe to L2 orderbook updates for a coin.
pub async fn subscribe_l2_book(&mut self, coin: &str) -> Result<(), HlError> {
    self.subscribe_typed(Subscription::L2Book { coin: coin.into() }).await
}

/// Subscribe to trade events for a coin.
pub async fn subscribe_trades(&mut self, coin: &str) -> Result<(), HlError> {
    self.subscribe_typed(Subscription::Trades { coin: coin.into() }).await
}

/// Subscribe to candlestick updates.
pub async fn subscribe_candle(&mut self, coin: &str, interval: &str) -> Result<(), HlError> {
    self.subscribe_typed(Subscription::Candle { coin: coin.into(), interval: interval.into() }).await
}

// ── User event convenience methods ──────────────────────

/// Subscribe to order status updates for a user address.
pub async fn subscribe_order_updates(&mut self, user: &str) -> Result<(), HlError> {
    self.subscribe_typed(Subscription::OrderUpdates { user: user.into() }).await
}

/// Subscribe to fill notifications for a user address.
pub async fn subscribe_user_fills(&mut self, user: &str) -> Result<(), HlError> {
    self.subscribe_typed(Subscription::UserFills { user: user.into() }).await
}

/// Subscribe to consolidated user events (fills, liquidations) for a user address.
pub async fn subscribe_user_events(&mut self, user: &str) -> Result<(), HlError> {
    self.subscribe_typed(Subscription::UserEvents { user: user.into() }).await
}

/// Subscribe to funding payment notifications for a user address.
pub async fn subscribe_user_fundings(&mut self, user: &str) -> Result<(), HlError> {
    self.subscribe_typed(Subscription::UserFundings { user: user.into() }).await
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p hl-client --features ws -v`
Expected: All tests pass including new subscription serialization tests

- [ ] **Step 7: Commit**

```bash
git add crates/hl-client/src/ws.rs
git commit -m "feat: add typed Subscription enum with convenience subscribe methods

Rename subscribe() to subscribe_raw(). New subscribe_typed() accepts
Subscription enum. Add 8 convenience methods: subscribe_l2_book,
subscribe_trades, subscribe_order_updates, subscribe_user_fills, etc."
```

---

### Task 2: WsMessage Enum + Data Structs + next_typed_message

**Files:**
- Modify: `crates/hl-client/src/ws.rs`

- [ ] **Step 1: Write message parsing tests**

Add to the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn parse_l2_book_message() {
    let raw = serde_json::json!({
        "channel": "l2Book",
        "data": {
            "coin": "BTC",
            "levels": [[{"px": "90000", "sz": "1.0"}], [{"px": "90001", "sz": "0.5"}]],
            "time": 1700000000000u64
        }
    });
    let msg = WsMessage::parse(raw);
    match msg {
        WsMessage::L2Book(data) => {
            assert_eq!(data.coin, "BTC");
            assert_eq!(data.time, 1700000000000);
        }
        other => panic!("expected L2Book, got: {:?}", other),
    }
}

#[test]
fn parse_user_fills_message() {
    let raw = serde_json::json!({
        "channel": "userFills",
        "data": {
            "user": "0xABC",
            "fills": [{"coin": "BTC", "px": "90000", "sz": "0.1"}]
        }
    });
    let msg = WsMessage::parse(raw);
    match msg {
        WsMessage::UserFills(data) => {
            assert_eq!(data.user, "0xABC");
            assert_eq!(data.fills.len(), 1);
        }
        other => panic!("expected UserFills, got: {:?}", other),
    }
}

#[test]
fn parse_order_updates_message() {
    let raw = serde_json::json!({
        "channel": "orderUpdates",
        "data": [{
            "order": {"oid": 123, "coin": "BTC"},
            "status": "filled",
            "statusTimestamp": 1700000000000u64
        }]
    });
    let msg = WsMessage::parse(raw);
    match msg {
        WsMessage::OrderUpdates(updates) => {
            assert_eq!(updates.len(), 1);
            assert_eq!(updates[0].status, "filled");
        }
        other => panic!("expected OrderUpdates, got: {:?}", other),
    }
}

#[test]
fn parse_subscription_response() {
    let raw = serde_json::json!({"channel": "subscriptionResponse", "data": {"method": "subscribe"}});
    let msg = WsMessage::parse(raw);
    assert!(matches!(msg, WsMessage::SubscriptionResponse));
}

#[test]
fn parse_unknown_channel_returns_unknown() {
    let raw = serde_json::json!({"channel": "futureChannel", "data": {}});
    let msg = WsMessage::parse(raw);
    assert!(matches!(msg, WsMessage::Unknown(_)));
}

#[test]
fn parse_malformed_message_returns_unknown() {
    let raw = serde_json::json!("just a string");
    let msg = WsMessage::parse(raw);
    assert!(matches!(msg, WsMessage::Unknown(_)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p hl-client --features ws -- parse_ -v`
Expected: FAIL — `WsMessage` does not exist

- [ ] **Step 3: Add data structs and WsMessage enum**

Add after the `Subscription` enum (before `WsConfig`):

```rust
// ── WebSocket Message Types ─────────────────────────────

/// Parsed WebSocket message from Hyperliquid.
///
/// Use [`WsMessage::parse`] to convert a raw `serde_json::Value` into a typed
/// message. Unrecognized channels fall through to [`WsMessage::Unknown`] — this
/// enum never fails to parse.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WsMessage {
    // Market data
    AllMids(AllMidsData),
    L2Book(L2BookData),
    Trades(TradesData),
    Candle(CandleData),
    Bbo(BboData),

    // User events
    OrderUpdates(Vec<OrderUpdateData>),
    UserEvents(UserEventsData),
    UserFills(UserFillsData),
    UserFundings(UserFundingsData),

    // Control
    SubscriptionResponse,

    // Fallback for unrecognized channels
    Unknown(serde_json::Value),
}

/// All mid-price snapshot.
#[derive(Debug, Clone)]
pub struct AllMidsData {
    pub mids: serde_json::Value,
}

/// L2 orderbook update.
#[derive(Debug, Clone)]
pub struct L2BookData {
    pub coin: String,
    pub levels: serde_json::Value,
    pub time: u64,
}

/// Trade events.
#[derive(Debug, Clone)]
pub struct TradesData {
    pub coin: String,
    pub trades: Vec<serde_json::Value>,
}

/// Candlestick update.
#[derive(Debug, Clone)]
pub struct CandleData {
    pub coin: String,
    pub candle: serde_json::Value,
}

/// Best bid/offer update.
#[derive(Debug, Clone)]
pub struct BboData {
    pub coin: String,
    pub data: serde_json::Value,
}

/// A single order status update.
#[derive(Debug, Clone)]
pub struct OrderUpdateData {
    pub order: serde_json::Value,
    pub status: String,
    pub timestamp: u64,
}

/// Consolidated user events (fills, liquidations, etc.).
#[derive(Debug, Clone)]
pub struct UserEventsData {
    pub events: Vec<serde_json::Value>,
}

/// User fill notification.
#[derive(Debug, Clone)]
pub struct UserFillsData {
    pub user: String,
    pub fills: Vec<serde_json::Value>,
}

/// User funding payment notification.
#[derive(Debug, Clone)]
pub struct UserFundingsData {
    pub user: String,
    pub funding: serde_json::Value,
}
```

- [ ] **Step 4: Add WsMessage::parse**

Add `impl WsMessage` after the data structs:

```rust
impl WsMessage {
    /// Parse a raw JSON value into a typed WebSocket message.
    ///
    /// Dispatches on the `"channel"` field. Unrecognized channels or
    /// malformed messages produce [`WsMessage::Unknown`] — this method
    /// never fails.
    pub fn parse(value: serde_json::Value) -> Self {
        let channel = value.get("channel").and_then(|c| c.as_str()).unwrap_or("");
        let data = value.get("data").cloned().unwrap_or(serde_json::Value::Null);

        match channel {
            "allMids" => WsMessage::AllMids(AllMidsData {
                mids: data.get("mids").cloned().unwrap_or(data.clone()),
            }),
            "l2Book" => WsMessage::L2Book(L2BookData {
                coin: data.get("coin").and_then(|c| c.as_str()).unwrap_or("").to_string(),
                levels: data.get("levels").cloned().unwrap_or_default(),
                time: data.get("time").and_then(|t| t.as_u64()).unwrap_or(0),
            }),
            "trades" => {
                let coin = data.get("coin").and_then(|c| c.as_str()).unwrap_or("").to_string();
                let trades = data.as_array().cloned().unwrap_or_default();
                WsMessage::Trades(TradesData { coin, trades })
            }
            "candle" => {
                let coin = data.get("s").and_then(|c| c.as_str()).unwrap_or("").to_string();
                WsMessage::Candle(CandleData {
                    coin,
                    candle: data,
                })
            }
            "bbo" => {
                let coin = data.get("coin").and_then(|c| c.as_str()).unwrap_or("").to_string();
                WsMessage::Bbo(BboData { coin, data })
            }
            "orderUpdates" => {
                let updates = data.as_array().map(|arr| {
                    arr.iter().map(|item| OrderUpdateData {
                        order: item.get("order").cloned().unwrap_or_default(),
                        status: item.get("status").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                        timestamp: item.get("statusTimestamp").and_then(|t| t.as_u64()).unwrap_or(0),
                    }).collect()
                }).unwrap_or_default();
                WsMessage::OrderUpdates(updates)
            }
            "user" => WsMessage::UserEvents(UserEventsData {
                events: data.as_array().cloned().unwrap_or_default(),
            }),
            "userFills" => WsMessage::UserFills(UserFillsData {
                user: data.get("user").and_then(|u| u.as_str()).unwrap_or("").to_string(),
                fills: data.get("fills").and_then(|f| f.as_array()).cloned().unwrap_or_default(),
            }),
            "userFundings" => WsMessage::UserFundings(UserFundingsData {
                user: data.get("user").and_then(|u| u.as_str()).unwrap_or("").to_string(),
                funding: data,
            }),
            "subscriptionResponse" => WsMessage::SubscriptionResponse,
            _ => WsMessage::Unknown(value),
        }
    }
}
```

- [ ] **Step 5: Add next_typed_message method**

Add to `impl HyperliquidWs`:

```rust
/// Read the next typed message from the WebSocket.
///
/// This is a convenience wrapper around [`next_message`] that parses the
/// raw JSON into a [`WsMessage`]. Unrecognized channels become
/// [`WsMessage::Unknown`].
pub async fn next_typed_message(&mut self) -> Option<Result<WsMessage, HlError>> {
    let raw = self.next_message().await?;
    Some(raw.map(WsMessage::parse))
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p hl-client --features ws -v`
Expected: All tests pass including new message parsing tests

- [ ] **Step 7: Commit**

```bash
git add crates/hl-client/src/ws.rs
git commit -m "feat: add WsMessage enum with typed parsing and next_typed_message

Typed envelope + semi-typed data for each channel. WsMessage::parse()
dispatches on channel field; unknown channels become WsMessage::Unknown.
New next_typed_message() wraps next_message() with parsing."
```

---

### Task 3: Update re-exports + final verification

**Files:**
- Modify: `crates/hl-client/src/lib.rs`

- [ ] **Step 1: Update lib.rs re-exports**

Change:

```rust
// Before:
#[cfg(feature = "ws")]
pub use ws::{HyperliquidWs, WsConfig};

// After:
#[cfg(feature = "ws")]
pub use ws::{
    HyperliquidWs, Subscription, WsConfig, WsMessage,
    // Data structs
    AllMidsData, BboData, CandleData, L2BookData, OrderUpdateData,
    TradesData, UserEventsData, UserFillsData, UserFundingsData,
};
```

- [ ] **Step 2: Run full test suite**

Run: `cargo test`
Expected: All tests pass across all crates

Run: `cargo test --features ws`
Expected: All ws-related tests pass

- [ ] **Step 3: Run clippy**

Run: `cargo clippy --all-features --all-targets -- -D warnings`
Expected: Clean

- [ ] **Step 4: Commit**

```bash
git add crates/hl-client/src/lib.rs
git commit -m "feat: export Subscription, WsMessage, and data structs from hl-client"
```
