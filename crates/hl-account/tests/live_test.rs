#![cfg(feature = "live-test")]

use hl_account::Account;
use hl_client::HyperliquidClient;
use hl_types::Decimal;

fn account() -> (Account, String) {
    let key = std::env::var("HYPERLIQUID_TESTNET_KEY")
        .expect("HYPERLIQUID_TESTNET_KEY must be set for live tests");
    let signer = hl_signing::PrivateKeySigner::from_hex(&key).unwrap();
    let address = signer.address().to_string();
    let client = HyperliquidClient::testnet().unwrap();
    (Account::from_client(client), address)
}

#[tokio::test]
async fn live_account_state() {
    let (acc, addr) = account();
    let state = acc.state(&addr).await;
    assert!(state.is_ok(), "account state failed: {:?}", state.err());
    let state = state.unwrap();
    // Equity should be non-negative (even if zero for a new testnet account)
    assert!(state.equity >= Decimal::ZERO);
}

#[tokio::test]
async fn live_positions() {
    let (acc, addr) = account();
    let positions = acc.positions(&addr).await;
    assert!(
        positions.is_ok(),
        "positions query failed: {:?}",
        positions.err()
    );
    // May be empty if no open positions -- that's fine
}

#[tokio::test]
async fn live_fills() {
    let (acc, addr) = account();
    let fills = acc.fills(&addr).await;
    assert!(fills.is_ok(), "fills query failed: {:?}", fills.err());
    // May be empty -- that's fine
}

#[tokio::test]
async fn live_open_orders() {
    let (acc, addr) = account();
    let result = acc.open_orders(&addr).await;
    assert!(result.is_ok(), "open_orders failed: {:?}", result.err());
}

#[tokio::test]
async fn live_historical_orders() {
    let (acc, addr) = account();
    let result = acc.historical_orders(&addr).await;
    assert!(
        result.is_ok(),
        "historical_orders failed: {:?}",
        result.err()
    );
}
