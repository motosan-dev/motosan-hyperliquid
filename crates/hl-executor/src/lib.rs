//! # hl-executor
//!
//! Order execution for the Hyperliquid L1: place/cancel orders, trigger
//! orders (stop-loss, take-profit), vault transfers, and position
//! reconciliation. Handles EIP-712 signing and nonce management internally.

#![warn(missing_docs)]

/// Order execution engine (place, cancel, modify, trigger, TWAP, transfers).
pub mod executor;
/// Asset metadata cache for resolving coin symbols to asset IDs.
pub mod meta_cache;
/// Position reconciliation between local state and the exchange.
pub mod reconcile;

pub use executor::OrderExecutor;
pub use meta_cache::AssetMetaCache;
pub use reconcile::{reconcile_positions, LocalPosition, ReconcileAction, ReconcileReport};
