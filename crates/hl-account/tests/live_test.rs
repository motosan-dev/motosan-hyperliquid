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
#[ignore]
async fn live_account_state() {
    let (acc, addr) = account();
    let state = acc.state(&addr).await;
    assert!(state.is_ok(), "account state failed: {:?}", state.err());
    let state = state.unwrap();
    // Equity should be non-negative (even if zero for a new testnet account)
    assert!(state.equity >= Decimal::ZERO);
}

#[tokio::test]
#[ignore]
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
#[ignore]
async fn live_fills() {
    let (acc, addr) = account();
    let fills = acc.fills(&addr).await;
    assert!(fills.is_ok(), "fills query failed: {:?}", fills.err());
    // May be empty -- that's fine
}

#[tokio::test]
#[ignore]
async fn live_open_orders() {
    let (acc, addr) = account();
    let result = acc.open_orders(&addr).await;
    assert!(result.is_ok(), "open_orders failed: {:?}", result.err());
}

#[tokio::test]
#[ignore]
async fn live_historical_orders() {
    let (acc, addr) = account();
    let result = acc.historical_orders(&addr).await;
    assert!(
        result.is_ok(),
        "historical_orders failed: {:?}",
        result.err()
    );
}

#[tokio::test]
#[ignore]
async fn live_vault_summaries() {
    let (acc, addr) = account();
    let result = acc.vault_summaries(&addr).await;
    assert!(result.is_ok(), "vault_summaries failed: {:?}", result.err());
    // May be empty if the account has no vault participation
}

#[tokio::test]
#[ignore]
async fn live_order_status() {
    let (acc, addr) = account();
    // OID 1 almost certainly doesn't exist for a testnet account, so we
    // tolerate both Ok and Err — the important thing is no panic.
    let _result = acc.order_status(&addr, 1).await;
}

#[tokio::test]
#[ignore]
async fn live_referral_state() {
    let (acc, addr) = account();
    // Testnet may not have referral data, so accept Ok or a parse error.
    match acc.referral_state(&addr).await {
        Ok(_state) => {} // great
        Err(e) => {
            // Parse errors are tolerable on testnet; network errors are not.
            let msg = format!("{e:?}");
            assert!(
                msg.contains("Parse") || msg.contains("parse"),
                "unexpected error (not a parse error): {e:?}"
            );
        }
    }
}

#[tokio::test]
#[ignore]
async fn live_staking_delegations() {
    let (acc, addr) = account();
    let result = acc.staking_delegations(&addr).await;
    // The stakingDelegations endpoint may not be available on testnet.
    // Accept both Ok (with possibly empty vec) and Serialization errors.
    match &result {
        Ok(_) => {}                                        // pass
        Err(hl_types::HlError::Serialization { .. }) => {} // endpoint not available on testnet
        Err(e) => panic!("unexpected staking_delegations error: {e:?}"),
    }
}

#[tokio::test]
#[ignore]
async fn live_borrow_lend_state() {
    let (acc, addr) = account();
    let result = acc.borrow_lend_state(&addr).await;
    assert!(
        result.is_ok(),
        "borrow_lend_state failed: {:?}",
        result.err()
    );
    // May be empty if the account has no borrow/lend positions
}
