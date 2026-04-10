//! HTTP transport abstraction for testability.
//!
//! The [`HttpTransport`] trait decouples higher-level SDK crates
//! (`hl-market`, `hl-account`, `hl-executor`) from the concrete
//! [`HyperliquidClient`](crate::HyperliquidClient) so that downstream
//! consumers can provide mock implementations for unit testing without
//! real HTTP calls.

use async_trait::async_trait;
use hl_types::{HlError, Signature};

/// Abstraction over the Hyperliquid HTTP layer.
///
/// Implement this trait to provide a mock or alternative HTTP backend.
/// The SDK ships a production implementation on
/// [`HyperliquidClient`](crate::HyperliquidClient).
#[async_trait]
pub trait HttpTransport: Send + Sync {
    /// POST a JSON payload to the `/info` read-only endpoint.
    async fn post_info(&self, request: serde_json::Value) -> Result<serde_json::Value, HlError>;

    /// POST a signed action to the `/exchange` endpoint.
    async fn post_action(
        &self,
        action: serde_json::Value,
        signature: &Signature,
        nonce: u64,
        vault_address: Option<&str>,
    ) -> Result<serde_json::Value, HlError>;

    /// Whether this transport targets mainnet (affects EIP-712 chain id).
    fn is_mainnet(&self) -> bool;
}
