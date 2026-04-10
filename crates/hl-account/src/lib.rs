//! # hl-account
//!
//! Account state queries for Hyperliquid: positions, fills, vaults, and
//! agent approvals. All queries are read-only and require only a public
//! Ethereum address.

// TODO: upgrade to #![warn(missing_docs)] once public API is fully documented
#![allow(missing_docs)]

pub mod account;

pub use account::Account;
