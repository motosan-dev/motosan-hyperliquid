# hl-types

> Shared domain types for the motosan-hyperliquid SDK — orders, positions, candles, errors, signatures, and parsing helpers.

## Overview

`hl-types` is the foundation crate for the workspace. It defines Rust structs/enums that map to Hyperliquid API data and provides the unified `HlError` type.

This crate has no network dependencies.

## Key Types

### Market Data

- `HlCandle` — OHLCV candle.
- `HlOrderbook` — L2 orderbook snapshot.
- `HlAssetInfo` — asset metadata.
- `HlFundingRate` — current funding rate and next funding time.
- `TradeSide` — trade side used in parsed market/account responses.

### Account

- `HlAccountState` — equity, available margin, positions.
- `HlPosition` — position size, entry, unrealized PnL, leverage, liquidation price.
- `HlFill` — fill price/size/side/fee/closed PnL.
- `HlOpenOrder` — basic open order.
- `HlFrontendOpenOrder` — richer frontend open-order view with trigger / TP-SL metadata and children.
- `HlOrderDetail` / `HlHistoricalOrder` — order status views.
- `HlVaultSummary`, `HlVaultDetails`, `HlUserFees`, `HlUserFundingEntry`, `HlStakingDelegation`, etc.

### Orders

- `OrderWire` — wire format for submitting orders.
- `OrderWireBuilder` — validates positive price/size at `build()`.
- `OrderTypeWire` — limit or trigger order type.
- `Tif` — `Gtc`, `Ioc`, `Alo`.
- `Tpsl` — `Sl`, `Tp`.
- `Side` — `Buy` / `Sell`, with `Side::from_is_buy`.
- `Grouping` — `Na`, `NormalTpsl`, `PositionTpsl` for bulk order grouping / TP-SL brackets.
- `OrderResponse`, `OrderStatus` — parsed execution response.
- `CancelRequest`, `CancelByCloidRequest`, `ModifyRequest` — batch action request structs.

### Signing

- `Signature` — ECDSA signature split into `r`, `s`, and `v`.

### Errors

`HlError` is the unified error enum. Retry helpers:

- `is_retryable()` — true for transport failures, timeouts, WebSocket errors, 429, and HTTP 5xx.
- `retry_after_ms()` — populated for `RateLimited`.

## Usage

```rust
use hl_types::{Grouping, HlError, OrderWire, Tif};
use rust_decimal::Decimal;

let order = OrderWire::limit_buy(0, Decimal::from(90_000), Decimal::new(1, 3))
    .tif(Tif::Gtc)
    .build()?;

let grouping = Grouping::NormalTpsl;
assert_eq!(grouping.as_str(), "normalTpsl");

let err = HlError::RateLimited { retry_after_ms: 1000, message: "slow down".into() };
assert!(err.is_retryable());
assert_eq!(err.retry_after_ms(), Some(1000));
# Ok::<(), HlError>(())
```

## Utility Functions

- `normalize_coin(coin)` — strips `-PERP`, `-USDC`, `-USD` suffixes and uppercases (`"BTC-PERP"` → `"BTC"`).
- `parse_mid_price_from_l2book(value)` — parse mid price from an `l2Book` JSON response.
- `parse_position_szi(value, coin)` — parse signed position size/side for close helpers.

## License

MIT
