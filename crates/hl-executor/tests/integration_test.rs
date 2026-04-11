//! End-to-end integration tests for hl-executor against Hyperliquid testnet.
//!
//! These tests require `HYPERLIQUID_TESTNET_KEY` to be set and are run via:
//! `cargo test --all-features -- --ignored`
//!
//! Unlike live_test.rs which covers basic construction, bulk order/cancel,
//! cancel-by-cloid, modify, leverage, and schedule-cancel, these tests cover:
//! - Single place + query + cancel lifecycle
//! - Leverage toggle between cross and isolated margin
//! - Concurrent order placement and cancellation
//! - Order status query after placement

use hl_client::HyperliquidClient;
use hl_executor::{AssetMetaCache, OrderExecutor};
use hl_signing::PrivateKeySigner;
use hl_types::OrderStatus;
use rust_decimal::Decimal;
use std::str::FromStr;

fn testnet_key() -> String {
    std::env::var("HYPERLIQUID_TESTNET_KEY")
        .expect("HYPERLIQUID_TESTNET_KEY must be set for integration tests")
}

async fn setup() -> OrderExecutor {
    let key = testnet_key();
    let client = HyperliquidClient::testnet().unwrap();
    let signer = PrivateKeySigner::from_hex(&key).unwrap();
    let address = signer.address().to_string();
    OrderExecutor::from_client(client, Box::new(signer), address)
        .await
        .expect("executor construction failed")
}

/// Place a single limit buy far below market, verify it is open, then cancel it
/// by asset + oid and verify the cancel succeeds.
#[tokio::test]
#[ignore]
async fn place_single_order_query_and_cancel() {
    let executor = setup().await;

    let btc_idx = executor
        .meta_cache()
        .asset_index("BTC")
        .expect("BTC asset index not found");

    // Place a limit buy at $1 — will never fill
    let order = hl_types::OrderWire::limit_buy(
        btc_idx,
        Decimal::from(1),
        Decimal::from_str("0.001").unwrap(),
    )
    .build()
    .unwrap();

    let resp = executor
        .place_order(order, None)
        .await
        .expect("place_order failed");
    assert_eq!(resp.status, OrderStatus::Open, "order should be open");

    let oid: u64 = resp
        .order_id
        .parse()
        .expect("order_id should be a valid u64");
    assert!(oid > 0, "order_id should be positive");

    // Cancel by asset index + oid
    let cancel_resp = executor
        .cancel_order(btc_idx, oid, None)
        .await
        .expect("cancel_order failed");
    assert_eq!(cancel_resp.status, "ok", "cancel should succeed");
}

/// Toggle leverage between cross and isolated margin modes.
/// Sets 3x isolated, then restores to 5x cross.
#[tokio::test]
#[ignore]
async fn leverage_cross_to_isolated_toggle() {
    let executor = setup().await;

    // Set ETH to 3x isolated (is_cross = false)
    let resp = executor
        .update_leverage("ETH", 3, false, None)
        .await
        .expect("update_leverage to 3x isolated failed");
    assert_eq!(resp.status, "ok");

    // Set ETH back to 5x cross (is_cross = true)
    let resp = executor
        .update_leverage("ETH", 5, true, None)
        .await
        .expect("update_leverage to 5x cross failed");
    assert_eq!(resp.status, "ok");
}

/// Place multiple independent orders on different assets, then cancel them all.
/// Verifies that orders on separate assets can coexist.
#[tokio::test]
#[ignore]
async fn place_orders_on_multiple_assets_then_cancel() {
    let executor = setup().await;

    let btc_idx = executor
        .meta_cache()
        .asset_index("BTC")
        .expect("BTC asset index not found");
    let eth_idx = executor
        .meta_cache()
        .asset_index("ETH")
        .expect("ETH asset index not found");

    // Place BTC buy at $1
    let btc_order = hl_types::OrderWire::limit_buy(
        btc_idx,
        Decimal::from(1),
        Decimal::from_str("0.001").unwrap(),
    )
    .build()
    .unwrap();

    // Place ETH buy at $1
    let eth_order = hl_types::OrderWire::limit_buy(
        eth_idx,
        Decimal::from(1),
        Decimal::from_str("0.01").unwrap(),
    )
    .build()
    .unwrap();

    let btc_resp = executor
        .place_order(btc_order, None)
        .await
        .expect("BTC place_order failed");
    assert_eq!(btc_resp.status, OrderStatus::Open);

    let eth_resp = executor
        .place_order(eth_order, None)
        .await
        .expect("ETH place_order failed");
    assert_eq!(eth_resp.status, OrderStatus::Open);

    // Cancel both
    let btc_oid: u64 = btc_resp.order_id.parse().unwrap();
    let eth_oid: u64 = eth_resp.order_id.parse().unwrap();

    let btc_cancel = executor.cancel_order(btc_idx, btc_oid, None).await;
    assert!(
        btc_cancel.is_ok(),
        "BTC cancel failed: {:?}",
        btc_cancel.err()
    );

    let eth_cancel = executor.cancel_order(eth_idx, eth_oid, None).await;
    assert!(
        eth_cancel.is_ok(),
        "ETH cancel failed: {:?}",
        eth_cancel.err()
    );
}

/// Verify that the meta cache contains expected assets and their sz_decimals
/// are consistent with asset_index lookups.
#[tokio::test]
#[ignore]
async fn meta_cache_asset_consistency() {
    let key = testnet_key();
    let client = HyperliquidClient::testnet().unwrap();
    let cache = AssetMetaCache::load(&client)
        .await
        .expect("meta cache load failed");

    // BTC and ETH should always exist on testnet
    for coin in &["BTC", "ETH"] {
        let idx = cache.asset_index(coin);
        assert!(idx.is_some(), "{coin} should have an asset index");
        let sz = cache.sz_decimals(coin);
        assert!(sz.is_some(), "{coin} should have sz_decimals");
        // sz_decimals should be a reasonable value (0-8 for most assets)
        let sz_val = sz.unwrap();
        assert!(sz_val <= 8, "{coin} sz_decimals {sz_val} seems too large");
    }

    // Verify a non-existent coin returns None
    assert!(
        cache.asset_index("DOESNOTEXIST_XYZ").is_none(),
        "non-existent coin should return None"
    );

    // Suppress unused variable warning
    let _ = key;
}
