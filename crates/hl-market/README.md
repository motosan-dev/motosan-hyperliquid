# hl-market

> Market data queries for Hyperliquid -- candles, orderbook, funding rates, asset metadata.

## Overview

`hl-market` wraps the Hyperliquid info API into typed Rust methods. All data is returned as strongly-typed structs from `hl-types` (no raw JSON wrangling).

## Usage

```rust
use hl_client::HyperliquidClient;
use hl_market::MarketData;

let client = HyperliquidClient::mainnet()?;
let market = MarketData::new(client);
```

### Candles

```rust
// Fetch the last 50 one-hour candles for BTC
let candles = market.candles("BTC", "1h", 50).await?;
for c in &candles {
    println!("{}: O={} H={} L={} C={} V={}",
        c.timestamp, c.open, c.high, c.low, c.close, c.volume);
}
```

Supported intervals: `1m`, `5m`, `15m`, `1h`, `4h`, `1d`.

### Orderbook

```rust
let book = market.orderbook("ETH").await?;
println!("Best bid: {} @ {}", book.bids[0].1, book.bids[0].0);
println!("Best ask: {} @ {}", book.asks[0].1, book.asks[0].0);
```

### Mid-Price

```rust
let mid = market.mid_price("BTC").await?;
println!("BTC mid-price: {mid}");
```

### Asset Metadata

```rust
let assets = market.asset_info().await?;
for a in &assets {
    println!("{}: id={}, min_size={}, sz_dec={}, px_dec={}",
        a.coin, a.asset_id, a.min_size, a.sz_decimals, a.px_decimals);
}
```

### Funding Rates

```rust
let rates = market.funding_rates().await?;
for r in &rates {
    println!("{}: rate={:.6}, next_funding={}",
        r.coin, r.funding_rate, r.next_funding_time);
}
```

## Coin Normalization

All methods accept flexible coin symbols. Suffixes like `-PERP`, `-USDC`, `-USD` are stripped automatically, so `"BTC"`, `"BTC-PERP"`, and `"BTC-USD"` all work.

## License

MIT
