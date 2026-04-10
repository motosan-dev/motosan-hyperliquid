//! # hl-types
//!
//! Shared domain types for the motosan-hyperliquid SDK.
//!
//! This crate defines the Rust structs that map to Hyperliquid's API data
//! model: orders, positions, candles, signatures, and a unified error type.
//! Every other crate in the SDK depends on `hl-types`.
//!
//! This crate has no network dependencies.

pub mod account;
pub mod candle;
pub mod error;
pub mod market;
pub mod order;
pub mod response;
pub mod signature;
pub mod util;

pub use account::{
    HlAccountState, HlBorrowLendState, HlExtraAgent, HlFill, HlPosition, HlRateLimitStatus,
    HlStakingDelegation, HlUserFees, HlVaultDetails, HlVaultSummary,
};
pub use candle::HlCandle;
pub use error::HlError;
pub use market::{
    HlAssetInfo, HlFundingRate, HlOrderbook, HlPerpDexStatus, HlSpotAssetInfo, HlSpotBalance,
    HlSpotMeta, HlTrade, TradeSide,
};
pub use order::{
    CancelByCloidRequest, CancelRequest, LimitOrderType, ModifyRequest, OrderStatus, OrderTypeWire,
    OrderWire, OrderWireBuilder, PositionSide, Side, Tif, Tpsl, TriggerOrderType,
};
pub use response::{HlActionResponse, OrderResponse};
pub use rust_decimal::Decimal;
pub use signature::Signature;
pub use util::{normalize_coin, parse_str_decimal};
