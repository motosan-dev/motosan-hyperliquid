# Order Execution

Requires signing — needs a private key.

```rust
use hl_client::HyperliquidClient;
use hl_signing::PrivateKeySigner;
use hl_executor::OrderExecutor;
use hl_types::{OrderWire, OrderTypeWire, LimitOrderType};

let client = HyperliquidClient::mainnet()?;
let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
let address = signer.address().to_string();

let executor = OrderExecutor::new(client, Box::new(signer), address).await?;
```

## Place Limit Order

```rust
let order = OrderWire {
    asset: 0, // BTC index — use asset metadata to look up
    is_buy: true,
    limit_px: "90000.0".to_string(),
    sz: "0.001".to_string(),
    reduce_only: false,
    order_type: OrderTypeWire {
        limit: Some(LimitOrderType { tif: "Gtc".to_string() }),
        trigger: None,
    },
    cloid: Some(HyperliquidClient::generate_cloid()),
};

let response = executor.place_order(order, None).await?;
println!("Order {}: status={}", response.order_id, response.status);
```

### Time-in-Force Options

- `"Gtc"` — Good til cancelled
- `"Ioc"` — Immediate or cancel
- `"Alo"` — Add liquidity only (post-only)

## Place Trigger Order (Stop-Loss / Take-Profit)

```rust
use hl_types::TriggerOrderType;

let order = OrderWire {
    asset: 0,
    is_buy: false,
    limit_px: "85000.0".to_string(),
    sz: "0.001".to_string(),
    reduce_only: true,
    order_type: OrderTypeWire {
        limit: None,
        trigger: Some(TriggerOrderType {
            trigger_px: "86000.0".to_string(),
            is_market: true,
            tpsl: "sl".to_string(), // "sl" or "tp"
        }),
    },
    cloid: None,
};

let response = executor.place_order(order, None).await?;
```

## Cancel Order

```rust
executor.cancel_order(asset_index, order_id).await?;
```

## Vault Parameter

Pass vault address for vault-delegated trading:

```rust
let response = executor.place_order(order, Some("0xVaultAddress")).await?;
```

## Position Reconciliation

```rust
executor.reconcile_positions().await?;
```

## Asset Index Lookup

The executor caches asset metadata on initialization. Use the market data crate to look up symbol → asset index mapping.
