//! Position reconciliation between caller-provided local state and the
//! exchange.
//!
//! This module fetches the authoritative position state from the Hyperliquid
//! `clearinghouseState` API and computes the diff against a caller-provided
//! list of [`LocalPosition`]s.
//!
//! Unlike the hyper-agent version, this SDK module does **not** write to any
//! database. It returns a [`ReconcileReport`] describing the actions that
//! *should* be taken; the caller decides how to apply them.

use std::collections::HashMap;

use hl_client::HyperliquidClient;
use hl_types::HlError;

/// A position tracked by the caller (e.g. from a local database).
#[derive(Debug, Clone)]
pub struct LocalPosition {
    pub id: String,
    pub coin: String,
    /// `"long"` or `"short"`.
    pub side: String,
    pub size: f64,
}

/// Summary of a single reconciliation action that should be taken.
#[derive(Debug, Clone)]
pub enum ReconcileAction {
    /// A local open position no longer exists on the exchange and should be
    /// closed.
    ClosedStale { id: String, market: String },
    /// A position exists on the exchange but was missing locally.
    AddedMissing {
        market: String,
        side: String,
        size: f64,
        entry_price: f64,
    },
    /// A local position's size or side diverged from the exchange.
    Updated {
        market: String,
        old_size: f64,
        new_size: f64,
        old_side: String,
        new_side: String,
    },
}

/// Result returned after a full reconciliation pass.
#[derive(Debug, Clone)]
pub struct ReconcileReport {
    pub actions: Vec<ReconcileAction>,
    pub exchange_position_count: usize,
    pub local_position_count: usize,
}

/// An exchange position parsed from the clearinghouseState response.
#[derive(Debug, Clone)]
struct ExchangePosition {
    market: String,
    side: String,
    size: f64,
    entry_price: f64,
}

/// Parse the `clearinghouseState` JSON into a list of exchange positions.
fn parse_exchange_positions(resp: &serde_json::Value) -> Vec<ExchangePosition> {
    let mut positions = Vec::new();
    if let Some(asset_positions) = resp["assetPositions"].as_array() {
        for pos in asset_positions {
            let p = &pos["position"];
            let size: f64 = p["szi"]
                .as_str()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0.0);
            if size.abs() < 1e-12 {
                continue;
            }
            let entry_price: f64 = p["entryPx"]
                .as_str()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0.0);
            let coin = match p["coin"].as_str() {
                Some(c) if !c.is_empty() => c,
                _ => {
                    tracing::warn!("Skipping exchange position with missing or empty coin field");
                    continue;
                }
            };
            let market = format!("{}-PERP", coin);
            let side = if size > 0.0 {
                "long".to_string()
            } else {
                "short".to_string()
            };
            positions.push(ExchangePosition {
                market,
                side,
                size: size.abs(),
                entry_price,
            });
        }
    }
    positions
}

