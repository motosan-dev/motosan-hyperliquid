# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

## [0.3.0] - 2026-06-04

Parity features closing high-value gaps with the official Hyperliquid Python SDK.

### Added
- **Builder codes**: `place_order_with_builder`, `bulk_order_with_builder`, and
  `place_trigger_order_with_builder` attach a `(builder_address, fee)` to the order
  action (`{"b","f"}`, fee in tenths of a basis point) so a builder earns the
  configured fee. Pairs with the existing `approve_builder_fee`.
- **Order grouping / OCO brackets**: `bulk_order_grouped(orders, Grouping, vault)`
  plus a new `Grouping` enum (`Na` / `NormalTpsl` / `PositionTpsl`) to link a parent
  entry with TP/SL children. `bulk_order` keeps its signature (delegates with `Na`).
- **Vault withdrawals**: `vault_transfer(vault, is_deposit, amount)` with
  `deposit_to_vault` / `withdraw_from_vault` wrappers — the vault transfer was
  previously deposit-only.
- **Info queries**: `frontend_open_orders` (richer order view with trigger
  conditions, TP/SL metadata, and children — new `HlFrontendOpenOrder` type),
  `order_status_by_cloid` (look up an order by client order id), and
  `fills_by_time(address, start_ms, end_ms, aggregate_by_time)` (time-ranged fills).
- **Action expiry**: `OrderExecutor::set_expires_after(Some(epoch_ms))` folds an
  `expiresAfter` timestamp into every subsequent signed L1 action (and the
  `/exchange` body) so the exchange rejects stale/replayed actions; `None`
  clears it. Requires `motosan-wallet-core` 0.5.3 (adds `sign_l1_action_with_expiry`).

### Fixed
- **Vault transfer amount encoding**: `transfer_to_vault` now sends `usd` as an
  integer in micro-units (6 decimals) to match the `vaultTransfer` wire format;
  previously it sent a whole-dollar string, which the exchange rejects.

## [0.2.0] - 2026-06-03

Second release. Major expansion of the exchange-action and info-query surface (near-parity with the
official Hyperliquid Python SDK), production-safety hardening, a unified facade crate, and four
critical signing/wire correctness fixes.

> **Upgrade note:** `0.1.0` shipped before the signing/wire fixes below — all users should move to
> `0.2.0`. MSRV is now **1.91** (driven by the `alloy` 1.8.3 dependency stack).

### Added
- **Facade crate `motosan-hyperliquid`** — single-crate entry point that re-exports the sub-crates
  behind feature flags (`market`, `account`, `executor`, `signing`, `ws`, `full`), plus an expanded
  `prelude`.
- **Exchange actions** (`hl-executor`): `bulk_order`, `place_order_by_symbol`, `modify_order` /
  `bulk_modify`, `bulk_cancel`, `cancel_by_cloid` / `bulk_cancel_by_cloid`, `market_open` /
  `market_close`, `place_scale_order`, `place_twap_order` / `cancel_twap`, `update_leverage` /
  `update_isolated_margin`, `schedule_cancel`, `claim_rewards`, `approve_agent`,
  `approve_builder_fee`, `set_referrer`.
- **Spot trading**: `place_spot_order`, `bulk_spot_order`, `spot_market_open`, `cancel_spot_order`,
  with spot asset/`szDecimals` resolution in the meta cache.
- **Transfers**: `usdc_transfer` (`usdSend`), `spot_send` (`spotSend`), `withdraw` (`withdraw3`),
  `class_transfer` (perp↔spot), `send_asset` (cross-chain `sendAsset`), `transfer_to_vault`, and
  sub-account create/modify/transfer.
- **Info queries** (`hl-account`): `open_orders`, `order_status`, `funding_history`, `user_funding`,
  `historical_orders`, `staking_delegations`.
- **Typed WebSocket** (`hl-client` `ws` feature): `Subscription` enum, never-failing `WsMessage::parse`
  with strongly-typed data structs, `subscribe_typed` + per-channel convenience methods
  (`subscribe_l2_book`, `subscribe_user_fills`, …), and `next_typed_message`.
- **Developer experience**: `Decimal`-accepting order builders, validated `OrderWireBuilder::build()`
  (rejects non-positive price/size), `Side::from_is_buy`, normalized meta-cache lookups on the hot
  path, and new examples (`examples/ws_stream.rs`, `examples/trigger_order.rs`).
- **Production safety** (`hl-client`): proactive token-bucket rate limiter + concurrency gate
  (`RateLimitConfig`), `#[tracing::instrument]` spans across executor/market/account/client, graceful
  shutdown via `CancellationToken` (`shutdown_token`), config validation at construction
  (`RetryConfig`/`TimeoutConfig`/`RateLimitConfig::validate`), and error source-chain preservation
  with new `HlError::Config`/`Validation` variants.
- **Signing golden-vector tests** (`hl-signing/tests/golden_vectors.rs`): canonical-order L1
  action-hash oracle, L1 signature regression baseline, and a `usdSend` digest-recovery oracle
  (mainnet + testnet).
- **CI/CD**: `ci-rust.yml` (fmt/clippy/test on stable + MSRV) and `publish-rust.yml` (ordered
  crates.io publish).

### Fixed
- **Signing (R2):** user-signed actions (`usdSend`, `spotSend`, `withdraw3`, `approveAgent`,
  `setReferrer`, sub-account transfers) now sign with the canonical EIP-712 domain — chainId `421614`
  (`0x66eee`) for both networks — via `motosan-wallet-core` 0.5.2, with the posted `signatureChainId`
  set to `0x66eee`. Previously the testnet domain mismatched and these actions were rejected (mainnet
  worked only by coincidence).
- **Wire format (R3):** SDK-computed order prices/sizes (market, trigger, scale, twap, spot orders)
  are rounded to Hyperliquid wire rules — ≤5 significant figures for non-integer prices (integers
  preserved) and `szDecimals` for size. Orders route through the validating builder, so a sub-lot
  size that rounds to zero is rejected rather than silently sent.
- **Idempotency (R4):** order writes auto-attach a client order id (`cloid`) so retried POSTs are
  deduplicated; `place_trigger_order` uses the canonical `0x`+32-hex cloid format.
- **Safety (R5):** `market_close` takes `size` as an unsigned magnitude and derives the close side
  from the live position, removing the sign-encoded-direction footgun.

### Internal
- `PrivateKeySigner` now carries a compile-time assertion that its inner `k256::ecdsa::SigningKey` is
  `ZeroizeOnDrop`, locking in the existing guarantee that secret key material is wiped from memory on
  drop (and replacing a comment that referenced a non-existent `k256` `zeroize` feature).

## [0.1.0] - 2026-04-11

Initial release.

### Added
- `hl-types`: shared domain types (orders, positions, candles, errors, signatures)
- `hl-signing`: EIP-712 signing with `Signer` trait and `PrivateKeySigner`
- `hl-client`: HTTP client with retry, rate-limit handling, optional WebSocket
- `hl-market`: market data queries (candles, orderbook, funding rates, asset metadata)
- `hl-account`: account state queries (positions, fills, vaults, agent approvals)
- `hl-executor`: order execution (place/cancel, trigger orders, position reconciliation)
- Comprehensive unit tests across all crates
- Live integration tests for testnet API (feature-gated)
- README and usage examples
