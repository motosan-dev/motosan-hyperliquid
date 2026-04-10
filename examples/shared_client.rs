//! # Shared Client via Arc
//!
//! Demonstrates the recommended pattern for sharing a single
//! `HyperliquidClient` across multiple consumer structs (`MarketData`,
//! `Account`, etc.) via `Arc<dyn HttpTransport>`.
//!
//! This avoids creating multiple HTTP connection pools and redundant TLS
//! handshakes -- a single client serves all read-only queries.
//!
//! ## Running
//!
//! ```bash
//! # Optional: set an address to query (any public address works)
//! export HL_ADDRESS="0xYourAddress"
//! cargo run --example shared_client
//! ```
//!
//! No private key is needed -- all queries are read-only.

use std::sync::Arc;

use hl_account::Account;
use hl_client::{HttpTransport, HyperliquidClient};
use hl_market::MarketData;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── Create one client, share it via Arc ──────────────────
    let client = Arc::new(HyperliquidClient::mainnet()?);

    // Upcast to `Arc<dyn HttpTransport>` so both consumers share
    // the same underlying HTTP connection pool.
    let transport: Arc<dyn HttpTransport> = client;

    let market = MarketData::new(transport.clone());
    let account = Account::new(transport); // last use, no clone needed

    // ── Market data queries ─────────────────────────────────
    println!("=== BTC Orderbook (top 3 levels) ===");
    let book = market.orderbook("BTC").await?;
    for (i, (px, sz)) in book.bids.iter().take(3).enumerate() {
        println!("  Bid {}: {} @ {}", i + 1, sz, px);
    }
    for (i, (px, sz)) in book.asks.iter().take(3).enumerate() {
        println!("  Ask {}: {} @ {}", i + 1, sz, px);
    }

    let mid = market.mid_price("BTC").await?;
    println!("\nBTC mid-price: {}", mid);

    // ── Account queries (same HTTP client) ───────────────────
    let address = std::env::var("HL_ADDRESS").unwrap_or_else(|_| {
        eprintln!("Set HL_ADDRESS env var to query an account. Using placeholder.");
        "0x0000000000000000000000000000000000000000".to_string()
    });

    println!("\n=== Account State for {} ===", &address[..10]);
    let state = account.state(&address).await?;
    println!("  Equity:           {}", state.equity);
    println!("  Margin available: {}", state.margin_available);
    println!("  Open positions:   {}", state.positions.len());

    Ok(())
}