/// Reconcile local positions against the exchange state.
///
/// This function:
/// 1. Fetches the current `clearinghouseState` from the exchange.
/// 2. Compares the exchange positions against the caller-provided `local`
///    positions.
/// 3. Reports positions that are stale (local-only), missing (exchange-only),
///    or diverged (size/side mismatch).
///
/// The caller is responsible for applying the actions (e.g. updating a
/// database).
pub async fn reconcile_positions(
    client: &HyperliquidClient,
    address: &str,
    local: &[LocalPosition],
) -> Result<ReconcileReport, HlError> {
    // 1. Fetch exchange state
    let body = serde_json::json!({
        "type": "clearinghouseState",
        "user": address,
    });
    let resp = client.post_info(body).await?;

    let exchange_positions = parse_exchange_positions(&resp);
    let exchange_position_count = exchange_positions.len();
    let local_position_count = local.len();

    // Build exchange lookup by market
    let exchange_by_market: HashMap<String, &ExchangePosition> = exchange_positions
        .iter()
        .map(|ep| (ep.market.clone(), ep))
        .collect();

    let mut actions = Vec::new();

    // Normalize local positions into a market-keyed map.
    // If multiple local positions exist for the same market, mark extras as
    // stale duplicates (Hyperliquid has a one-position-per-market model).
    let mut local_by_market: HashMap<String, &LocalPosition> = HashMap::new();
    for p in local {
        let market = format!("{}-PERP", p.coin.to_uppercase());
        if local_by_market.contains_key(&market) {
            tracing::warn!(
                market = %market,
                duplicate_id = %p.id,
                "Duplicate local position for same market — marking as stale"
            );
            actions.push(ReconcileAction::ClosedStale {
                id: p.id.clone(),
                market: market.clone(),
            });
        } else {
            local_by_market.insert(market, p);
        }
    }

    // 2. Close local positions not on the exchange (liquidated / manually closed)
    for (market, local_pos) in &local_by_market {
        if !exchange_by_market.contains_key(market) {
            tracing::info!(
                id = %local_pos.id,
                market = %market,
                "Reconciliation: local position not found on exchange"
            );
            actions.push(ReconcileAction::ClosedStale {
                id: local_pos.id.clone(),
                market: market.clone(),
            });
        }
    }

    // 3. Add exchange positions missing locally & detect divergences
    for ep in &exchange_positions {
        match local_by_market.get(&ep.market) {
            None => {
                tracing::info!(
                    market = %ep.market,
                    side = %ep.side,
                    size = %ep.size,
                    entry_price = %ep.entry_price,
                    "Reconciliation: exchange position missing locally"
                );
                actions.push(ReconcileAction::AddedMissing {
                    market: ep.market.clone(),
                    side: ep.side.clone(),
                    size: ep.size,
                    entry_price: ep.entry_price,
                });
            }
            Some(local_pos) => {
                // Both exist — check if size or side diverged
                let size_diff = (local_pos.size - ep.size).abs();
                let threshold = ep.size * 0.001; // 0.1% tolerance
                let side_changed = local_pos.side != ep.side;
                if size_diff > threshold || side_changed {
                    tracing::info!(
                        market = %ep.market,
                        old_size = %local_pos.size,
                        new_size = %ep.size,
                        old_side = %local_pos.side,
                        new_side = %ep.side,
                        "Reconciliation: position diverged from exchange"
                    );
                    actions.push(ReconcileAction::Updated {
                        market: ep.market.clone(),
                        old_size: local_pos.size,
                        new_size: ep.size,
                        old_side: local_pos.side.clone(),
                        new_side: ep.side.clone(),
                    });
                }
            }
        }
    }

    Ok(ReconcileReport {
        actions,
        exchange_position_count,
        local_position_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_exchange_positions_basic() {
        let resp = serde_json::json!({
            "marginSummary": { "accountValue": "50000" },
            "assetPositions": [
                {
                    "position": {
                        "coin": "BTC",
                        "szi": "0.5",
                        "entryPx": "60000.0"
                    }
                },
                {
                    "position": {
                        "coin": "ETH",
                        "szi": "-2.0",
                        "entryPx": "3000.0"
                    }
                },
                {
                    "position": {
                        "coin": "DOGE",
                        "szi": "0.0",
                        "entryPx": "0.1"
                    }
                }
            ]
        });

        let positions = parse_exchange_positions(&resp);
        assert_eq!(positions.len(), 2); // DOGE skipped (size = 0)

        let btc = &positions[0];
        assert_eq!(btc.market, "BTC-PERP");
        assert_eq!(btc.side, "long");
        assert!((btc.size - 0.5).abs() < 1e-12);
        assert!((btc.entry_price - 60000.0).abs() < 1e-6);

        let eth = &positions[1];
        assert_eq!(eth.market, "ETH-PERP");
        assert_eq!(eth.side, "short");
        assert!((eth.size - 2.0).abs() < 1e-12);
        assert!((eth.entry_price - 3000.0).abs() < 1e-6);
    }

    #[test]
    fn parse_exchange_positions_empty() {
        let resp = serde_json::json!({
            "marginSummary": { "accountValue": "0" },
            "assetPositions": []
        });
        assert!(parse_exchange_positions(&resp).is_empty());
    }

    #[test]
    fn parse_exchange_positions_missing_field() {
        let resp = serde_json::json!({
            "marginSummary": { "accountValue": "0" }
        });
        assert!(parse_exchange_positions(&resp).is_empty());
    }

    #[test]
    fn side_mismatch_detected_same_size() {
        let local_side = "long";
        let exchange_side = "short";
        let local_size: f64 = 1.0;
        let exchange_size: f64 = 1.0;

        let size_diff = (local_size - exchange_size).abs();
        let threshold = exchange_size * 0.001;
        let side_changed = local_side != exchange_side;

        assert!(size_diff <= threshold, "sizes should be equal");
        assert!(side_changed, "sides should differ");
        assert!(
            size_diff > threshold || side_changed,
            "divergence should be detected when side changes"
        );
    }

    #[test]
    fn no_divergence_when_same_side_and_size() {
        let local_side = "long";
        let exchange_side = "long";
        let local_size: f64 = 1.0;
        let exchange_size: f64 = 1.0;

        let size_diff = (local_size - exchange_size).abs();
        let threshold = exchange_size * 0.001;
        let side_changed = local_side != exchange_side;

        assert!(
            !(size_diff > threshold || side_changed),
            "no divergence when side and size match"
        );
    }

    #[test]
    fn reconcile_action_debug_format() {
        let action = ReconcileAction::ClosedStale {
            id: "test".to_string(),
            market: "BTC-PERP".to_string(),
        };
        let debug = format!("{:?}", action);
        assert!(debug.contains("ClosedStale"));
    }
}
