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

pub use account::{HlAccountState, HlFill, HlPosition};
pub use candle::HlCandle;
pub use error::HlError;
pub use market::{HlAssetInfo, HlFundingRate, HlOrderbook};
pub use order::{LimitOrderType, OrderTypeWire, OrderWire, TriggerOrderType};
pub use response::OrderResponse;
pub use signature::Signature;
pub use util::normalize_coin;
