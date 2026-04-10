# Account Queries

No private key needed — queries use public address only.

```rust
use hl_client::HyperliquidClient;
use hl_account::Account;

let client = HyperliquidClient::mainnet()?;
let account = Account::new(client);
```

## Account State

```rust
let state = account.state("0xYourAddress").await?;
println!("Equity: {}, Margin available: {}", state.equity, state.margin_available);

for pos in &state.positions {
    println!("{}: size={} entry={} pnl={}", pos.coin, pos.size, pos.entry_px, pos.unrealized_pnl);
}
```

## Fills

```rust
let fills = account.fills("0xYourAddress").await?;
for f in &fills {
    println!("{}: {} {} @ {} (fee={})", f.coin, f.side, f.size, f.price, f.fee);
}
```

## Vault Operations

```rust
let vaults = account.vaults("0xYourAddress").await?;
```

## Agent Approvals

```rust
let approvals = account.agent_approvals("0xYourAddress").await?;
```

## Standalone Parsing

For manual JSON handling:

```rust
use hl_account::parse_account_state;

let json_str = /* raw JSON from Hyperliquid API */;
let state = parse_account_state(json_str)?;
```
