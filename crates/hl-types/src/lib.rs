//! # hl-types
//!
//! Shared domain types for the motosan-hyperliquid SDK.
//!
//! This crate defines the Rust structs that map to Hyperliquid's API data
//! model: orders, positions, candles, signatures, and a unified error type.
//! Every other crate in the SDK depends on `hl-types`.
//!
//! This crate has no network dependencies.

#![warn(missing_docs)]

/// Account state types (positions, fills, vaults, fees, etc.).
pub mod account;
/// OHLCV candlestick types.
pub mod candle;
/// Unified error type used across all SDK crates.
pub mod error;
/// Market data types (orderbook, asset info, funding rates, trades).
pub mod market;
/// Order wire types (order builders, cancel/modify requests, enums).
pub mod order;
/// Exchange action and order response types.
pub mod response;
/// ECDSA signature type (r, s, v components).
pub mod signature;
/// Utility functions (coin normalization, JSON parsing helpers).
pub mod util;

pub use account::{
    HlAccountState, HlActiveAssetData, HlBorrowLendState, HlExtraAgent, HlFill, HlFundingEntry,
    HlHistoricalOrder, HlOpenOrder, HlOrderDetail, HlPosition, HlRateLimitStatus, HlReferralState,
    HlStakingDelegation, HlUserFees, HlUserFundingEntry, HlVaultDetails, HlVaultSummary,
};
pub use candle::HlCandle;
pub use error::HlError;
pub use market::{
    AssetContext, HlAssetInfo, HlFundingRate, HlOrderbook, HlPerpDexStatus, HlSpotAssetInfo,
    HlSpotBalance, HlSpotMeta, HlTrade, SpotAssetContext, TradeSide,
};
pub use order::{
    CancelByCloidRequest, CancelRequest, LimitOrderType, ModifyRequest, OrderStatus, OrderTypeWire,
    OrderWire, OrderWireBuilder, PositionSide, Side, Tif, Tpsl, TriggerOrderType,
};
pub use response::{HlActionResponse, OrderResponse};
pub use rust_decimal::Decimal;
pub use signature::Signature;
pub use util::{
    normalize_coin, parse_mid_price_from_l2book, parse_position_szi, parse_str_decimal,
};
