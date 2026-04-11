use hl_client::HyperliquidClient;
use hl_market::MarketData;
use hl_types::{AssetContext, Decimal, HlAssetInfo, HlSpotMeta};

fn market() -> MarketData {
    let client = HyperliquidClient::testnet().unwrap();
    MarketData::from_client(client)
}

#[tokio::test]
#[ignore]
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
#[ignore]
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
#[ignore]
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
#[ignore]
async fn live_funding_rates() {
    let m = market();
    let rates = m.funding_rates().await;
    assert!(rates.is_ok(), "funding_rates failed: {:?}", rates.err());
    let rates = rates.unwrap();
    assert!(!rates.is_empty());
}

#[tokio::test]
#[ignore]
async fn live_mid_price() {
    let m = market();
    let price = m.mid_price("BTC").await;
    assert!(price.is_ok(), "mid_price failed: {:?}", price.err());
    let price = price.unwrap();
    assert!(price > Decimal::ZERO, "BTC mid price should be positive");
}

#[tokio::test]
#[ignore]
async fn live_all_mids() {
    let m = market();
    let mids = m.all_mids().await;
    assert!(mids.is_ok(), "all_mids failed: {:?}", mids.err());
    let mids = mids.unwrap();
    assert!(!mids.is_empty(), "should return at least one mid price");
    assert!(mids.contains_key("BTC"), "BTC should be in all_mids");
}

#[tokio::test]
#[ignore]
async fn live_recent_trades() {
    let m = market();
    let trades = m.recent_trades("BTC").await;
    assert!(trades.is_ok(), "recent_trades failed: {:?}", trades.err());
    // Trades may be empty on testnet, just verify the call succeeded
    let _trades = trades.unwrap();
}

#[tokio::test]
#[ignore]
async fn live_spot_meta() {
    let m = market();
    let meta = m.spot_meta().await;
    assert!(meta.is_ok(), "spot_meta failed: {:?}", meta.err());
    let meta: HlSpotMeta = meta.unwrap();
    assert!(
        !meta.tokens.is_empty(),
        "should return at least one spot token"
    );
}

#[tokio::test]
#[ignore]
async fn live_meta_and_asset_contexts() {
    let m = market();
    let result = m.meta_and_asset_contexts().await;
    assert!(
        result.is_ok(),
        "meta_and_asset_contexts failed: {:?}",
        result.err()
    );
    let (infos, ctxs): (Vec<HlAssetInfo>, Vec<AssetContext>) = result.unwrap();
    assert!(!infos.is_empty(), "should return asset infos");
    assert!(!ctxs.is_empty(), "should return asset contexts");
}

#[tokio::test]
#[ignore]
async fn live_perp_dex_status() {
    let m = market();
    // "hyperliquid" may or may not be a valid dex name on testnet;
    // we just verify the call doesn't panic.
    let result = m.perp_dex_status("hyperliquid").await;
    // Accept either Ok or a known error — just verify no panic
    if let Ok(status) = result {
        assert!(!status.name.is_empty(), "dex name should not be empty");
    }
}

#[tokio::test]
#[ignore]
async fn live_perps_at_oi_cap() {
    let m = market();
    let result = m.perps_at_oi_cap().await;
    assert!(result.is_ok(), "perps_at_oi_cap failed: {:?}", result.err());
    // May be empty — just verify it returns a valid Vec
    let _perps: Vec<String> = result.unwrap();
}
