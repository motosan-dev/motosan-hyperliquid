//! # hl-client
//!
//! HTTP and WebSocket client for the Hyperliquid exchange API.
//!
//! [`HyperliquidClient`] handles REST communication with automatic retry,
//! exponential backoff, and rate-limit awareness. Enable the `ws` feature
//! for [`HyperliquidWs`] WebSocket support with auto-reconnect.

#![warn(missing_docs)]

/// HTTP client builder and main [`HyperliquidClient`] implementation.
pub mod client;
/// Rate-limit tracking and configuration.
pub mod rate_limit;
/// Retry and timeout configuration for HTTP requests.
pub mod retry;
/// HTTP transport abstraction.
pub mod transport;

#[cfg(feature = "ws")]
/// WebSocket client with auto-reconnect and typed message parsing.
pub mod ws;

pub use client::{ClientBuilder, HyperliquidClient};
pub use rate_limit::RateLimitConfig;
pub use retry::{RetryConfig, TimeoutConfig};
pub use transport::HttpTransport;

#[cfg(feature = "ws")]
pub use ws::{
    ActiveAssetCtxData, ActiveAssetDataMsg, AllMidsData, BboData, CandleData,
    ClearinghouseStateData, HyperliquidWs, L2BookData, OrderUpdateData, PriceLevel, Subscription,
    TradesData, UserEventsData, UserFillsData, UserFundingsData, UserTwapHistoryData,
    UserTwapSliceFillsData, WebData3Data, WsConfig, WsMessage, WsOrderUpdate, WsTrade,
};
