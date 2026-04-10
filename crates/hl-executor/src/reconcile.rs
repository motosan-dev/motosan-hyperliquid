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
use std::str::FromStr;

use rust_decimal::Decimal;

use hl_client::HyperliquidClient;
use hl_types::{HlError, PositionSide};

/// A small threshold used to detect zero-size (closed) positions.
const ZERO_SIZE_THRESHOLD: Decimal = Decimal::from_parts(1, 0, 0, false, 12); // 1e-12

/// The relative tolerance used when comparing local vs exchange sizes.
///
/// Size differences within 0.1% of the exchange size are not flagged.
const SIZE_TOLERANCE_BPS: Decimal = Decimal::from_parts(1, 0, 0, false, 3); // 0.001

/// A position tracked by the caller (e.g. from a local database).
#[derive(Debug, Clone)]
pub struct LocalPosition {
    pub id: String,
    pub coin: String,
    /// Position side: long or short.
    pub side: PositionSide,
    pub size: Decimal,
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
        side: PositionSide,
        size: Decimal,
        entry_price: Decimal,
    },
    /// A local position's size or side diverged from the exchange.
    Updated {
        market: String,
        old_size: Decimal,
        new_size: Decimal,
        old_side: PositionSide,
        new_side: PositionSide,
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
    side: PositionSide,
    size: Decimal,
    entry_price: Decimal,
}

