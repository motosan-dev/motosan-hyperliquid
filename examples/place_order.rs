//! # Place an Order
//!
//! Demonstrates how to sign and submit a limit order to Hyperliquid.
//!
//! **WARNING**: This example places a real order on the exchange. Use testnet
//! for experimentation.
//!
//! ## Running
//!
//! ```bash
//! # REQUIRED: Set your private key (use testnet for testing!)
//! export HL_PRIVATE_KEY="0xYourPrivateKey"
//!
//! # Optional: Set to "mainnet" to target mainnet (default: testnet)
//! export HL_NETWORK="testnet"
//!
//! cargo run --example place_order
//! ```

use hl_client::HyperliquidClient;
use hl_executor::OrderExecutor;
use hl_signing::PrivateKeySigner;
use hl_types::{LimitOrderType, OrderTypeWire, OrderWire};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── Configuration ────────────────────────────────────────
    let private_key = std::env::var("HL_PRIVATE_KEY").expect(
        "Set HL_PRIVATE_KEY env var to your private key (use testnet for testing!)",
    );
    let is_mainnet = std::env::var("HL_NETWORK")
        .map(|n| n == "mainnet")
        .unwrap_or(false);

    let network_name = if is_mainnet { "MAINNET" } else { "testnet" };
    println!("=== Placing Order on {} ===\n", network_name);

    // ── Setup ────────────────────────────────────────────────
    let client = HyperliquidClient::new(is_mainnet)?;
    let signer = PrivateKeySigner::from_hex(&private_key)?;
    let address = signer.address().to_string();
    println!("Wallet address: {}", address);

    let executor = OrderExecutor::new(client, Box::new(signer), address).await?;

    // Look up BTC asset index from the meta cache
    let btc_idx = executor
        .meta_cache()
        .asset_index("BTC")
        .expect("BTC not found in exchange universe");
    println!("BTC asset index: {}", btc_idx);

    // ── Build Order ──────────────────────────────────────────
    // A GTC limit buy at a price well below market (unlikely to fill immediately)
    let order = OrderWire {
        asset: btc_idx,
        is_buy: true,
        limit_px: "10000.0".to_string(), // Far below market -- safe for testing
        sz: "0.001".to_string(),
        reduce_only: false,
        order_type: OrderTypeWire {
            limit: Some(LimitOrderType {
                tif: "Gtc".to_string(),
            }),
            trigger: None,
        },
        cloid: Some(HyperliquidClient::generate_cloid()),
    };

    println!(
        "\nSubmitting: {} BTC {} @ {} (tif=Gtc)",
        if order.is_buy { "BUY" } else { "SELL" },
        order.sz,
        order.limit_px
    );

    // ── Submit ───────────────────────────────────────────────
    let response = executor.place_order(order, None).await?;

    println!("\n=== Order Response ===");
    println!("  Order ID:       {}", response.order_id);
    println!("  Status:         {}", response.status);
    println!("  Filled size:    {}", response.filled_size);
    println!("  Requested size: {}", response.requested_size);
    if let Some(px) = response.filled_price {
        println!("  Fill price:     {}", px);
    }

    // ── Cancel (cleanup) ─────────────────────────────────────
    if response.status == "open" {
        println!("\nCancelling resting order...");
        let oid: u64 = response.order_id.parse().unwrap_or(0);
        if oid > 0 {
            let cancel_result = executor.cancel_order(btc_idx, oid, None).await?;
            println!("  Cancel result: {}", cancel_result);
        }
    }

    Ok(())
}
