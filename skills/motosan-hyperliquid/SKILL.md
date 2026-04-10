---
name: motosan-hyperliquid
description: Help developers use the motosan-hyperliquid SDK (Rust) — market data, account queries, EIP-712 signing, and order execution on Hyperliquid L1. Use when code imports hl_client/hl_market/hl_account/hl_executor/hl_signing/hl_types, or user asks how to query Hyperliquid, place orders, sign transactions, or set up WebSocket feeds.
---

# motosan-hyperliquid SDK

Modular Rust SDK for Hyperliquid L1 — v0.1.0

6 crates: `hl-types`, `hl-signing`, `hl-client`, `hl-market`, `hl-account`, `hl-executor`

## Install

```toml
# Cargo.toml — pick the crates you need
[dependencies]
hl-client  = "0.1.0"    # HTTP + optional WebSocket
hl-types   = "0.1.0"    # shared domain types
hl-market  = "0.1.0"    # market data queries
hl-account = "0.1.0"    # account state queries
hl-signing = "0.1.0"    # EIP-712 signing
hl-executor = "0.1.0"   # order execution

# Enable WebSocket
hl-client = { version = "0.1.0", features = ["ws"] }
```

## Architecture

```
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
let market = MarketData::new(client);
let book = market.orderbook("BTC").await?;
println!("Best bid: {:?}", book.bids[0]);
```

## When to Read References

| Task | File |
|------|------|
| Client setup, retry config, timeout config, WebSocket | `references/client.md` |
| Market data — candles, orderbook, funding, mid-price | `references/market.md` |
| Account — positions, fills, vaults, agent approvals | `references/account.md` |
| Order execution — place/cancel, triggers, reconciliation | `references/execution.md` |
| EIP-712 signing — Signer trait, PrivateKeySigner | `references/signing.md` |
| Domain types — OrderWire, HlError, all structs | `references/types.md` |
| Release process, version bump, tag convention, CI | `references/release.md` |

## Key Design Decisions

- **Layered dependencies** — use `hl-market` for read-only data without pulling signing/execution
- **Unified error type** — `hl_types::HlError` across all crates, with `is_retryable()` helper
- **Automatic retry** — client handles 429 and 5xx with exponential backoff
- **Coin normalization** — all market/account methods accept raw symbols ("BTC-PERP" → "BTC")
- **Feature-gated WebSocket** — `hl-client` `ws` feature, not pulled by default
