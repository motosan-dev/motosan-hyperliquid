# hl-account

> Account state queries for Hyperliquid -- positions, fills, vaults, agent approvals.

## Overview

`hl-account` provides typed access to Hyperliquid's account-related info API endpoints. No private key is needed -- all queries use public address lookup.

## Usage

```rust
use hl_client::HyperliquidClient;
use hl_account::Account;

let client = HyperliquidClient::mainnet()?;
let account = Account::new(client);
let address = "0xYourAddress";
```

### Account State (Equity + Positions)

```rust
let state = account.state(address).await?;
println!("Equity: {}", state.equity);
println!("Margin available: {}", state.margin_available);

for pos in &state.positions {
    println!("{}: size={} entry={} pnl={} lev={}x liq={:?}",
        pos.coin, pos.size, pos.entry_px,
        pos.unrealized_pnl, pos.leverage, pos.liquidation_px);
}
```

### Positions Only

```rust
let positions = account.positions(address).await?;
```

### Trade History (Fills)

```rust
let fills = account.fills(address).await?;
for f in &fills {
    let side = if f.is_buy { "BUY" } else { "SELL" };
    println!("{} {} {} @ {} (fee={}, pnl={})",
        side, f.sz, f.coin, f.px, f.fee, f.closed_pnl);
}
```

### Vault Operations

```rust
// List vaults
let vaults = account.vault_summaries(address).await?;

// Get vault details
let details = account.vault_details(address, "0xVaultAddress").await?;
```

### Agent Approvals

```rust
let agents = account.extra_agents(address).await?;
```

## Parsing Helpers

The crate also exports standalone parsing functions if you need to parse raw JSON responses yourself:

- `parse_account_state(&serde_json::Value)` -- Parse a `clearinghouseState` response
- `parse_fills(&serde_json::Value)` -- Parse a `userFills` response

## License

MIT
