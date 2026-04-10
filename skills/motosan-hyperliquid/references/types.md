# Domain Types (hl-types)

No network dependencies — pure data types + serde + thiserror.

## Market Data Types

- **`HlCandle`** — `timestamp`, `open`, `high`, `low`, `close`, `volume` (all f64)
- **`HlOrderbook`** — `bids: Vec<(f64, f64)>`, `asks: Vec<(f64, f64)>` (price, size)
- **`HlAssetInfo`** — `symbol`, `asset_id`, `size_decimals`, `price_decimals`, `min_size`
- **`HlFundingRate`** — `coin`, `rate`, `next_funding_time`

## Account Types

- **`HlAccountState`** — `equity`, `margin_available`, `positions: Vec<HlPosition>`
- **`HlPosition`** — `coin`, `size`, `entry_px`, `unrealized_pnl`, `leverage`, `liquidation_px`
- **`HlFill`** — `coin`, `side`, `size`, `price`, `fee`, `realized_pnl`, `timestamp`

## Order Types

- **`OrderWire`** — `asset`, `is_buy`, `limit_px`, `sz`, `reduce_only`, `order_type`, `cloid`
- **`OrderTypeWire`** — `limit: Option<LimitOrderType>`, `trigger: Option<TriggerOrderType>`
- **`LimitOrderType`** — `tif` (String: `"Gtc"`, `"Ioc"`, `"Alo"`)
- **`TriggerOrderType`** — `trigger_px`, `is_market`, `tpsl` (`"sl"` or `"tp"`)
- **`OrderResponse`** — `order_id`, `status`, fill info

## Signing Types

- **`Signature`** — `r: String`, `s: String`, `v: u8`

## Error Type

**`HlError`** — unified error enum:

| Variant | Fields | Retryable |
|---------|--------|-----------|
| `Http` | `message` | Yes |
| `RateLimited` | `retry_after_ms`, `message` | Yes |
| `Api` | `status`, `message` | 5xx only |
| `Signing` | `message` | No |
| `Serialization` | `message` | No |
| `InvalidAddress` | `message` | No |
| `Parse` | `message` | No |

Helpers: `error.is_retryable()`, `error.retry_after_ms() -> Option<u64>`

## Utility Functions

- **`normalize_coin(coin: &str) -> String`** — strips `-PERP`, `-USDC`, `-USD` suffixes
