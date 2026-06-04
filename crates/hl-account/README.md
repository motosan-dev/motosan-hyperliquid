# hl-account

> Typed public account queries for Hyperliquid — positions, fills, open orders, vaults, fees, funding, staking, and agent approvals.

## Overview

`hl-account` wraps Hyperliquid `/info` account endpoints. No private key is needed; queries use public address lookup.

## Usage

```rust
use hl_account::Account;
use hl_client::HyperliquidClient;

let account = Account::from_client(HyperliquidClient::mainnet()?);
let address = "0xYourAddress";
```

Use `Account::new(Arc<dyn HttpTransport>)` when sharing one client/transport across market, account, and executor modules.

## Account State

```rust
let state = account.state(address).await?;
println!("Equity: {}", state.equity);
println!("Margin available: {}", state.margin_available);

for pos in &state.positions {
    println!(
        "{}: size={} entry={} pnl={} lev={}x liq={:?}",
        pos.coin, pos.size, pos.entry_px, pos.unrealized_pnl, pos.leverage, pos.liquidation_px
    );
}
```

Other state helpers:

```rust
let states = account.states(&[address, other_address]).await?;
let positions = account.positions(address).await?;
let spot_balances = account.spot_state(address).await?;
let all_dexs_raw = account.all_dexs_state(address).await?;
```

## Fills

```rust
let fills = account.fills(address).await?;
let fills_by_time = account.fills_by_time(address, start_ms, Some(end_ms), false).await?;
```

`fills_by_time` uses unix epoch milliseconds and can aggregate same-timestamp fills with `aggregate_by_time = true`.

## Orders

```rust
let open = account.open_orders(address).await?;
let frontend = account.frontend_open_orders(address, None).await?;
let detail = account.order_status(address, oid).await?;
let detail_by_cloid = account.order_status_by_cloid(address, cloid).await?;
let historical = account.historical_orders(address).await?;
```

`frontend_open_orders` returns richer trigger / TP-SL metadata, original size, reduce-only flags, and child bracket orders.

## Vaults and Agents

```rust
let vaults = account.vault_summaries(address).await?;
let details = account.vault_details(address, "0xVaultAddress").await?;
let agents = account.extra_agents(address).await?;
```

## Funding, Fees, Staking, Referral

```rust
let funding = account.funding_history("BTC", start_ms, Some(end_ms)).await?;
let user_funding = account.user_funding(address, start_ms, None).await?;
let fees = account.user_fees(address).await?;
let rate_limit = account.rate_limit_status(address).await?;
let staking = account.staking_delegations(address).await?;
let referral = account.referral_state(address).await?;
let active = account.active_asset_data(address, "BTC").await?;
let borrow_lend = account.borrow_lend_state(address).await?;
```

## License

MIT
