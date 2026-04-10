//! # hl-market
//!
//! Market data queries for Hyperliquid: candles, orderbook, funding rates,
//! and asset metadata. All data is returned as strongly-typed structs from
//! [`hl_types`].

// TODO: upgrade to #![warn(missing_docs)] once public API is fully documented
#![allow(missing_docs)]

pub mod market_data;
pub use market_data::MarketData;
