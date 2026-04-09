#![cfg(feature = "live-test")]

use hl_client::HyperliquidClient;
use hl_market::MarketData;
use hl_types::Decimal;

fn market() -> MarketData {
    let client = HyperliquidClient::testnet().unwrap();
    MarketData::new(client)
}

#[tokio::test]
async fn live_candles() {
    let m = market();
    let candles = m.candles("BTC", "1h", 10).await;
    assert!(candles.is_ok(), "candles failed: {:?}", candles.err());
    let candles = candles.unwrap();
    assert!(!candles.is_empty(), "should return candles");
    assert!(candles.len() <= 10, "should respect limit");
    // Verify candle fields are reasonable
    let c = &candles[0];
    assert!(c.open > Decimal::ZERO);
    assert!(c.high >= c.low);
    assert!(c.volume >= Decimal::ZERO);
}

#[tokio::test]
async fn live_orderbook() {
    let m = market();
    let book = m.orderbook("BTC").await;
    assert!(book.is_ok(), "orderbook failed: {:?}", book.err());
    let book = book.unwrap();
    assert!(!book.bids.is_empty(), "should have bids");
    assert!(!book.asks.is_empty(), "should have asks");
    // Best bid should be less than best ask
    let best_bid = book.bids[0].0;
    let best_ask = book.asks[0].0;
    assert!(
        best_bid < best_ask,
        "bid {} should be < ask {}",
        best_bid,
        best_ask
    );
}

#[tokio::test]
async fn live_asset_info() {
    let m = market();
    let assets = m.asset_info().await;
    assert!(assets.is_ok(), "asset_info failed: {:?}", assets.err());
    let assets = assets.unwrap();
    assert!(!assets.is_empty());
    let btc = assets.iter().find(|a| a.coin == "BTC");
    assert!(btc.is_some(), "BTC should be in asset list");
}

#[tokio::test]
async fn live_funding_rates() {
    let m = market();
    let rates = m.funding_rates().await;
    assert!(rates.is_ok(), "funding_rates failed: {:?}", rates.err());
    let rates = rates.unwrap();
    assert!(!rates.is_empty());
}

#[tokio::test]
async fn live_mid_price() {
    let m = market();
    let price = m.mid_price("BTC").await;
    assert!(price.is_ok(), "mid_price failed: {:?}", price.err());
    let price = price.unwrap();
    assert!(price > Decimal::ZERO, "BTC mid price should be positive");
}
