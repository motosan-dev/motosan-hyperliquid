//! # hl-market
//!
//! Market data queries for Hyperliquid: candles, orderbook, funding rates,
//! and asset metadata. All data is returned as strongly-typed structs from
//! [`hl_types`].

pub mod market_data;
pub use market_data::parse_spot_meta;
pub use market_data::MarketData;
pub use market_data::{parse_perp_dex_status, parse_perps_at_oi_cap};
