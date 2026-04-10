# User WebSocket Events — Design Spec

> Sub-project 2 of Phase 2 (High-Value Features). Add typed subscriptions, typed messages, and convenience methods to the WebSocket client.

## Context

The current `HyperliquidWs` accepts `serde_json::Value` for both subscriptions and messages. Users must manually construct subscription JSON and parse incoming messages. The official Hyperliquid Rust SDK provides a typed `Subscription` enum with 14 channel variants and a typed `Message` enum for responses.

## Scope

Add typed layer on top of the existing untyped WebSocket implementation. No changes to the underlying connect/reconnect/heartbeat logic.

- Typed `Subscription` enum (12 variants)
- Typed `WsMessage` enum with semi-typed data structs
- Convenience subscribe methods (8 methods)
- ~14 unit tests

Out of scope: unsubscribe methods, WS authentication, full field-level typing of all message data.

---

## Subscription Enum

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Subscription {
    // Market data (public)
    AllMids,
    L2Book { coin: String },
    Trades { coin: String },
    Candle { coin: String, interval: String },
    Bbo { coin: String },

    // User events (require address, no auth token)
    OrderUpdates { user: String },
    UserEvents { user: String },
    UserFills { user: String },
    UserFundings { user: String },
    UserNonFundingLedgerUpdates { user: String },
    Notification { user: String },
    WebData2 { user: String },
}
```

Serialization uses `#[serde(tag = "type")]` with camelCase renaming to match the Hyperliquid wire format:
- `Subscription::L2Book { coin: "BTC".into() }` → `{"type": "l2Book", "coin": "BTC"}`
- `Subscription::UserFills { user: "0x...".into() }` → `{"type": "userFills", "user": "0x..."}`

---

## WsMessage Enum

**Design principle**: Typed envelope + semi-typed data. Key fields (coin, user, status, timestamp) are typed. Detail fields stay `serde_json::Value` to avoid breaking when the API adds fields.

```rust
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
    Pong,

    // Fallback
    Unknown(serde_json::Value),
}
```

### Data Structs

```rust
pub struct AllMidsData {
    pub mids: serde_json::Value,
}

pub struct L2BookData {
    pub coin: String,
    pub levels: serde_json::Value,
    pub time: u64,
}

pub struct TradesData {
    pub coin: String,
    pub trades: Vec<serde_json::Value>,
}

pub struct CandleData {
    pub coin: String,
    pub candle: serde_json::Value,
}

pub struct BboData {
    pub coin: String,
    pub data: serde_json::Value,
}

pub struct OrderUpdateData {
    pub order: serde_json::Value,
    pub status: String,
    pub timestamp: u64,
}

pub struct UserEventsData {
    pub events: Vec<serde_json::Value>,
}

pub struct UserFillsData {
    pub user: String,
    pub fills: Vec<serde_json::Value>,
}

pub struct UserFundingsData {
    pub user: String,
    pub funding: serde_json::Value,
}
```

All structs derive `Debug, Clone`.

### Parse Logic

`WsMessage::parse(value: serde_json::Value) -> WsMessage` — dispatches on the `channel` field:

```
channel == "allMids"      → AllMids
channel == "l2Book"       → L2Book
channel == "trades"       → Trades
channel == "candle"       → Candle
channel == "bbo"          → Bbo
channel == "orderUpdates" → OrderUpdates
channel == "user"         → UserEvents
channel == "userFills"    → UserFills
channel == "userFundings" → UserFundings
"subscriptionResponse"    → SubscriptionResponse
"pong" in message         → Pong
anything else             → Unknown(original value)
```

Never fails — unrecognized messages become `Unknown`.

---

## API Changes to HyperliquidWs

### New Methods

```rust
/// Subscribe using a typed subscription.
pub async fn subscribe_typed(&mut self, sub: Subscription) -> Result<(), HlError>

/// Receive the next typed message.
pub async fn next_typed_message(&mut self) -> Option<Result<WsMessage, HlError>>
```

### Convenience Methods

```rust
// Market data
pub async fn subscribe_all_mids(&mut self) -> Result<(), HlError>
pub async fn subscribe_l2_book(&mut self, coin: &str) -> Result<(), HlError>
pub async fn subscribe_trades(&mut self, coin: &str) -> Result<(), HlError>
pub async fn subscribe_candle(&mut self, coin: &str, interval: &str) -> Result<(), HlError>

// User events
pub async fn subscribe_order_updates(&mut self, user: &str) -> Result<(), HlError>
pub async fn subscribe_user_fills(&mut self, user: &str) -> Result<(), HlError>
pub async fn subscribe_user_events(&mut self, user: &str) -> Result<(), HlError>
pub async fn subscribe_user_fundings(&mut self, user: &str) -> Result<(), HlError>
```

### Preserved Methods

```rust
// Renamed from subscribe → subscribe_raw (escape hatch)
pub async fn subscribe_raw(&mut self, subscription: serde_json::Value) -> Result<(), HlError>

// Unchanged
pub async fn next_message(&mut self) -> Option<Result<serde_json::Value, HlError>>
```

**Breaking change**: `subscribe()` is renamed to `subscribe_raw()`. The old name is too generic and conflicts conceptually with `subscribe_typed()`. Since the SDK is pre-1.0 (v0.1.0), this is acceptable.

---

## File Structure

All additions in `crates/hl-client/src/ws.rs`. No new files — the ws module stays as a single file (~800 lines after changes). New types are feature-gated behind `ws` like the rest of the module.

### Re-exports

Update `crates/hl-client/src/lib.rs`:

```rust
#[cfg(feature = "ws")]
pub use ws::{HyperliquidWs, WsConfig, Subscription, WsMessage};
```

---

## Testing Strategy

### Subscription Serialization (~8 tests)
- Each variant serializes to correct JSON format
- `AllMids` → `{"type": "allMids"}`
- `L2Book { coin: "BTC" }` → `{"type": "l2Book", "coin": "BTC"}`
- `UserFills { user: "0x..." }` → `{"type": "userFills", "user": "0x..."}`
- `Candle { coin: "ETH", interval: "1h" }` → `{"type": "candle", "coin": "ETH", "interval": "1h"}`

### Message Deserialization (~6 tests)
- L2Book channel → `WsMessage::L2Book` with correct coin
- UserFills channel → `WsMessage::UserFills` with fills array
- OrderUpdates channel → `WsMessage::OrderUpdates` with status
- Subscription response → `WsMessage::SubscriptionResponse`
- Unknown channel → `WsMessage::Unknown`
- Malformed → `WsMessage::Unknown` (never panics)

### No Live WS Tests
WebSocket live tests require long connections + event triggers. Not suitable for CI.

---

## Migration Impact

- `subscribe()` renamed to `subscribe_raw()` — breaking for existing callers (pre-1.0, acceptable)
- New types (`Subscription`, `WsMessage`, data structs) are additive
- `next_message()` unchanged — users can migrate at their own pace
- All new types are `#[cfg(feature = "ws")]`
