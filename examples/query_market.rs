//! # Query Market Data
//!
//! Demonstrates how to fetch candles, orderbook snapshots, mid-prices,
//! asset metadata, and funding rates from Hyperliquid.
//!
//! ## Running
//!
//! From the `sdks/motosan-hyperliquid/` directory, add an `[[example]]`
//! entry to a crate's `Cargo.toml` or run directly:
//!
//! ```bash
//! cargo run --example query_market
//! ```
//!
//! No private key is needed -- all queries are read-only.

use hl_client::HyperliquidClient;
use hl_market::MarketData;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a mainnet client (read-only, no key needed)
    let client = HyperliquidClient::mainnet()?;
    let market = MarketData::new(client);

    // ── Candles ──────────────────────────────────────────────
    println!("=== BTC 1h Candles (last 5) ===");
    let candles = market.candles("BTC", "1h", 5).await?;
    for c in &candles {
        println!(
            "  ts={} O={:.2} H={:.2} L={:.2} C={:.2} V={:.2}",
            c.timestamp, c.open, c.high, c.low, c.close, c.volume
        );
    }

    // ── Orderbook ────────────────────────────────────────────
    println!("\n=== ETH Orderbook (top 3 levels) ===");
    let book = market.orderbook("ETH").await?;
    for (i, (px, sz)) in book.bids.iter().take(3).enumerate() {
        println!("  Bid {}: {:.2} @ {:.2}", i + 1, sz, px);
    }
    for (i, (px, sz)) in book.asks.iter().take(3).enumerate() {
        println!("  Ask {}: {:.2} @ {:.2}", i + 1, sz, px);
    }

    // ── Mid-Price ────────────────────────────────────────────
    let mid = market.mid_price("BTC").await?;
    println!("\n=== BTC Mid-Price ===");
    println!("  {:.2}", mid);

    // ── Asset Metadata ───────────────────────────────────────
    println!("\n=== Asset Info (first 5) ===");
    let assets = market.asset_info().await?;
    for a in assets.iter().take(5) {
        println!(
            "  {} (id={}): min_size={} sz_dec={} px_dec={}",
            a.coin, a.asset_id, a.min_size, a.sz_decimals, a.px_decimals
        );
    }

    // ── Funding Rates ────────────────────────────────────────
    println!("\n=== Funding Rates (first 5) ===");
    let rates = market.funding_rates().await?;
    for r in rates.iter().take(5) {
        println!(
            "  {}: rate={:.8} next_funding={}",
            r.coin, r.funding_rate, r.next_funding_time
        );
    }

    Ok(())
}
