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
- Live integration tests for testnet API (feature-gated)
- README and usage examples
- Self-contained signing golden-vector tests (`hl-signing/tests/golden_vectors.rs`): canonical-order
  L1 action-hash oracle, L1 signature regression baseline, and a `usdSend` digest-recovery oracle
  (mainnet + testnet)

### Fixed
- **Signing (R2):** user-signed actions (`usdSend`, `spotSend`, `withdraw3`, `approveAgent`,
  `setReferrer`, sub-account transfers) now sign with the canonical EIP-712 domain — chainId `421614`
  (`0x66eee`) for both networks — via `motosan-wallet-core` 0.5.2, with the posted `signatureChainId`
  set to `0x66eee`. Previously the testnet domain mismatched and these actions were rejected (mainnet
  worked only by coincidence).
- **Wire format (R3):** SDK-computed order prices/sizes (market and trigger orders) are rounded to
  Hyperliquid wire rules — ≤5 significant figures for non-integer prices (integers preserved) and
  `szDecimals` for size. Trigger orders route through the validating builder, so a sub-lot size that
  rounds to zero is rejected rather than silently sent.
- **Idempotency (R4):** order writes auto-attach a client order id (`cloid`) so retried POSTs are
  deduplicated; `place_trigger_order` uses the canonical `0x`+32-hex cloid format.
- **Safety (R5):** `market_close` takes `size` as an unsigned magnitude and derives the close side
  from the live position, removing the sign-encoded-direction footgun.
