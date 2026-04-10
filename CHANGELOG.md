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
- Comprehensive unit tests (118 tests across all crates)
- Live integration tests for testnet API (feature-gated)
- README and usage examples
