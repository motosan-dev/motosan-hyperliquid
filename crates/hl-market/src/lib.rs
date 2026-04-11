//! # hl-market
//!
//! Market data queries for Hyperliquid: candles, orderbook, funding rates,
//! and asset metadata. All data is returned as strongly-typed structs from
//! [`hl_types`].

#![warn(missing_docs)]

/// Market data query methods (candles, orderbook, funding, assets, trades).
pub mod market_data;
pub use market_data::MarketData;
