use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use hl_client::HyperliquidClient;
use hl_signing::{sign_l1_action, Signer};
use hl_types::*;

use crate::meta_cache::AssetMetaCache;

/// Normalize a market symbol to its base coin name.
///
/// Uses [`hl_types::normalize_coin`] to strip common suffixes (-PERP, -USDC,
/// -USD) and then uppercases the result.
fn normalize_symbol(symbol: &str) -> String {
    normalize_coin(symbol).to_uppercase()
}

/// Parse the order/fill information from a Hyperliquid exchange response.
///
/// Both regular orders and trigger orders return the same response structure
/// under `response.data.statuses[0]`. This helper extracts the order ID,
/// average fill price, and total fill size from either "filled" or "resting"
/// status entries.
fn parse_order_response(
    result: &serde_json::Value,
    fallback_price: f64,
    fallback_size: f64,
) -> Result<(String, f64, f64), HlError> {
    let status_entry = result
        .get("response")
        .and_then(|r| r.get("data"))
        .and_then(|d| d.get("statuses"))
        .and_then(|s| s.as_array())
        .and_then(|a| a.first());

    if let Some(entry) = status_entry {
        if let Some(filled) = entry.get("filled") {
            let oid = filled
                .get("oid")
                .and_then(|o| o.as_u64())
                .map(|o| o.to_string())
                .unwrap_or_else(|| {
                    let fallback = uuid::Uuid::new_v4().to_string();
                    tracing::warn!(
                        fallback_oid = %fallback,
                        "filled status missing oid, using generated UUID"
                    );
                    fallback
                });
            let avg_px = filled
                .get("avgPx")
                .and_then(|p| p.as_str())
                .and_then(|s| s.parse::<f64>().ok());
            let total_sz = filled
                .get("totalSz")
                .and_then(|s| s.as_str())
                .and_then(|s| s.parse::<f64>().ok());
            Ok((
                oid,
                avg_px.unwrap_or(fallback_price),
                total_sz.unwrap_or(fallback_size),
            ))
        } else if let Some(resting) = entry.get("resting") {
            let oid = resting
                .get("oid")
                .and_then(|o| o.as_u64())
                .map(|o| o.to_string())
                .unwrap_or_else(|| {
                    let fallback = uuid::Uuid::new_v4().to_string();
                    tracing::warn!(
                        fallback_oid = %fallback,
                        "resting status missing oid, using generated UUID"
                    );
                    fallback
                });
            Ok((
                oid,
                fallback_price, // Not filled yet, use fallback price
                0.0,            // Resting order has zero fill
            ))
        } else {
            Err(HlError::Parse(format!(
                "unrecognized order status format: {}",
                entry
            )))
        }
    } else {
        Err(HlError::Parse(
            "exchange returned ok but statuses array is empty".into(),
        ))
    }
}

/// Standalone order executor for the Hyperliquid L1.
///
/// Provides methods to place, cancel, and manage orders without any
/// hyper-agent-specific dependencies (no `OrderSubmitter` trait, no
/// `PositionManager`).
pub struct OrderExecutor {
    client: HyperliquidClient,
    signer: Box<dyn Signer>,
    address: String,
    meta_cache: AssetMetaCache,
    /// Per-instance monotonically increasing nonce counter.
    ///
    /// Ensures that nonces never decrease even if the system clock jumps
    /// backward (e.g. due to NTP synchronisation). If callers need shared
    /// nonces across multiple executors, they can wrap this in an
    /// `Arc<AtomicU64>` externally.
    nonce: AtomicU64,
}

impl OrderExecutor {
    /// Create a new executor, loading the asset meta cache from the exchange.
    pub async fn new(
        client: HyperliquidClient,
        signer: Box<dyn Signer>,
        address: String,
    ) -> Result<Self, HlError> {
        let meta_cache = AssetMetaCache::load(&client).await?;
        Ok(Self {
            client,
            signer,
            address,
            meta_cache,
            nonce: AtomicU64::new(0),
        })
    }

