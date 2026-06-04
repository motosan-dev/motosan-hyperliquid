# Market Data

```rust
use hl_client::HyperliquidClient;
use hl_market::MarketData;

let client = HyperliquidClient::mainnet()?;
let market = MarketData::from_client(client);
```

Use `MarketData::new(Arc<dyn HttpTransport>)` when sharing a transport.

## Orderbook

```rust
let book = market.orderbook("BTC").await?;
// book.bids / book.asks: Vec<(Decimal, Decimal)> — (price, size)
println!("Best bid: {:?}, best ask: {:?}", book.bids[0], book.asks[0]);
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

Intervals: `"1m"`, `"5m"`, `"15m"`, `"1h"`, `"4h"`, `"1d"`.

## Funding Rates

```rust
let rates = market.funding_rates().await?;
for r in &rates {
    println!("{}: rate={} next={}", r.coin, r.funding_rate, r.next_funding_time);
}
```

## Asset Metadata

```rust
let assets = market.asset_info().await?;
for a in &assets {
    println!("{}: id={} min={} sz_dec={} px_dec={}", a.coin, a.asset_id, a.min_size, a.sz_decimals, a.px_decimals);
}

let spot = market.spot_meta().await?;
```

## Coin Normalization

All methods accept raw symbols. `"BTC-PERP"`, `"BTC-USDC"`, and `"BTC"` normalize to `"BTC"`.
