//! # hl-client
//!
//! HTTP and WebSocket client for the Hyperliquid exchange API.
//!
//! [`HyperliquidClient`] handles REST communication with automatic retry,
//! exponential backoff, and rate-limit awareness. Enable the `ws` feature
//! for [`HyperliquidWs`] WebSocket support with auto-reconnect.

pub mod client;
pub mod retry;

#[cfg(feature = "ws")]
pub mod ws;

pub use client::HyperliquidClient;
pub use retry::{RetryConfig, TimeoutConfig};

#[cfg(feature = "ws")]
pub use ws::{HyperliquidWs, WsConfig};
