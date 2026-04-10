//! # hl-executor
//!
//! Order execution for the Hyperliquid L1: place/cancel orders, trigger
//! orders (stop-loss, take-profit), vault transfers, and position
//! reconciliation. Handles EIP-712 signing and nonce management internally.

// TODO: upgrade to #![warn(missing_docs)] once public API is fully documented
#![allow(missing_docs)]

pub mod executor;
pub mod meta_cache;
pub mod reconcile;

pub use executor::OrderExecutor;
pub use meta_cache::AssetMetaCache;
pub use reconcile::{reconcile_positions, LocalPosition, ReconcileAction, ReconcileReport};
