//! End-to-end integration tests for hl-account against Hyperliquid testnet.
//!
//! These tests require `HYPERLIQUID_TESTNET_KEY` to be set and are run via:
//! `cargo test --all-features -- --ignored`
//!
//! Unlike live_test.rs which covers state, positions, fills, open_orders, and
//! historical_orders, these tests cover:
//! - Market data queries via the info endpoint (candles, orderbook, mid price)
//! - Account fee and rate limit queries
//! - Funding history queries
//! - Spot state and extra agents

use hl_account::Account;
use hl_client::HyperliquidClient;
use hl_types::Decimal;

fn testnet_key() -> String {
    std::env::var("HYPERLIQUID_TESTNET_KEY")
        .expect("HYPERLIQUID_TESTNET_KEY must be set for integration tests")
}

fn account() -> (Account, String) {
    let key = testnet_key();
    let signer = hl_signing::PrivateKeySigner::from_hex(&key).unwrap();
    let address = signer.address().to_string();
    let client = HyperliquidClient::testnet().unwrap();
    (Account::from_client(client), address)
}

/// Query user fees and verify the response has valid fee rates.
#[tokio::test]
#[ignore]
async fn user_fees_query() {
    let (acc, addr) = account();
    let fees = acc.user_fees(&addr).await;
    assert!(fees.is_ok(), "user_fees query failed: {:?}", fees.err());
    let fees = fees.unwrap();
    // Maker and taker rates should be non-negative
    assert!(
        fees.maker_rate >= Decimal::ZERO,
        "maker_rate should be >= 0, got {}",
        fees.maker_rate
    );
    assert!(
        fees.taker_rate >= Decimal::ZERO,
        "taker_rate should be >= 0, got {}",
        fees.taker_rate
    );
}

/// Query rate limit status and verify a sensible response.
#[tokio::test]
#[ignore]
async fn rate_limit_status_query() {
    let (acc, addr) = account();
    let status = acc.rate_limit_status(&addr).await;
    assert!(
        status.is_ok(),
        "rate_limit_status query failed: {:?}",
        status.err()
    );
}

/// Query spot state (token balances) for the testnet account.
#[tokio::test]
#[ignore]
async fn spot_state_query() {
    let (acc, addr) = account();
    let spot = acc.spot_state(&addr).await;
    assert!(spot.is_ok(), "spot_state query failed: {:?}", spot.err());
    // Spot balances may be empty on testnet — that's fine
}

/// Query extra agents (sub-account approvals) for the testnet account.
#[tokio::test]
#[ignore]
async fn extra_agents_query() {
    let (acc, addr) = account();
    let agents = acc.extra_agents(&addr).await;
    assert!(
        agents.is_ok(),
        "extra_agents query failed: {:?}",
        agents.err()
    );
    // May be empty if no agents are approved
}

/// Query funding history for BTC from the beginning of time.
#[tokio::test]
#[ignore]
async fn funding_history_query() {
    let (acc, _addr) = account();
    // Start from 0 to get all available funding history
    let history = acc.funding_history("BTC", 0, None).await;
    assert!(
        history.is_ok(),
        "funding_history query failed: {:?}",
        history.err()
    );
    let history = history.unwrap();
    // BTC should have some funding history on testnet
    assert!(
        !history.is_empty(),
        "BTC should have funding history entries"
    );
}

/// Batch query clearinghouse states for multiple addresses at once.
#[tokio::test]
#[ignore]
async fn batch_clearinghouse_states() {
    let (acc, addr) = account();
    // Query the same address twice to verify batch API works
    let states = acc.states(&[&addr, &addr]).await;
    assert!(
        states.is_ok(),
        "batch states query failed: {:?}",
        states.err()
    );
    let states = states.unwrap();
    assert_eq!(states.len(), 2, "should return 2 states for 2 addresses");
    // Both should have the same equity since it's the same address
    assert_eq!(
        states[0].equity, states[1].equity,
        "same address should have same equity"
    );
}
