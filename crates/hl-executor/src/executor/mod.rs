use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rust_decimal::Decimal;

use hl_client::HttpTransport;
use hl_signing::{sign_l1_action, Signer};
use hl_types::{normalize_coin, HlActionResponse, HlError};

use crate::meta_cache::AssetMetaCache;

/// EIP-712 signature chain ID used by Hyperliquid (Arbitrum One, chain ID 42161).
pub(crate) const SIGNATURE_CHAIN_ID: &str = "0xa4b1";

/// Validate that `addr` looks like a valid Ethereum address (`0x` + 40 hex chars).
///
/// Returns `Ok(())` on success, or [`HlError::InvalidAddress`] if the format is wrong.
pub(crate) fn validate_eth_address(addr: &str) -> Result<(), HlError> {
    if !addr.starts_with("0x")
        || addr.len() != 42
        || !addr[2..].chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(HlError::InvalidAddress(format!(
            "expected 0x-prefixed 40-hex-char address, got \"{}\"",
            addr
        )));
    }
    Ok(())
}

/// Agent approval and admin actions.
pub mod admin;
/// Order cancellation (by OID and by CLOID).
pub mod cancel;
/// Leverage configuration.
pub mod leverage;
/// Order modification (atomic amend).
pub mod modify;
/// Order placement (limit, market, trigger).
pub mod orders;
/// Response parsing for exchange actions.
pub mod response;
/// Scaled order placement (laddered entries).
pub mod scale;
/// Spot token order placement.
pub mod spot;
/// Sub-account management.
pub mod sub_account;
/// USDC transfers (deposit, withdraw, internal).
pub mod transfer;
/// TWAP (time-weighted average price) order management.
pub mod twap;

/// Normalize a market symbol to its base coin name.
///
/// Delegates to [`hl_types::normalize_coin`] which strips common suffixes
/// (-PERP, -USDC, -USD) and uppercases the result.
pub(crate) fn normalize_symbol(symbol: &str) -> String {
    normalize_coin(symbol).into_owned()
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

    /// Return the Hyperliquid chain name for EIP-712 signing.
    pub(crate) fn chain_name(&self) -> &'static str {
        if self.client.is_mainnet() {
            "Mainnet"
        } else {
            "Testnet"
        }
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
                HlError::Validation(format!("Asset '{}' not found in exchange universe", symbol))
            })
    }

    /// Normalize a symbol string and look up its spot token index in the meta cache.
    pub(crate) fn resolve_spot_asset(&self, symbol: &str) -> Result<u32, HlError> {
        let token = normalize_symbol(symbol);
        self.meta_cache
            .spot_asset_index_normalized(&token)
            .ok_or_else(|| {
                HlError::Validation(format!(
                    "Spot token '{}' not found in exchange universe",
                    symbol
                ))
            })
    }

    /// Check the API response status and parse into an [`HlActionResponse`].
    ///
    /// User-signed actions (EIP-712) bypass `send_signed_action` and post
    /// directly via `client.post_action`, so they need their own status check.
    pub(crate) fn check_and_parse_response(
        result: serde_json::Value,
        context: &str,
    ) -> Result<HlActionResponse, HlError> {
        let api_status = result
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");
        if api_status != "ok" {
            return Err(HlError::Rejected {
                reason: format!("Exchange rejected {}: {}", context, result),
            });
        }
        serde_json::from_value(result)
            .map_err(|e| HlError::Parse(format!("{} response: {e}", context)))
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
