//! # motosan-hyperliquid
//!
//! Unified facade for the **motosan-hyperliquid** SDK.
//!
//! Instead of adding individual crates to your `Cargo.toml`, depend on this
//! single crate and enable the features you need:
//!
//! ```toml
//! [dependencies]
//! motosan-hyperliquid = { version = "0.1", features = ["full"] }
//! ```
//!
//! Then import commonly used types via the prelude:
//!
//! ```rust,ignore
//! use motosan_hyperliquid::prelude::*;
//! ```
//!
//! ## Feature flags
//!
//! | Flag | Default | Description |
//! |------|---------|-------------|
//! | `market` | yes (via `full`) | Re-exports [`hl_market::MarketData`] |
//! | `account` | yes (via `full`) | Re-exports [`hl_account::Account`] |
//! | `executor` | yes (via `full`) | Re-exports [`hl_executor::OrderExecutor`] and reconciliation helpers |
//! | `signing` | yes (via `full`) | Re-exports [`hl_signing::Signer`], [`hl_signing::PrivateKeySigner`], and EIP-712 helpers |
//! | `ws` | yes (via `full`) | Enables WebSocket support via [`hl_client::HyperliquidWs`] |
//! | `full` | **yes** | Enables all of the above |
//!
//! With `--no-default-features`, only [`hl_types`] and the core
//! [`hl_client::HyperliquidClient`] / [`hl_client::HttpTransport`] are
//! available.

#![warn(missing_docs)]

pub mod prelude;

// ---------------------------------------------------------------------------
// Always-available re-exports
// ---------------------------------------------------------------------------

/// Core domain types (orders, positions, candles, errors, etc.).
pub use hl_types;

/// HTTP client and transport trait.
pub use hl_client;

// ---------------------------------------------------------------------------
// Feature-gated re-exports
// ---------------------------------------------------------------------------

/// Market data queries (candles, orderbook, funding rates, asset info).
#[cfg(feature = "market")]
pub use hl_market;

/// Account state queries (positions, fills, vaults, agent approvals).
#[cfg(feature = "account")]
pub use hl_account;

/// Order execution (place/cancel, triggers, reconciliation).
#[cfg(feature = "executor")]
pub use hl_executor;

/// EIP-712 signing (Signer trait, PrivateKeySigner, action hashing).
#[cfg(feature = "signing")]
pub use hl_signing;
