//! # Check Account State
//!
//! Demonstrates how to query positions, fills, and vault information
//! for a Hyperliquid account.
//!
//! ## Running
//!
//! ```bash
//! # Set the address to query (any public address works)
//! export HL_ADDRESS="0xYourAddress"
//! cargo run --example check_account
//! ```
//!
//! No private key is needed -- all queries are read-only.

use hl_account::Account;
use hl_client::HyperliquidClient;
use hl_types::Decimal;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = std::env::var("HL_ADDRESS").unwrap_or_else(|_| {
        eprintln!("Set HL_ADDRESS env var to query an account. Using placeholder.");
        "0x0000000000000000000000000000000000000000".to_string()
    });

    let client = HyperliquidClient::mainnet()?;
    let account = Account::from_client(client);

    // ── Account State ────────────────────────────────────────
    println!("=== Account State for {} ===", &address[..10]);
    let state = account.state(&address).await?;
    println!("  Equity:           {}", state.equity);
    println!("  Margin available: {}", state.margin_available);
    println!("  Open positions:   {}", state.positions.len());

    for pos in &state.positions {
        let direction = if pos.size > Decimal::ZERO {
            "LONG"
        } else {
            "SHORT"
        };
        println!(
            "\n  {} {} (size={}, entry={}, pnl={}, lev={}x)",
            direction,
            pos.coin,
            pos.size.abs(),
            pos.entry_px,
            pos.unrealized_pnl,
            pos.leverage
        );
        if let Some(liq) = pos.liquidation_px {
            println!("    Liquidation price: {}", liq);
        }
    }

    // ── Recent Fills ─────────────────────────────────────────
    println!("\n=== Recent Fills (last 10) ===");
    let fills = account.fills(&address).await?;
    for f in fills.iter().take(10) {
        let side = if f.is_buy { "BUY " } else { "SELL" };
        println!(
            "  {} {} {} @ {} (fee={}, pnl={})",
            side, f.sz, f.coin, f.px, f.fee, f.closed_pnl
        );
    }
    if fills.is_empty() {
        println!("  (no fills)");
    }

    // ── Vault Summaries ──────────────────────────────────────
    println!("\n=== Vault Summaries ===");
    let vaults = account.vault_summaries(&address).await?;
    if vaults.is_empty() {
        println!("  (no vaults)");
    } else {
        for (i, v) in vaults.iter().enumerate() {
            println!("  Vault {}: {:?}", i + 1, v);
        }
    }

    Ok(())
}
