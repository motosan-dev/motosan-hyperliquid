---
name: motosan-hyperliquid
description: Help developers use the motosan-hyperliquid SDK (Rust) — market data, account queries, EIP-712 signing, WebSocket feeds, and order execution on Hyperliquid L1. Use when code imports hl_client/hl_market/hl_account/hl_executor/hl_signing/hl_types, or user asks how to query Hyperliquid, place orders, sign transactions, manage vault transfers, use TP/SL brackets, or set up WebSocket feeds.
---

# motosan-hyperliquid SDK

Modular Rust SDK for Hyperliquid L1 — latest published release **v0.3.0** (MSRV Rust 1.91+).

7 published crates: `hl-types`, `hl-signing`, `hl-client`, `hl-market`, `hl-account`, `hl-executor`, and the `motosan-hyperliquid` facade.

Release 0.3.0 includes builder-code order actions, OCO / TP-SL grouped bulk orders, vault withdrawals, richer account queries (`frontend_open_orders`, `order_status_by_cloid`, `fills_by_time`), and action expiry via `OrderExecutor::set_expires_after`.

## Install

```toml
# Option A — single facade crate (simplest)
[dependencies]
motosan-hyperliquid = "0.3.0" # all sub-crates via the `full` feature

# Option B — pick crates
[dependencies]
hl-client   = "0.3.0" # HTTP + optional WebSocket
hl-types    = "0.3.0" # shared domain types
hl-market   = "0.3.0" # market data queries
hl-account  = "0.3.0" # account state queries
hl-signing  = "0.3.0" # EIP-712 signing
hl-executor = "0.3.0" # order execution

# Enable WebSocket
hl-client = { version = "0.3.0", features = ["ws"] }
```

## Architecture

```text
hl-types (pure data)
    |
hl-signing → hl-client
               / \
        hl-market  hl-account
               \   /
            hl-executor
```

## Minimal Example

```rust
use hl_client::HyperliquidClient;
use hl_market::MarketData;

let client = HyperliquidClient::mainnet()?;
let market = MarketData::from_client(client);
let book = market.orderbook("BTC").await?;
println!("Best bid: {:?}", book.bids[0]);
```

## Recent Execution APIs

```rust
use hl_types::Grouping;

// L1 replay protection: unix epoch milliseconds; None clears it.
executor.set_expires_after(Some(epoch_ms + 60_000));

// Builder fee is in tenths of a bp (10 = 1 bp).
executor
    .place_order_with_builder(order.clone(), Some(("0x1111111111111111111111111111111111111111", 10)), None)
    .await?;

// OCO / TP-SL bracket: parent entry first, reduce-only children after it.
executor
    .bulk_order_grouped(vec![parent, take_profit, stop_loss], Grouping::NormalTpsl, None)
    .await?;

// Explicit vault direction.
executor.deposit_to_vault(vault, amount).await?;
executor.withdraw_from_vault(vault, amount).await?;
```

## When to Read References

| Task | File |
|------|------|
| Client setup, retry config, timeout config, WebSocket, action payload expiry | `references/client.md` |
| Market data — candles, orderbook, funding, mid-price | `references/market.md` |
| Account — positions, fills, frontend open orders, vaults, fees, funding | `references/account.md` |
| Order execution — place/cancel, triggers, builders, OCO grouping, vault transfers, action expiry | `references/execution.md` |
| EIP-712 signing — Signer trait, PrivateKeySigner, L1 expiry signing | `references/signing.md` |
| Domain types — OrderWire, Grouping, HlError, account/order structs | `references/types.md` |
| Release process, version bump, tag convention, CI | `references/release.md` |

## Key Design Decisions

- **Layered dependencies** — use `hl-market` for read-only data without signing/execution deps.
- **Unified error type** — `hl_types::HlError` across all crates.
- **Automatic retry** — client handles 429 and 5xx with exponential backoff.
- **Coin normalization** — normalize symbols before lookup (`"BTC-PERP"` → `"BTC"`).
- **Feature-gated WebSocket** — `hl-client` `ws` feature.
- **Execution safety** — Decimal builders validate positive price/size, write helpers auto-attach CLOIDs, and `set_expires_after` can reject stale L1 actions.