/// Parse the `clearinghouseState` JSON into a list of exchange positions.
fn parse_exchange_positions(resp: &serde_json::Value) -> Vec<ExchangePosition> {
    let mut positions = Vec::new();
    if let Some(asset_positions) = resp["assetPositions"].as_array() {
        for pos in asset_positions {
            let p = &pos["position"];
            let size: Decimal = p["szi"]
                .as_str()
                .unwrap_or("0")
                .parse()
                .unwrap_or(Decimal::ZERO);
            if size.abs() < ZERO_SIZE_THRESHOLD {
                continue;
            }
            let entry_price: Decimal = p["entryPx"]
                .as_str()
                .and_then(|s| Decimal::from_str(s).ok())
                .unwrap_or(Decimal::ZERO);
            let coin = match p["coin"].as_str() {
                Some(c) if !c.is_empty() => c,
                _ => {
                    tracing::warn!("Skipping exchange position with missing or empty coin field");
                    continue;
                }
            };
            let market = format!("{}-PERP", coin);
            let side = if size > Decimal::ZERO {
                PositionSide::Long
            } else {
                PositionSide::Short
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
        match local_by_market.entry(market.clone()) {
            std::collections::hash_map::Entry::Occupied(_) => {
                tracing::warn!(
                    market = %market,
                    duplicate_id = %p.id,
                    "Duplicate local position for same market — marking as stale"
                );
                actions.push(ReconcileAction::ClosedStale {
                    id: p.id.clone(),
                    market,
                });
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(p);
            }
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
                    side: ep.side,
                    size: ep.size,
                    entry_price: ep.entry_price,
                });
            }
            Some(local_pos) => {
                // Both exist — check if size or side diverged
                let size_diff = (local_pos.size - ep.size).abs();
                let threshold = ep.size * SIZE_TOLERANCE_BPS; // 0.1% tolerance
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
                        old_side: local_pos.side,
                        new_side: ep.side,
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
        assert_eq!(btc.side, PositionSide::Long);
        assert_eq!(btc.size, Decimal::from_str("0.5").unwrap());
        assert_eq!(btc.entry_price, Decimal::from_str("60000.0").unwrap());

        let eth = &positions[1];
        assert_eq!(eth.market, "ETH-PERP");
        assert_eq!(eth.side, PositionSide::Short);
        assert_eq!(eth.size, Decimal::from_str("2.0").unwrap());
        assert_eq!(eth.entry_price, Decimal::from_str("3000.0").unwrap());
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
        let local_side = PositionSide::Long;
        let exchange_side = PositionSide::Short;
        let local_size = Decimal::ONE;
        let exchange_size = Decimal::ONE;

        let size_diff = (local_size - exchange_size).abs();
        let threshold = exchange_size * SIZE_TOLERANCE_BPS;
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
        let local_side = PositionSide::Long;
        let exchange_side = PositionSide::Long;
        let local_size = Decimal::ONE;
        let exchange_size = Decimal::ONE;

        let size_diff = (local_size - exchange_size).abs();
        let threshold = exchange_size * SIZE_TOLERANCE_BPS;
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

    /// Helper that replicates the reconciliation logic from `reconcile_positions`
    /// but works synchronously with pre-parsed exchange positions.
    fn reconcile_local_vs_exchange(
        local: &[LocalPosition],
        exchange_json: &serde_json::Value,
    ) -> Vec<ReconcileAction> {
        let exchange_positions = parse_exchange_positions(exchange_json);

        let exchange_by_market: HashMap<String, &ExchangePosition> = exchange_positions
            .iter()
            .map(|ep| (ep.market.clone(), ep))
            .collect();

        let mut actions = Vec::new();

        let mut local_by_market: HashMap<String, &LocalPosition> = HashMap::new();
        for p in local {
            let market = format!("{}-PERP", p.coin.to_uppercase());
            match local_by_market.entry(market.clone()) {
                std::collections::hash_map::Entry::Occupied(_) => {
                    actions.push(ReconcileAction::ClosedStale {
                        id: p.id.clone(),
                        market,
                    });
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(p);
                }
            }
        }

        for (market, local_pos) in &local_by_market {
            if !exchange_by_market.contains_key(market) {
                actions.push(ReconcileAction::ClosedStale {
                    id: local_pos.id.clone(),
                    market: market.clone(),
                });
            }
        }

        for ep in &exchange_positions {
            match local_by_market.get(&ep.market) {
                None => {
                    actions.push(ReconcileAction::AddedMissing {
                        market: ep.market.clone(),
                        side: ep.side,
                        size: ep.size,
                        entry_price: ep.entry_price,
                    });
                }
                Some(local_pos) => {
                    let size_diff = (local_pos.size - ep.size).abs();
                    let threshold = ep.size * SIZE_TOLERANCE_BPS;
                    let side_changed = local_pos.side != ep.side;
                    if size_diff > threshold || side_changed {
                        actions.push(ReconcileAction::Updated {
                            market: ep.market.clone(),
                            old_size: local_pos.size,
                            new_size: ep.size,
                            old_side: local_pos.side,
                            new_side: ep.side,
                        });
                    }
                }
            }
        }

        actions
    }

    #[test]
    fn reconcile_exchange_has_position_local_does_not() {
        let local: Vec<LocalPosition> = vec![];
        let exchange = serde_json::json!({
            "assetPositions": [{
                "position": {
                    "coin": "BTC",
                    "szi": "0.5",
                    "entryPx": "60000.0"
                }
            }]
        });
        let actions = reconcile_local_vs_exchange(&local, &exchange);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            ReconcileAction::AddedMissing {
                market,
                side,
                size,
                entry_price,
            } => {
                assert_eq!(market, "BTC-PERP");
                assert_eq!(*side, PositionSide::Long);
                assert_eq!(*size, Decimal::from_str("0.5").unwrap());
                assert_eq!(*entry_price, Decimal::from_str("60000.0").unwrap());
            }
            other => panic!("Expected AddedMissing, got {:?}", other),
        }
    }

    #[test]
    fn reconcile_local_has_position_exchange_does_not() {
        let local = vec![LocalPosition {
            id: "pos-1".into(),
            coin: "ETH".into(),
            side: PositionSide::Long,
            size: Decimal::from_str("2.0").unwrap(),
        }];
        let exchange = serde_json::json!({
            "assetPositions": []
        });
        let actions = reconcile_local_vs_exchange(&local, &exchange);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            ReconcileAction::ClosedStale { id, market } => {
                assert_eq!(id, "pos-1");
                assert_eq!(market, "ETH-PERP");
            }
            other => panic!("Expected ClosedStale, got {:?}", other),
        }
    }

    #[test]
    fn reconcile_size_diverged_triggers_updated() {
        let local = vec![LocalPosition {
            id: "pos-2".into(),
            coin: "BTC".into(),
            side: PositionSide::Long,
            size: Decimal::ONE,
        }];
        let exchange = serde_json::json!({
            "assetPositions": [{
                "position": {
                    "coin": "BTC",
                    "szi": "0.5",
                    "entryPx": "60000.0"
                }
            }]
        });
        let actions = reconcile_local_vs_exchange(&local, &exchange);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            ReconcileAction::Updated {
                market,
                old_size,
                new_size,
                old_side,
                new_side,
            } => {
                assert_eq!(market, "BTC-PERP");
                assert_eq!(*old_size, Decimal::ONE);
                assert_eq!(*new_size, Decimal::from_str("0.5").unwrap());
                assert_eq!(*old_side, PositionSide::Long);
                assert_eq!(*new_side, PositionSide::Long);
            }
            other => panic!("Expected Updated, got {:?}", other),
        }
    }

    #[test]
    fn reconcile_side_changed_triggers_updated() {
        let local = vec![LocalPosition {
            id: "pos-3".into(),
            coin: "BTC".into(),
            side: PositionSide::Long,
            size: Decimal::ONE,
        }];
        let exchange = serde_json::json!({
            "assetPositions": [{
                "position": {
                    "coin": "BTC",
                    "szi": "-1.0",
                    "entryPx": "60000.0"
                }
            }]
        });
        let actions = reconcile_local_vs_exchange(&local, &exchange);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            ReconcileAction::Updated {
                old_side, new_side, ..
            } => {
                assert_eq!(*old_side, PositionSide::Long);
                assert_eq!(*new_side, PositionSide::Short);
            }
            other => panic!("Expected Updated, got {:?}", other),
        }
    }

    #[test]
    fn reconcile_matching_positions_no_action() {
        let local = vec![LocalPosition {
            id: "pos-4".into(),
            coin: "BTC".into(),
            side: PositionSide::Long,
            size: Decimal::from_str("0.5").unwrap(),
        }];
        let exchange = serde_json::json!({
            "assetPositions": [{
                "position": {
                    "coin": "BTC",
                    "szi": "0.5",
                    "entryPx": "60000.0"
                }
            }]
        });
        let actions = reconcile_local_vs_exchange(&local, &exchange);
        assert!(actions.is_empty(), "Expected no actions, got {:?}", actions);
    }

    #[test]
    fn reconcile_both_empty_no_action() {
        let local: Vec<LocalPosition> = vec![];
        let exchange = serde_json::json!({
            "assetPositions": []
        });
        let actions = reconcile_local_vs_exchange(&local, &exchange);
        assert!(actions.is_empty());
    }

    #[test]
    fn reconcile_duplicate_local_positions_marked_stale() {
        let local = vec![
            LocalPosition {
                id: "pos-a".into(),
                coin: "BTC".into(),
                side: PositionSide::Long,
                size: Decimal::from_str("0.5").unwrap(),
            },
            LocalPosition {
                id: "pos-b".into(),
                coin: "BTC".into(),
                side: PositionSide::Long,
                size: Decimal::from_str("0.3").unwrap(),
            },
        ];
        let exchange = serde_json::json!({
            "assetPositions": [{
                "position": {
                    "coin": "BTC",
                    "szi": "0.5",
                    "entryPx": "60000.0"
                }
            }]
        });
        let actions = reconcile_local_vs_exchange(&local, &exchange);
        let stale_count = actions
            .iter()
            .filter(|a| matches!(a, ReconcileAction::ClosedStale { .. }))
            .count();
        assert_eq!(
            stale_count, 1,
            "Duplicate local position should be marked stale"
        );
    }

    #[test]
    fn reconcile_small_size_diff_within_tolerance() {
        // Size differs by less than 0.1% -- should not trigger Updated
        let local = vec![LocalPosition {
            id: "pos-5".into(),
            coin: "BTC".into(),
            side: PositionSide::Long,
            size: Decimal::from_str("1.0005").unwrap(), // ~0.05% diff from 1.0
        }];
        let exchange = serde_json::json!({
            "assetPositions": [{
                "position": {
                    "coin": "BTC",
                    "szi": "1.0",
                    "entryPx": "60000.0"
                }
            }]
        });
        let actions = reconcile_local_vs_exchange(&local, &exchange);
        assert!(
            actions.is_empty(),
            "Small size diff within tolerance should produce no action, got {:?}",
            actions
        );
    }
}
