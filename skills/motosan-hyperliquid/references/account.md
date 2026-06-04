# Account Queries

No private key is needed — account queries use a public address.

```rust
use hl_account::Account;
use hl_client::HyperliquidClient;

let account = Account::from_client(HyperliquidClient::mainnet()?);
let address = "0xYourAddress";
```

Use `Account::new(Arc<dyn HttpTransport>)` when sharing one transport across modules.

## Account State

```rust
let state = account.state(address).await?;
println!("Equity: {}, margin available: {}", state.equity, state.margin_available);

for pos in &state.positions {
    println!("{}: size={} entry={} pnl={}", pos.coin, pos.size, pos.entry_px, pos.unrealized_pnl);
}
```

Related helpers:

```rust
let states = account.states(&[address, other_address]).await?;
let positions = account.positions(address).await?;
let spot_balances = account.spot_state(address).await?;
let all_dexs_raw = account.all_dexs_state(address).await?; // HIP-3 multi-DEX raw JSON
```

## Fills

```rust
let fills = account.fills(address).await?;
for f in &fills {
    let side = if f.is_buy { "BUY" } else { "SELL" };
    println!("{} {} {} @ {} (fee={}, pnl={})", side, f.sz, f.coin, f.px, f.fee, f.closed_pnl);
}
```

Time-ranged fills:

```rust
let fills = account
    .fills_by_time(address, start_ms, Some(end_ms), false)
    .await?;

let aggregated = account
    .fills_by_time(address, start_ms, None, true)
    .await?;
```

`start_ms` / `end_ms` are unix epoch milliseconds. `aggregate_by_time = true` mirrors the Python SDK behavior for timestamp-level aggregation.

## Open Orders and Status

```rust
let open = account.open_orders(address).await?;
let detail = account.order_status(address, oid).await?;
```

Frontend order views and CLOID lookup:

```rust
let frontend = account.frontend_open_orders(address, None).await?;
for order in &frontend {
    println!(
        "{} oid={} trigger={} px={} children={}",
        order.coin,
        order.oid,
        order.is_trigger,
        order.trigger_px,
        order.children.len()
    );
}

let by_cloid = account
    .order_status_by_cloid(address, "0x0123456789abcdef0123456789abcdef")
    .await?;
```

`frontend_open_orders(address, dex)` includes trigger conditions, TP/SL metadata, original size, reduce-only flags, and child bracket orders. Pass `None` for the primary perp DEX.

## Funding, Fees, Staking, Referral

```rust
let funding = account.funding_history("BTC", start_ms, Some(end_ms)).await?;
let user_funding = account.user_funding(address, start_ms, None).await?;
let historical = account.historical_orders(address).await?;
let fees = account.user_fees(address).await?;
let rate_limit = account.rate_limit_status(address).await?;
let staking = account.staking_delegations(address).await?;
let referral = account.referral_state(address).await?;
let active = account.active_asset_data(address, "BTC").await?;
let borrow_lend = account.borrow_lend_state(address).await?;
```

## Vaults and Agents

```rust
let vaults = account.vault_summaries(address).await?;
let details = account.vault_details(address, "0xVaultAddress").await?;
let agents = account.extra_agents(address).await?;
```

## Standalone Parsing

The crate contains internal parsing helpers used by the typed methods. Prefer the `Account` API unless you are extending the crate internals.
