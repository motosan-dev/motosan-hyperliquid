# Domain Types (`hl-types`)

No network dependencies — pure data types, serde, decimal parsing, and the unified error type.

## Market Data Types

- `HlCandle` — `timestamp`, `open`, `high`, `low`, `close`, `volume`.
- `HlOrderbook` — `coin`, `bids: Vec<(Decimal, Decimal)>`, `asks: Vec<(Decimal, Decimal)>`, `timestamp`.
- `HlAssetInfo` — `coin`, `asset_id`, `min_size`, `sz_decimals`, `px_decimals`.
- `HlFundingRate` — `coin`, `funding_rate`, `next_funding_time`.
- `HlSpotAssetInfo`, `HlSpotMeta` — spot token metadata.

## Account Types

- `HlAccountState` — `equity`, `margin_available`, `positions`.
- `HlPosition` — `coin`, `size`, `entry_px`, `unrealized_pnl`, `leverage`, `liquidation_px`.
- `HlFill` — `coin`, `px`, `sz`, `is_buy`, `timestamp`, `fee`, `closed_pnl`.
- `HlOpenOrder` — basic open order.
- `HlFrontendOpenOrder` — richer frontend view: trigger flags/conditions, `orig_sz`, `reduce_only`, `is_position_tpsl`, children.
- `HlOrderDetail`, `HlHistoricalOrder` — order status/history.
- `HlVaultSummary`, `HlVaultDetails`, `HlUserFees`, `HlFundingEntry`, `HlUserFundingEntry`, `HlStakingDelegation`, `HlStakingSummary`, `HlStakingReward`, etc.
- `HlStakingDelegation` includes `locked_until_timestamp`; `rewards` defaults to `0` when omitted by the live `delegations` response.

## Order Types

- `OrderWire` — `asset`, `is_buy`, `limit_px`, `sz`, `reduce_only`, `order_type`, `cloid`.
- `OrderWireBuilder` — produced by `OrderWire::limit_buy`, `limit_sell`, `trigger_buy`, `trigger_sell`; `build()` validates positive price/size.
- `OrderTypeWire` — limit or trigger order wire enum.
- `Tif` — `Gtc`, `Ioc`, `Alo`.
- `Tpsl` — `Sl`, `Tp`.
- `Side` — `Buy`, `Sell`, `Side::from_is_buy(bool)`.
- `Grouping` — `Na`, `NormalTpsl`, `PositionTpsl` for bulk-order grouping / OCO brackets.
- `OrderResponse`, `OrderStatus` — parsed submit response.
- `CancelRequest`, `CancelByCloidRequest`, `ModifyRequest` — batch action inputs.

## Signing Types

- `Signature` — `r: String`, `s: String`, `v: u8`.

## Error Type

`HlError` is shared by all crates.

| Variant | Retryable | Notes |
|---------|-----------|-------|
| `Http`, `Timeout`, `WebSocket` | Yes | Transport-level failures. |
| `RateLimited` | Yes | Includes `retry_after_ms`. |
| `Api` | 5xx only | Non-success HTTP status. |
| `Signing` | No | EIP-712 signing failure. |
| `Serialization` | No | JSON/msgpack encoding failure. |
| `InvalidAddress` | No | Bad Ethereum address. |
| `Validation`, `Config` | No | Bad input/config. |
| `Parse` | No | Unexpected response shape. |
| `Rejected` | No | Exchange rejected the action. |

Helpers: `error.is_retryable()`, `error.retry_after_ms() -> Option<u64>`.

## Utility Functions

- `normalize_coin(coin)` — strips `-PERP`, `-USDC`, `-USD` suffixes and uppercases.
- `parse_mid_price_from_l2book(value)` — parse an L2 book mid-price.
- `parse_position_szi(value, coin)` — parse signed position size/side.
