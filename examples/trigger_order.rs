//! Trigger order example — place a stop-loss and then cancel it.
//!
//! Run: `HYPERLIQUID_TESTNET_KEY=0x... cargo run --example trigger_order`

use std::str::FromStr;
use hl_client::HyperliquidClient;
use hl_executor::OrderExecutor;
use hl_signing::PrivateKeySigner;
use hl_types::{Side, Tpsl};
use rust_decimal::Decimal;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::var("HYPERLIQUID_TESTNET_KEY")
        .expect("Set HYPERLIQUID_TESTNET_KEY to run this example");

    let client = HyperliquidClient::testnet()?;
    let signer = PrivateKeySigner::from_hex(&key)?;
    let address = signer.address().to_string();
    println!("Address: {address}");

    let executor = OrderExecutor::from_client(client, Box::new(signer), address).await?;

    let trigger_price = Decimal::from_str("1000.0")?;
    let size = Decimal::from_str("0.001")?;

    println!("Placing BTC stop-loss at ${trigger_price} (size={size})...");
    let resp = executor
        .place_trigger_order("BTC", Side::Sell, size, trigger_price, Tpsl::Sl, None)
        .await?;
    println!("Order placed: id={}, status={}", resp.order_id, resp.status);

    let btc_idx = executor.meta_cache().asset_index("BTC").unwrap();
    if let Ok(oid) = resp.order_id.parse::<u64>() {
        println!("Cancelling order {oid}...");
        let cancel = executor.cancel_order(btc_idx, oid, None).await?;
        println!("Cancel result: status={}", cancel.status);
    }

    Ok(())
}
