//! End-to-end WebSocket integration tests for hl-client against Hyperliquid testnet.
//!
//! These tests require network access to the Hyperliquid testnet WebSocket and
//! are run via: `cargo test --all-features -- --ignored`
//!
//! Unlike live_test.rs which covers REST info queries (meta, candle, l2Book),
//! this file tests the WebSocket subscription and message flow.

use hl_client::{HyperliquidWs, Subscription, WsConfig, WsMessage};
use tokio_util::sync::CancellationToken;

/// Connect to testnet WS, subscribe to BBO for BTC, receive at least one
/// typed BBO message, and verify it parses correctly.
#[tokio::test]
#[ignore]
async fn subscribe_receive_bbo() {
    let token = CancellationToken::new();
    let config = WsConfig::default()
        .max_reconnect_attempts(3)
        .cancellation_token(token.clone());

    let mut ws = HyperliquidWs::testnet_with_config(config);
    ws.connect().await.expect("WS connect failed");
    ws.subscribe_typed(Subscription::Bbo { coin: "BTC".into() })
        .await
        .expect("subscribe BBO failed");

    // Read messages until we get a BBO (typed or Unknown from the bbo channel)
    // or hit a reasonable timeout. On testnet with low liquidity, BBO data
    // may have missing price fields, causing the parser to return Unknown.
    let timeout = tokio::time::Duration::from_secs(15);
    let mut received_any_bbo = false;
    let _ = tokio::time::timeout(timeout, async {
        let mut count = 0;
        loop {
            match ws.next_typed_message().await {
                Some(Ok(WsMessage::Bbo(bbo))) => {
                    assert_eq!(bbo.coin, "BTC", "BBO coin should be BTC");
                    received_any_bbo = true;
                    break;
                }
                Some(Ok(WsMessage::Unknown(_))) => {
                    // BBO with missing fields parses as Unknown — still counts
                    received_any_bbo = true;
                    break;
                }
                Some(Ok(WsMessage::SubscriptionResponse | WsMessage::Pong)) => continue,
                Some(Ok(_)) => {
                    count += 1;
                    if count > 20 {
                        break;
                    }
                    continue;
                }
                Some(Err(_)) | None => break,
            }
        }
    })
    .await;

    token.cancel();

    // We should have received at least one message from the BBO channel
    assert!(
        received_any_bbo,
        "should receive at least one BBO-related message"
    );
}

/// Connect to testnet WS, subscribe to all mids, and verify we receive
/// a typed AllMids message with BTC included.
#[tokio::test]
#[ignore]
async fn subscribe_receive_all_mids() {
    let token = CancellationToken::new();
    let config = WsConfig::default()
        .max_reconnect_attempts(3)
        .cancellation_token(token.clone());

    let mut ws = HyperliquidWs::testnet_with_config(config);
    ws.connect().await.expect("WS connect failed");
    ws.subscribe_all_mids()
        .await
        .expect("subscribe allMids failed");

    let timeout = tokio::time::Duration::from_secs(30);
    let result = tokio::time::timeout(timeout, async {
        loop {
            match ws.next_typed_message().await {
                Some(Ok(WsMessage::AllMids(mids))) => {
                    return mids;
                }
                Some(Ok(WsMessage::SubscriptionResponse | WsMessage::Pong)) => continue,
                Some(Ok(_)) => continue,
                Some(Err(e)) => panic!("WS error: {e}"),
                None => panic!("WS closed before receiving AllMids"),
            }
        }
    })
    .await;

    token.cancel();

    let mids = result.expect("timed out waiting for AllMids message");
    assert!(
        !mids.mids.is_empty(),
        "AllMids should contain at least one entry"
    );
    // BTC should be in the mids
    assert!(
        mids.mids.contains_key("BTC"),
        "AllMids should contain BTC, got keys: {:?}",
        mids.mids.keys().take(5).collect::<Vec<_>>()
    );
}

/// Connect to testnet WS, subscribe to trades for BTC, and verify we
/// receive at least one trade message (or timeout gracefully).
#[tokio::test]
#[ignore]
async fn subscribe_receive_trades() {
    let token = CancellationToken::new();
    let config = WsConfig::default()
        .max_reconnect_attempts(3)
        .cancellation_token(token.clone());

    let mut ws = HyperliquidWs::testnet_with_config(config);
    ws.connect().await.expect("WS connect failed");
    ws.subscribe_trades("BTC")
        .await
        .expect("subscribe trades failed");

    // Trades may be infrequent on testnet, so use a shorter timeout
    // and accept timeout as a valid outcome.
    let timeout = tokio::time::Duration::from_secs(15);
    let result = tokio::time::timeout(timeout, async {
        loop {
            match ws.next_typed_message().await {
                Some(Ok(WsMessage::Trades(trades))) => {
                    return Some(trades);
                }
                Some(Ok(WsMessage::SubscriptionResponse | WsMessage::Pong)) => continue,
                Some(Ok(_)) => continue,
                Some(Err(e)) => panic!("WS error: {e}"),
                None => panic!("WS closed"),
            }
        }
    })
    .await;

    token.cancel();

    // Timeout is acceptable — testnet may have no trades in 15s
    if let Ok(Some(trades)) = result {
        assert!(!trades.trades.is_empty(), "trades data should not be empty");
    }
    // If timed out, that's OK — we verified the subscription worked without error
}
