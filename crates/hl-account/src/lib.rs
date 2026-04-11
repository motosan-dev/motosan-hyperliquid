//! # hl-account
//!
//! Account state queries for Hyperliquid: positions, fills, vaults, and
//! agent approvals. All queries are read-only and require only a public
//! Ethereum address.

#![warn(missing_docs)]

/// Account state query methods (positions, fills, orders, vaults, fees).
pub mod account;

pub use account::Account;
