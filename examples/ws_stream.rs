//! WebSocket streaming example — subscribe to L2 orderbook and print typed messages.
//!
//! Run: `cargo run --example ws_stream --features ws`

use hl_client::{HyperliquidWs, WsMessage};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ws = HyperliquidWs::testnet();
    ws.connect().await?;

    ws.subscribe_l2_book("BTC").await?;
    println!("Subscribed to BTC L2 book. Listening for 10 seconds...\n");

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);

    loop {
        tokio::select! {
            msg = ws.next_typed_message() => {
                match msg {
                    Some(Ok(WsMessage::L2Book(data))) => {
                        println!("L2Book update: coin={}, time={}", data.coin, data.time);
                    }
                    Some(Ok(WsMessage::SubscriptionResponse)) => {
                        println!("Subscription confirmed.");
                    }
                    Some(Ok(WsMessage::Unknown(_))) => {}
                    Some(Ok(other)) => {
                        println!("Other: {:?}", other);
                    }
                    Some(Err(e)) => {
                        eprintln!("Error: {e}");
                        break;
                    }
                    None => {
                        println!("Connection closed.");
                        break;
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                println!("\nDone — 10 seconds elapsed.");
                break;
            }
        }
    }
    Ok(())
}
