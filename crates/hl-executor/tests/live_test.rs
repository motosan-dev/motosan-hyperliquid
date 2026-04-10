#![cfg(feature = "live-test")]

use hl_client::HyperliquidClient;
use hl_executor::{AssetMetaCache, OrderExecutor};
use hl_signing::PrivateKeySigner;

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
