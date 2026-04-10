#![cfg(feature = "live-test")]

use hl_client::HyperliquidClient;
use hl_executor::{AssetMetaCache, OrderExecutor};
use hl_signing::PrivateKeySigner;
use rust_decimal::Decimal;
use std::str::FromStr;

fn setup() -> (HyperliquidClient, Box<dyn hl_signing::Signer>, String) {
    let key = std::env::var("HYPERLIQUID_TESTNET_KEY")
        .expect("HYPERLIQUID_TESTNET_KEY must be set for live tests");
    let signer = PrivateKeySigner::from_hex(&key).unwrap();
    let address = signer.address().to_string();
    let client = HyperliquidClient::testnet().unwrap();
    (client, Box::new(signer), address)
}

#[tokio::test]
async fn live_asset_meta_cache_load() {
    let (client, _, _) = setup();
    let cache = AssetMetaCache::load(&client).await;
    assert!(cache.is_ok(), "meta cache load failed: {:?}", cache.err());
    let cache = cache.unwrap();
    assert!(
        cache.asset_index("BTC").is_some(),
        "BTC should have an index"
    );
    assert!(
        cache.sz_decimals("BTC").is_some(),
        "BTC should have sz_decimals"
    );
}

#[tokio::test]
async fn live_order_executor_construction() {
    let (client, signer, address) = setup();
    let executor = OrderExecutor::from_client(client, signer, address).await;
    assert!(
        executor.is_ok(),
        "executor construction failed: {:?}",
        executor.err()
    );
    let executor = executor.unwrap();
    assert!(!executor.address().is_empty());
    assert!(executor.meta_cache().asset_index("BTC").is_some());
}

// NOTE: We do NOT test place_order/cancel_order in automated tests
// because they would modify testnet state. Those should be tested
// manually via the examples/ directory.

#[tokio::test]
async fn live_bulk_order_and_cancel() {
    let (client, signer, address) = setup();
    let executor = OrderExecutor::from_client(client, signer, address)
        .await
        .expect("executor construction failed");

    let btc_idx = executor
        .meta_cache()
        .asset_index("BTC")
        .expect("BTC asset index not found");

    // Place 2 BTC buy orders at $1 (will never fill)
    let order1 = hl_types::OrderWire::limit_buy(
        btc_idx,
        Decimal::from(1),
        Decimal::from_str("0.001").unwrap(),
    )
    .build()
    .unwrap();
    let order2 = hl_types::OrderWire::limit_buy(
        btc_idx,
        Decimal::from(1),
        Decimal::from_str("0.001").unwrap(),
    )
    .build()
    .unwrap();

    let responses = executor
        .bulk_order(vec![order1, order2], None)
        .await
        .expect("bulk_order failed");

    assert_eq!(responses.len(), 2, "expected 2 order responses");
    assert_eq!(responses[0].status, hl_types::OrderStatus::Open);
    assert_eq!(responses[1].status, hl_types::OrderStatus::Open);

    // Bulk cancel both orders
    let cancels = vec![
        hl_types::CancelRequest::new(btc_idx, responses[0].order_id.parse::<u64>().unwrap()),
        hl_types::CancelRequest::new(btc_idx, responses[1].order_id.parse::<u64>().unwrap()),
    ];
    let cancel_resp = executor
        .bulk_cancel(cancels, None)
        .await
        .expect("bulk_cancel failed");
    assert_eq!(cancel_resp.status, "ok");
}

#[tokio::test]
async fn live_cancel_by_cloid() {
    let (client, signer, address) = setup();
    let executor = OrderExecutor::from_client(client, signer, address)
        .await
        .expect("executor construction failed");

    let btc_idx = executor
        .meta_cache()
        .asset_index("BTC")
        .expect("BTC asset index not found");

    let cloid = HyperliquidClient::generate_cloid();

    // Place a BTC buy at $1 with the generated cloid
    let order = hl_types::OrderWire::limit_buy(
        btc_idx,
        Decimal::from(1),
        Decimal::from_str("0.001").unwrap(),
    )
    .cloid(&cloid)
    .build()
    .unwrap();

    let resp = executor
        .place_order(order, None)
        .await
        .expect("place_order failed");
    assert_eq!(resp.status, hl_types::OrderStatus::Open);

    // Cancel using cancel_by_cloid
    let cancel_resp = executor
        .cancel_by_cloid("BTC", &cloid, None)
        .await
        .expect("cancel_by_cloid failed");
    assert_eq!(cancel_resp.status, "ok");
}

#[tokio::test]
async fn live_modify_order() {
    let (client, signer, address) = setup();
    let executor = OrderExecutor::from_client(client, signer, address)
        .await
        .expect("executor construction failed");

    let btc_idx = executor
        .meta_cache()
        .asset_index("BTC")
        .expect("BTC asset index not found");

    // Place a BTC buy at $1
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
    assert_eq!(resp.status, hl_types::OrderStatus::Open);

    let oid: u64 = resp
        .order_id
        .parse()
        .expect("failed to parse order_id to u64");

    // Modify the order: change price to $2, same size
    let new_order = hl_types::OrderWire::limit_buy(
        btc_idx,
        Decimal::from(2),
        Decimal::from_str("0.001").unwrap(),
    )
    .build()
    .unwrap();
    let modify_resp = executor
        .modify_order(oid, new_order, None)
        .await
        .expect("modify_order failed");
    assert_eq!(modify_resp.status, hl_types::OrderStatus::Open);

    // Clean up: cancel the modified order
    let new_oid: u64 = modify_resp
        .order_id
        .parse()
        .expect("failed to parse modified order_id");
    let cancel_resp = executor
        .cancel_order(btc_idx, new_oid, None)
        .await
        .expect("cancel after modify failed");
    assert_eq!(cancel_resp.status, "ok");
}

#[tokio::test]
async fn live_update_leverage() {
    let (client, signer, address) = setup();
    let executor = OrderExecutor::from_client(client, signer, address)
        .await
        .expect("executor construction failed");

    // Set BTC leverage to 5x cross
    let resp = executor
        .update_leverage("BTC", 5, true, None)
        .await
        .expect("update_leverage to 5x failed");
    assert_eq!(resp.status, "ok");

    // Set BTC leverage back to 10x cross
    let resp = executor
        .update_leverage("BTC", 10, true, None)
        .await
        .expect("update_leverage to 10x failed");
    assert_eq!(resp.status, "ok");
}

#[tokio::test]
async fn live_schedule_cancel() {
    let (client, signer, address) = setup();
    let executor = OrderExecutor::from_client(client, signer, address)
        .await
        .expect("executor construction failed");

    // Schedule cancel 1 hour from now
    let future_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 3_600_000;

    let resp = executor.schedule_cancel(Some(future_time), None).await;
    assert!(resp.is_ok(), "schedule_cancel failed: {:?}", resp.err());

    // Unschedule
    let resp2 = executor.schedule_cancel(None, None).await;
    assert!(resp2.is_ok(), "unschedule_cancel failed: {:?}", resp2.err());
}
