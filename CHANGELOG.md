# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [0.1.0] - Unreleased

### Added
- `hl-types`: shared domain types (orders, positions, candles, errors, signatures)
- `hl-signing`: EIP-712 signing with `Signer` trait and `PrivateKeySigner`
- `hl-client`: HTTP client with retry, rate-limit handling, optional WebSocket
- `hl-market`: market data queries (candles, orderbook, funding rates, asset metadata)
- `hl-account`: account state queries (positions, fills, vaults, agent approvals)
- `hl-executor`: order execution (place/cancel, trigger orders, position reconciliation)
- Comprehensive unit tests across all crates
- Live integration tests for testnet API (feature-gated, `#[ignore]`d — run with
  `cargo test --all-features -- --ignored`)
- README and usage examples
- Self-contained signing golden-vector tests (`hl-signing/tests/golden_vectors.rs`): a canonical-order
  L1 action-hash oracle, an L1 signature regression baseline, and a `usdSend` digest-recovery oracle
  (mainnet + testnet)

### Fixed
- **Signing (R1):** L1 action signatures are now valid — `motosan-wallet-core` 0.5.2 serializes the
  msgpack action in canonical (insertion) field order via `serde_json` `preserve_order`. The previous
  alphabetical key ordering produced an invalid `connectionId` that Hyperliquid rejected.
- **Signing (R2):** user-signed actions (`usdSend`, `approveAgent`, etc.) now sign with the canonical
  EIP-712 domain — chainId `421614` (`0x66eee`) for both mainnet and testnet — via
  `motosan-wallet-core` 0.5.2.
- **Wire format (R3):** SDK-computed order prices/sizes (market and trigger orders) are normalized to
  Hyperliquid wire rules — canonical float string (`normalize_wire`), ≤5 significant figures for
  non-integer prices (integers preserved), and `szDecimals` rounding for size. Trigger orders are
  routed through the validating builder, so a sub-lot size that rounds to zero is rejected rather
  than silently sent.
- **Idempotency (R4):** order writes auto-attach a client order id (`cloid`) so retried POSTs are
  deduplicated by the exchange; `place_trigger_order` now uses the canonical `0x`+32-hex cloid format.
- **Safety (R5):** `market_close` takes `size` as an unsigned magnitude and always derives the close
  side from the live position, removing the sign-encoded-direction footgun.
