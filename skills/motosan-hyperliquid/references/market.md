# Market Data

```rust
use hl_client::HyperliquidClient;
use hl_market::MarketData;

let client = HyperliquidClient::mainnet()?;
let market = MarketData::new(client);
```

## Orderbook

```rust
let book = market.orderbook("BTC").await?;
// book.bids: Vec<(f64, f64)> — (price, size)
// book.asks: Vec<(f64, f64)>
println!("Best bid: {:?}, Best ask: {:?}", book.bids[0], book.asks[0]);
```

## Mid-Price

```rust
let mid = market.mid_price("BTC").await?;
println!("BTC mid-price: {mid}");
```

## Candles

```rust
let candles = market.candles("ETH", "1h", 10).await?;
for c in &candles {
    println!("{}: O={} H={} L={} C={} V={}", c.timestamp, c.open, c.high, c.low, c.close, c.volume);
}
```

Intervals: `"1m"`, `"5m"`, `"15m"`, `"1h"`, `"4h"`, `"1d"`

## Funding Rates

```rust
let rates = market.funding_rates().await?;
for r in &rates {
    println!("{}: rate={} next={}", r.coin, r.rate, r.next_funding_time);
}
```

## Asset Metadata

```rust
let assets = market.asset_info().await?;
// Vec<HlAssetInfo> — symbol, asset_id, size_decimals, price_decimals, min_size
```

## Coin Normalization

All methods accept raw symbols. `"BTC-PERP"`, `"BTC-USDC"`, `"BTC"` all resolve to `"BTC"`.
