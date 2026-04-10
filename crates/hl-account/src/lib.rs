//! # hl-account
//!
//! Account state queries for Hyperliquid: positions, fills, vaults, and
//! agent approvals. All queries are read-only and require only a public
//! Ethereum address.

// TODO: upgrade to #![warn(missing_docs)] once public API is fully documented
#![allow(missing_docs)]

pub mod account;

pub use account::Account;
pub use account::{
    parse_account_state, parse_borrow_lend_state, parse_fills, parse_funding_history,
    parse_historical_orders, parse_open_orders, parse_order_status, parse_rate_limit_status,
    parse_spot_state, parse_staking_delegations, parse_user_fees, parse_user_funding,
};
