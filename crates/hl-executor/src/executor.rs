use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rust_decimal::Decimal;

use hl_client::{HttpTransport, HyperliquidClient};
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
    fallback_price: Decimal,
    fallback_size: Decimal,
) -> Result<(String, Decimal, Decimal), HlError> {
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
                .and_then(|s| Decimal::from_str(s).ok());
            let total_sz = filled
                .get("totalSz")
                .and_then(|s| s.as_str())
                .and_then(|s| Decimal::from_str(s).ok());
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
                Decimal::ZERO,  // Resting order has zero fill
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

/// The fill-size threshold ratio used to distinguish "filled" from "partial".
///
/// If `fill_size >= requested_size * FILL_THRESHOLD` the order is considered
/// fully filled.
const FILL_THRESHOLD: Decimal = Decimal::from_parts(99, 0, 0, false, 2); // 0.99

/// Standalone order executor for the Hyperliquid L1.
///
/// Provides methods to place, cancel, and manage orders without any
/// hyper-agent-specific dependencies (no `OrderSubmitter` trait, no
/// `PositionManager`).
pub struct OrderExecutor {
    client: Arc<dyn HttpTransport>,
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
        client: HyperliquidClient,
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
        client: HyperliquidClient,
        signer: Box<dyn Signer>,
        address: String,
        meta_cache: AssetMetaCache,
    ) -> Self {
        Self::with_meta_cache(Arc::new(client), signer, address, meta_cache)
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

        let fallback_price: Decimal = Decimal::from_str(&order.limit_px).unwrap_or(Decimal::ZERO);
        let fallback_size: Decimal = Decimal::from_str(&order.sz).unwrap_or(Decimal::ZERO);

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
        match &order.order_type {
            OrderTypeWire::Limit(limit) => {
                order_json["t"] = serde_json::json!({ "limit": { "tif": limit.tif.to_string() } });
            }
            OrderTypeWire::Trigger(trigger) => {
                order_json["t"] = serde_json::json!({
                    "trigger": {
                        "triggerPx": trigger.trigger_px,
                        "isMarket": trigger.is_market,
                        "tpsl": trigger.tpsl.to_string(),
                    }
                });
            }
            _ => unreachable!("unknown OrderTypeWire variant"),
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
            return Err(HlError::Rejected {
                reason: format!("Exchange rejected order: {}", result),
            });
        }

        let (order_id, fill_price, fill_size) =
            parse_order_response(&result, fallback_price, fallback_size)?;

        // Determine status
        let status = if fill_size >= fallback_size * FILL_THRESHOLD {
            OrderStatus::Filled
        } else if fill_size > Decimal::ZERO {
            tracing::warn!(
                order_id = %order_id,
                filled = %fill_size,
                requested = %fallback_size,
                "Partial fill detected"
            );
            OrderStatus::Partial
        } else {
            OrderStatus::Open
        };

        Ok(OrderResponse {
            order_id,
            filled_price: if fill_size > Decimal::ZERO {
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
    ) -> Result<HlActionResponse, HlError> {
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
        let resp = self.client.post_action(action, &sig, nonce, vault).await?;
        serde_json::from_value(resp)
            .map_err(|e| HlError::Parse(format!("cancel_order response: {e}")))
    }

    /// Place a trigger order (stop-loss or take-profit) on Hyperliquid.
    ///
    /// `side` indicates the order direction (opposite of position side).
    /// `tpsl` indicates whether this is a stop-loss or take-profit trigger.
    /// The order fires as a market order when the trigger price is hit.
    pub async fn place_trigger_order(
        &self,
        symbol: &str,
        side: Side,
        size: Decimal,
        trigger_price: Decimal,
        tpsl: Tpsl,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        let coin = normalize_symbol(symbol);
        let asset_idx = self.meta_cache.asset_index(&coin).ok_or_else(|| {
            HlError::Parse(format!("Asset '{}' not found in exchange universe", symbol))
        })?;

        let is_buy = side.is_buy();
        let nonce = self.next_nonce();
        let cloid = uuid::Uuid::new_v4().to_string();

        let action = serde_json::json!({
            "type": "order",
            "orders": [{
                "a": asset_idx,
                "b": is_buy,
                "p": trigger_price.to_string(),
                "s": size.to_string(),
                "r": true,
                "t": {
                    "trigger": {
                        "triggerPx": trigger_price.to_string(),
                        "isMarket": true,
                        "tpsl": tpsl.to_string()
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
            return Err(HlError::Rejected {
                reason: format!("Trigger order rejected: {}", result),
            });
        }

        let (order_id, fill_price, fill_size) = parse_order_response(&result, trigger_price, size)?;

        // Trigger orders typically rest unfilled until the trigger fires
        let status = if fill_size < size * FILL_THRESHOLD && fill_size > Decimal::ZERO {
            tracing::warn!(
                order_id = %order_id,
                filled = %fill_size,
                requested = %size,
                "Partial fill detected on trigger order"
            );
            OrderStatus::Partial
        } else if fill_size == Decimal::ZERO {
            OrderStatus::Open
        } else {
            match tpsl {
                Tpsl::Sl => OrderStatus::TriggerSl,
                Tpsl::Tp => OrderStatus::TriggerTp,
            }
        };

        Ok(OrderResponse {
            order_id,
            filled_price: if fill_size > Decimal::ZERO {
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
        amount: Decimal,
    ) -> Result<HlActionResponse, HlError> {
        let action = serde_json::json!({
            "type": "vaultTransfer",
            "vaultAddress": vault,
            "isDeposit": true,
            "usd": amount.to_string(),
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
        let resp = self.client.post_action(action, &sig, nonce, None).await?;
        serde_json::from_value(resp)
            .map_err(|e| HlError::Parse(format!("transfer_to_vault response: {e}")))
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
