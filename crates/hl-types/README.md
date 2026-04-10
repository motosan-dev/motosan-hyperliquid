# hl-types

> Shared domain types for the motosan-hyperliquid SDK -- orders, positions, candles, errors, and signatures.

## Overview

`hl-types` is the foundation crate that every other crate in the SDK depends on. It defines the Rust structs that map to Hyperliquid's API data model, plus a unified error type.

This crate has **no network dependencies**. It only uses `serde` for serialization and `thiserror` for error derivation.

## Key Types

### Market Data

- **`HlCandle`** -- OHLCV candle with `timestamp`, `open`, `high`, `low`, `close`, `volume`
- **`HlOrderbook`** -- L2 orderbook snapshot with `bids`, `asks` as `Vec<(f64, f64)>`
- **`HlAssetInfo`** -- Static asset metadata (symbol, asset ID, size/price decimals, min size)
- **`HlFundingRate`** -- Current funding rate and next funding time

### Account

- **`HlAccountState`** -- Equity, available margin, and open positions
- **`HlPosition`** -- Single position with size, entry price, PnL, leverage, liquidation price
- **`HlFill`** -- Trade fill with price, size, side, fee, and realized PnL

### Orders

- **`OrderWire`** -- Wire format for submitting orders (asset index, price, size, order type)
- **`OrderTypeWire`** -- Either a limit order (`LimitOrderType`) or trigger order (`TriggerOrderType`)
- **`OrderResponse`** -- Parsed response after order submission (order ID, fill info, status)

### Signing

- **`Signature`** -- ECDSA signature split into `r`, `s` (hex strings) and `v` (recovery byte)

### Errors

- **`HlError`** -- Unified error enum with variants for HTTP, API, signing, parsing, and rate limiting. Includes `is_retryable()` and `retry_after_ms()` helpers.

## Usage

```rust
use hl_types::{HlCandle, HlError, OrderWire, Tif};

// Construct a limit order using the builder
let order = OrderWire::limit_buy(0, "90000.0", "0.001")
    .tif(Tif::Gtc)
    .build();

// Check if an error is retryable
let err = HlError::RateLimited { retry_after_ms: 1000, message: "slow down".into() };
assert!(err.is_retryable());
assert_eq!(err.retry_after_ms(), Some(1000));
```

## Utility Functions

- **`normalize_coin(coin)`** -- Strips `-PERP`, `-USDC`, `-USD` suffixes from a symbol string. `"BTC-PERP"` becomes `"BTC"`.

## License

MIT
