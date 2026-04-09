//! # hl-market
//!
//! Market data queries for Hyperliquid: candles, orderbook, funding rates,
//! and asset metadata. All data is returned as strongly-typed structs from
//! [`hl_types`].

pub mod market_data;
pub use market_data::MarketData;