    /// Create an executor with a pre-built meta cache (avoids the network call).
    pub fn with_meta_cache(
        client: HyperliquidClient,
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

    /// Generate a monotonically increasing nonce based on the current time in
    /// milliseconds since the UNIX epoch.
    fn next_nonce(&self) -> u64 {
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

    /// Place an order on the Hyperliquid L1.
    ///
    /// The `OrderWire` must already have the asset index, price, size, order
    /// type, etc. fully populated. This method constructs the action JSON,
    /// signs it, submits it, and parses the response.
    pub async fn place_order(
        &self,
        order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let nonce = self.next_nonce();

        let fallback_price: f64 = order.limit_px.parse().unwrap_or(0.0);
        let fallback_size: f64 = order.sz.parse().unwrap_or(0.0);

        // Build the wire-format order object
        let mut order_json = serde_json::json!({
            "a": order.asset,
            "b": order.is_buy,
            "p": order.limit_px,
            "s": order.sz,
            "r": order.reduce_only,
            "t": {},
        });

        // Set order type
        if let Some(ref limit) = order.order_type.limit {
            order_json["t"] = serde_json::json!({ "limit": { "tif": limit.tif } });
        } else if let Some(ref trigger) = order.order_type.trigger {
            order_json["t"] = serde_json::json!({
                "trigger": {
                    "triggerPx": trigger.trigger_px,
                    "isMarket": trigger.is_market,
                    "tpsl": trigger.tpsl,
                }
            });
        }

        // Set cloid if present
        if let Some(ref cloid) = order.cloid {
            order_json["c"] = serde_json::json!(cloid);
        }

        let action = serde_json::json!({
            "type": "order",
            "orders": [order_json],
            "grouping": "na"
        });

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
            return Err(HlError::Api {
                status: 400,
                body: format!("Exchange rejected order: {}", result),
            });
        }

        let (order_id, fill_price, fill_size) =
            parse_order_response(&result, fallback_price, fallback_size)?;

        // Determine status string
        let status = if fill_size >= fallback_size * 0.99 {
            "filled".to_string()
        } else if fill_size > 0.0 {
            tracing::warn!(
                order_id = %order_id,
                filled = fill_size,
                requested = fallback_size,
                "Partial fill detected"
            );
            "partial".to_string()
        } else {
            "open".to_string()
        };

        Ok(OrderResponse {
            order_id,
            filled_price: if fill_size > 0.0 {
                Some(fill_price)
            } else {
                None
            },
            filled_size: fill_size,
            requested_size: fallback_size,
            status,
        })
    }

    /// Cancel an order by asset index and exchange order ID.
    pub async fn cancel_order(
        &self,
        asset: u32,
        oid: u64,
        vault: Option<&str>,
    ) -> Result<serde_json::Value, HlError> {
        let action = serde_json::json!({
            "type": "cancel",
            "cancels": [{"a": asset, "o": oid}]
        });
        let nonce = self.next_nonce();
        let sig = sign_l1_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            nonce,
            self.client.is_mainnet(),
            vault,
        )?;
        self.client.post_action(action, &sig, nonce, vault).await
    }

    /// Place a trigger order (stop-loss or take-profit) on Hyperliquid.
    ///
    /// `side` should be `"buy"` or `"sell"` (opposite of position side).
    /// `tpsl` should be `"sl"` for stop-loss or `"tp"` for take-profit.
    /// The order fires as a market order when the trigger price is hit.
    pub async fn place_trigger_order(
        &self,
        symbol: &str,
        side: &str,
        size: f64,
        trigger_price: f64,
        tpsl: &str,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let coin = normalize_symbol(symbol);
        let asset_idx = self.meta_cache.asset_index(&coin).ok_or_else(|| {
            HlError::Parse(format!("Asset '{}' not found in exchange universe", symbol))
        })?;

        let is_buy = side == "buy";
        let nonce = self.next_nonce();
        let cloid = uuid::Uuid::new_v4().to_string();

        let action = serde_json::json!({
            "type": "order",
            "orders": [{
                "a": asset_idx,
                "b": is_buy,
                "p": format!("{}", trigger_price),
                "s": format!("{}", size),
                "r": true,
                "t": {
                    "trigger": {
                        "triggerPx": format!("{}", trigger_price),
                        "isMarket": true,
                        "tpsl": tpsl
                    }
                },
                "c": cloid
            }],
            "grouping": "na"
        });

        tracing::debug!(
            symbol = %symbol,
            side = %side,
            size = %size,
            tpsl = %tpsl,
            "Submitting trigger order"
        );

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
            return Err(HlError::Api {
                status: 400,
                body: format!("Trigger order rejected: {}", result),
            });
        }

        let (order_id, fill_price, fill_size) = parse_order_response(&result, trigger_price, size)?;

        // Trigger orders typically rest unfilled until the trigger fires
        let status = if fill_size < size * 0.99 && fill_size > 0.0 {
            tracing::warn!(
                order_id = %order_id,
                filled = fill_size,
                requested = size,
                "Partial fill detected on trigger order"
            );
            "partial".to_string()
        } else if fill_size == 0.0 {
            "open".to_string()
        } else {
            match tpsl {
                "sl" => "trigger_sl".to_string(),
                "tp" => "trigger_tp".to_string(),
                _ => "filled".to_string(),
            }
        };

        Ok(OrderResponse {
            order_id,
            filled_price: if fill_size > 0.0 {
                Some(fill_price)
            } else {
                None
            },
            filled_size: fill_size,
            requested_size: size,
            status,
        })
    }

    /// Transfer USDC into a vault.
    pub async fn transfer_to_vault(
        &self,
        vault: &str,
        amount: f64,
    ) -> Result<serde_json::Value, HlError> {
        let action = serde_json::json!({
            "type": "vaultTransfer",
            "vaultAddress": vault,
            "isDeposit": true,
            "usd": amount,
        });
        let nonce = self.next_nonce();
        let sig = sign_l1_action(
            self.signer.as_ref(),
            &self.address,
            &action,
            nonce,
            self.client.is_mainnet(),
            None,
        )?;
        self.client.post_action(action, &sig, nonce, None).await
    }

    /// Borrow the underlying HTTP client.
    pub fn client(&self) -> &HyperliquidClient {
        &self.client
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
