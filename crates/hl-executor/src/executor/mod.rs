use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rust_decimal::Decimal;

use hl_client::HttpTransport;
use hl_signing::{sign_l1_action, Signer};
use hl_types::*;

use crate::meta_cache::AssetMetaCache;

pub mod admin;
pub mod cancel;
pub mod leverage;
pub mod modify;
pub mod orders;
pub mod response;
pub mod sub_account;
pub mod transfer;
pub mod twap;

/// Normalize a market symbol to its base coin name.
///
/// Uses [`hl_types::normalize_coin`] to strip common suffixes (-PERP, -USDC,
/// -USD) and then uppercases the result.
pub(crate) fn normalize_symbol(symbol: &str) -> String {
    normalize_coin(symbol).to_uppercase()
}

/// The fill-size threshold ratio used to distinguish "filled" from "partial".
///
/// If `fill_size >= requested_size * FILL_THRESHOLD` the order is considered
/// fully filled.
pub(crate) const FILL_THRESHOLD: Decimal = Decimal::from_parts(99, 0, 0, false, 2); // 0.99

/// Standalone order executor for the Hyperliquid L1.
///
/// Provides methods to place, cancel, and manage orders without any
/// hyper-agent-specific dependencies (no `OrderSubmitter` trait, no
/// `PositionManager`).
pub struct OrderExecutor {
    pub(crate) client: Arc<dyn HttpTransport>,
    pub(crate) signer: Box<dyn Signer>,
    pub(crate) address: String,
    pub(crate) meta_cache: AssetMetaCache,
    /// Per-instance monotonically increasing nonce counter.
    ///
    /// Ensures that nonces never decrease even if the system clock jumps
    /// backward (e.g. due to NTP synchronisation). If callers need shared
    /// nonces across multiple executors, they can wrap this in an
    /// `Arc<AtomicU64>` externally.
    pub(crate) nonce: AtomicU64,
}

impl OrderExecutor {
    /// Create a new executor, loading the asset meta cache from the exchange.
    pub async fn new(
        client: Arc<dyn HttpTransport>,
        signer: Box<dyn Signer>,
        address: String,
    ) -> Result<Self, HlError> {
        let meta_cache = AssetMetaCache::load(client.as_ref()).await?;
        Ok(Self {
            client,
            signer,
            address,
            meta_cache,
            nonce: AtomicU64::new(0),
        })
    }

    /// Convenience constructor that wraps a [`HyperliquidClient`] in an `Arc`.
    pub async fn from_client(
        client: hl_client::HyperliquidClient,
        signer: Box<dyn Signer>,
        address: String,
    ) -> Result<Self, HlError> {
        Self::new(Arc::new(client), signer, address).await
    }

    /// Create an executor with a pre-built meta cache (avoids the network call).
    pub fn with_meta_cache(
        client: Arc<dyn HttpTransport>,
        signer: Box<dyn Signer>,
        address: String,
        meta_cache: AssetMetaCache,
    ) -> Self {
        Self {
            client,
            signer,
            address,
            meta_cache,
            nonce: AtomicU64::new(0),
        }
    }

    /// Convenience constructor with meta cache that wraps a [`HyperliquidClient`] in an `Arc`.
    pub fn from_client_with_meta_cache(
        client: hl_client::HyperliquidClient,
        signer: Box<dyn Signer>,
        address: String,
        meta_cache: AssetMetaCache,
    ) -> Self {
        Self::with_meta_cache(Arc::new(client), signer, address, meta_cache)
    }

    /// Generate a monotonically increasing nonce based on the current time in
    /// milliseconds since the UNIX epoch.
    pub(crate) fn next_nonce(&self) -> u64 {
        loop {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before UNIX epoch")
                .as_millis() as u64;
            let prev = self.nonce.load(Ordering::Acquire);
            let next = std::cmp::max(now_ms, prev + 1);
            match self
                .nonce
                .compare_exchange_weak(prev, next, Ordering::Release, Ordering::Acquire)
            {
                Ok(_) => return next,
                Err(_) => continue,
            }
        }
    }

    /// Sign and post an action to the exchange, returning the raw JSON response.
    ///
    /// This is the shared nonce-sign-post-status-check pipeline used by
    /// `place_order`, `cancel_order`, `place_trigger_order`, etc.
    pub(crate) async fn send_signed_action(
        &self,
        action: serde_json::Value,
        vault: Option<&str>,
    ) -> Result<serde_json::Value, HlError> {
        let nonce = self.next_nonce();
        let signature = sign_l1_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            nonce,
            self.client.is_mainnet(),
            vault,
        )?;
        let result = self
            .client
            .post_action(action, &signature, nonce, vault)
            .await?;

        let api_status = result
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");
        if api_status != "ok" {
            return Err(HlError::Rejected {
                reason: format!("Exchange rejected action: {}", result),
            });
        }

        Ok(result)
    }

    /// Normalize a symbol string and look up its asset index in the meta cache.
    pub(crate) fn resolve_asset(&self, symbol: &str) -> Result<u32, HlError> {
        let coin = normalize_symbol(symbol);
        self.meta_cache
            .asset_index_normalized(&coin)
            .ok_or_else(|| {
                HlError::Parse(format!("Asset '{}' not found in exchange universe", symbol))
            })
    }

    /// Borrow the underlying HTTP transport.
    pub fn client(&self) -> &dyn HttpTransport {
        self.client.as_ref()
    }

    /// Return the wallet address used for signing.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Borrow the asset meta cache.
    pub fn meta_cache(&self) -> &AssetMetaCache {
        &self.meta_cache
    }
}
