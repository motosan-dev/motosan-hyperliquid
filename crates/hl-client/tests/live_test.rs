use hl_client::HyperliquidClient;

fn testnet_client() -> HyperliquidClient {
    HyperliquidClient::testnet().expect("failed to create testnet client")
}

#[tokio::test]
#[ignore]
async fn live_post_info_meta() {
    let client = testnet_client();
    let resp = client.post_info(serde_json::json!({"type": "meta"})).await;
    assert!(resp.is_ok(), "meta query failed: {:?}", resp.err());
    let meta = resp.unwrap();
    assert!(
        meta["universe"].is_array(),
        "meta should have universe array"
    );
    let universe = meta["universe"].as_array().unwrap();
    assert!(!universe.is_empty(), "universe should not be empty");
    // BTC should be in the universe
    let has_btc = universe.iter().any(|a| a["name"].as_str() == Some("BTC"));
    assert!(has_btc, "BTC should be in the universe");
}

#[tokio::test]
#[ignore]
async fn live_post_info_candle_snapshot() {
    let client = testnet_client();
    let resp = client
        .post_info(serde_json::json!({
            "type": "candleSnapshot",
            "req": { "coin": "BTC", "interval": "1h", "startTime": 0 }
        }))
        .await;
    assert!(resp.is_ok(), "candle snapshot failed: {:?}", resp.err());
    let candles = resp.unwrap();
    assert!(candles.is_array(), "candle response should be array");
}

#[tokio::test]
#[ignore]
async fn live_post_info_l2_book() {
    let client = testnet_client();
    let resp = client
        .post_info(serde_json::json!({
            "type": "l2Book",
            "coin": "BTC"
        }))
        .await;
    assert!(resp.is_ok(), "l2Book query failed: {:?}", resp.err());
    let book = resp.unwrap();
    assert!(book["levels"].is_array(), "book should have levels");
}
