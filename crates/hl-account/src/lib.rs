//! # hl-account
//!
//! Account state queries for Hyperliquid: positions, fills, vaults, and
//! agent approvals. All queries are read-only and require only a public
//! Ethereum address.

pub mod account;

pub use account::Account;
pub use account::{
    parse_account_state, parse_borrow_lend_state, parse_fills, parse_rate_limit_status,
    parse_spot_state, parse_staking_delegations, parse_user_fees,
};
