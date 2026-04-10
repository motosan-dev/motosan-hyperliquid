use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use rust_decimal::Decimal;

use hl_client::{HttpTransport, HyperliquidClient};
use hl_types::{
    normalize_coin, HlAssetInfo, HlCandle, HlError, HlFundingRate, HlOrderbook, HlPerpDexStatus,
    HlSpotAssetInfo, HlSpotMeta, HlTrade, TradeSide,
};

/// Typed interface for Hyperliquid market data queries.
///
/// Wraps an [`HttpTransport`] and provides methods to fetch candles,
/// orderbook snapshots, mid-prices, asset metadata, and funding rates.
pub struct MarketData {
    client: Arc<dyn HttpTransport>,
}

impl MarketData {
    /// Create a new `MarketData` instance wrapping an [`HttpTransport`].
    pub fn new(client: Arc<dyn HttpTransport>) -> Self {
        Self { client }
    }

    /// Convenience constructor that wraps a [`HyperliquidClient`] in an `Arc`.
    pub fn from_client(client: HyperliquidClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// Fetch OHLCV candle snapshots for a given coin, interval, and limit.
    ///
    /// `interval` must be one of: `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `8h`, `12h`, `1d`, `3d`, `1w`, `1M`.
    /// `limit` caps the number of candles returned (most recent).
    #[tracing::instrument(skip(self))]
    pub async fn candles(
        &self,
        coin: &str,
        interval: &str,
        limit: usize,
    ) -> Result<Vec<HlCandle>, HlError> {
        let coin = normalize_coin(coin).to_uppercase();
        let interval_ms = interval_to_ms(interval)?;
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let start_ms = now_ms.saturating_sub((limit as u64) * interval_ms);

        let payload = serde_json::json!({
            "type": "candleSnapshot",
            "req": {
                "coin": coin,
                "interval": interval,
                "startTime": start_ms,
                "endTime": now_ms
            }
        });
        let resp = self.client.post_info(payload).await?;
        parse_candles(&resp, limit)
    }

    /// Fetch the L2 orderbook for a given coin.
    #[tracing::instrument(skip(self))]
    pub async fn orderbook(&self, coin: &str) -> Result<HlOrderbook, HlError> {
        let coin = normalize_coin(coin).to_uppercase();
        let payload = serde_json::json!({ "type": "l2Book", "coin": coin });
        let resp = self.client.post_info(payload).await?;
        parse_orderbook(&resp, &coin)
    }

    /// Fetch static asset metadata for all perpetuals.
    #[tracing::instrument(skip(self))]
    pub async fn asset_info(&self) -> Result<Vec<HlAssetInfo>, HlError> {
        let payload = serde_json::json!({ "type": "metaAndAssetCtxs" });
        let resp = self.client.post_info(payload).await?;
        parse_asset_info(&resp)
    }

    /// Fetch current funding rates for all perpetuals.
    #[tracing::instrument(skip(self))]
    pub async fn funding_rates(&self) -> Result<Vec<HlFundingRate>, HlError> {
        let payload = serde_json::json!({ "type": "metaAndAssetCtxs" });
        let resp = self.client.post_info(payload).await?;
        parse_funding_rates(&resp)
    }

    /// Fetch spot market universe metadata.
    #[tracing::instrument(skip(self))]
    pub async fn spot_meta(&self) -> Result<HlSpotMeta, HlError> {
        let payload = serde_json::json!({ "type": "spotMeta" });
        let resp = self.client.post_info(payload).await?;
        parse_spot_meta(&resp)
    }

    /// Fetch recent trades for a coin.
    #[tracing::instrument(skip(self))]
    pub async fn recent_trades(&self, coin: &str) -> Result<Vec<HlTrade>, HlError> {
        let coin = normalize_coin(coin).to_uppercase();
        let payload = serde_json::json!({ "type": "recentTrades", "coin": coin });
        let resp = self.client.post_info(payload).await?;
        parse_recent_trades(&resp)
    }

    /// Fetch all asset mid prices in a single call.
    #[tracing::instrument(skip(self))]
    pub async fn all_mids(&self) -> Result<HashMap<String, Decimal>, HlError> {
        let payload = serde_json::json!({ "type": "allMids" });
        let resp = self.client.post_info(payload).await?;
        parse_all_mids(&resp)
    }

    /// Compute the mid-price for a coin from its current orderbook.
    ///
    /// Returns `Err` if either the bid or ask side of the book is empty.
    #[tracing::instrument(skip(self))]
    pub async fn mid_price(&self, coin: &str) -> Result<Decimal, HlError> {
        let book = self.orderbook(coin).await?;
        let best_bid = book.bids.first().map(|(p, _)| *p);
        let best_ask = book.asks.first().map(|(p, _)| *p);
        match (best_bid, best_ask) {
            (Some(bid), Some(ask)) => Ok((bid + ask) / Decimal::TWO),
            _ => Err(HlError::Parse(format!(
                "empty orderbook for {coin}, cannot compute mid price"
            ))),
        }
    }

    /// Fetch status of a builder-deployed perpetual DEX (HIP-3).
    #[tracing::instrument(skip(self))]
    pub async fn perp_dex_status(&self, dex_name: &str) -> Result<HlPerpDexStatus, HlError> {
        let payload = serde_json::json!({ "type": "perpDexStatus", "dexName": dex_name });
        let resp = self.client.post_info(payload).await?;
        parse_perp_dex_status(&resp, dex_name)
    }

    /// Fetch list of assets currently at open interest cap.
    #[tracing::instrument(skip(self))]
    pub async fn perps_at_oi_cap(&self) -> Result<Vec<String>, HlError> {
        let payload = serde_json::json!({ "type": "perpsAtOpenInterestCap" });
        let resp = self.client.post_info(payload).await?;
        parse_perps_at_oi_cap(&resp)
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse a JSON value that might be a string-encoded decimal or a number.
///
/// Returns an error if the value is missing, null, or not parseable as `Decimal`.
fn parse_str_decimal(val: Option<&serde_json::Value>, field: &str) -> Result<Decimal, HlError> {
    match val {
        Some(serde_json::Value::String(s)) => Decimal::from_str(s)
            .map_err(|_| parse_err(format!("cannot parse '{field}' value \"{s}\" as Decimal"))),
        Some(serde_json::Value::Number(n)) => {
            // Convert via the string representation to preserve precision
            let s = n.to_string();
            Decimal::from_str(&s)
                .map_err(|_| parse_err(format!("cannot convert '{field}' number to Decimal")))
        }
        Some(v) => Err(parse_err(format!(
            "unexpected type for '{field}': expected string or number, got {v}"
        ))),
        None => Err(parse_err(format!("missing field '{field}'"))),
    }
}

/// Convenience constructor for a local parse error.
fn parse_err(msg: impl Into<String>) -> HlError {
    HlError::Parse(msg.into())
}

/// Parse a `candleSnapshot` response into a `Vec<HlCandle>`.
///
/// Hyperliquid returns an array of objects:
/// `{ "t": epoch_ms, "o": open, "h": high, "l": low, "c": close, "v": volume, ... }`
///
/// The most recent `limit` candles are returned.
pub fn parse_candles(response: &serde_json::Value, limit: usize) -> Result<Vec<HlCandle>, HlError> {
    let arr = response
        .as_array()
        .ok_or_else(|| parse_err("candle response is not an array"))?;

    let mut candles = Vec::with_capacity(arr.len());
    for item in arr {
        let time_ms = item
            .get("t")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| parse_err("missing 't' field in candle entry"))?;

        candles.push(HlCandle::new(
            time_ms,
            parse_str_decimal(item.get("o"), "o")?,
            parse_str_decimal(item.get("h"), "h")?,
            parse_str_decimal(item.get("l"), "l")?,
            parse_str_decimal(item.get("c"), "c")?,
            // Volume of 0 is valid (no trades in interval), so treat missing as 0.
            parse_str_decimal(item.get("v"), "v").unwrap_or(Decimal::ZERO),
        ));
    }

    // Return only the most recent `limit` candles.
    if limit > 0 && candles.len() > limit {
        let start = candles.len() - limit;
        candles = candles[start..].to_vec();
    }

    Ok(candles)
}

/// Parse an `l2Book` response into an `HlOrderbook`.
///
/// The response has a `levels` array with two entries:
///   - `levels[0]` — bid levels, each `{ "px": "<price>", "sz": "<size>", ... }`
///   - `levels[1]` — ask levels, same shape
///
/// Bids are returned highest-price-first; asks lowest-price-first (as received).
pub fn parse_orderbook(response: &serde_json::Value, coin: &str) -> Result<HlOrderbook, HlError> {
    let levels = response
        .get("levels")
        .and_then(|v| v.as_array())
        .ok_or_else(|| parse_err("l2Book response missing 'levels' array"))?;

    if levels.len() < 2 {
        return Err(parse_err("l2Book 'levels' array has fewer than 2 entries"));
    }

    let parse_levels = |arr: &serde_json::Value| -> Result<Vec<(Decimal, Decimal)>, HlError> {
        let entries = arr
            .as_array()
            .ok_or_else(|| parse_err("orderbook level is not an array"))?;
        let mut result = Vec::with_capacity(entries.len());
        for entry in entries {
            let px = parse_str_decimal(entry.get("px"), "px")?;
            // Size of 0 is valid (empty level), but skip zero-price entries.
            let sz = parse_str_decimal(entry.get("sz"), "sz").unwrap_or(Decimal::ZERO);
            if px > Decimal::ZERO {
                result.push((px, sz));
            }
        }
        Ok(result)
    };

    let bids = parse_levels(&levels[0])?;
    let asks = parse_levels(&levels[1])?;

    let timestamp = response
        .get("time")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as u64);

    Ok(HlOrderbook::new(coin.to_string(), bids, asks, timestamp))
}

/// Parse a `metaAndAssetCtxs` response into a list of `HlAssetInfo`.
///
/// The response is a two-element array `[metaObj, [assetCtx, ...]]`.
/// Asset info comes from `metaObj.universe[i]`:
///   `{ "name": "<coin>", "szDecimals": <u32>, "maxLeverage": <u32>, ... }`
///
/// `px_decimals` is derived from the asset context's `markPx` field when available;
/// otherwise it defaults to 2.
pub fn parse_asset_info(response: &serde_json::Value) -> Result<Vec<HlAssetInfo>, HlError> {
    let arr = response
        .as_array()
        .ok_or_else(|| parse_err("metaAndAssetCtxs response is not an array"))?;

    if arr.len() < 2 {
        return Err(parse_err(
            "metaAndAssetCtxs array has fewer than 2 elements",
        ));
    }

    let meta_obj = &arr[0];
    let asset_ctxs = arr[1]
        .as_array()
        .ok_or_else(|| parse_err("metaAndAssetCtxs[1] is not an array"))?;

    let universe = meta_obj
        .get("universe")
        .and_then(|u| u.as_array())
        .ok_or_else(|| parse_err("metaAndAssetCtxs[0].universe is missing or not an array"))?;

    let mut result = Vec::with_capacity(universe.len());
    for (idx, asset) in universe.iter().enumerate() {
        let coin = asset
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let sz_decimals = asset
            .get("szDecimals")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        // Derive price decimals from the mark price decimal places in the context.
        let px_decimals = if idx < asset_ctxs.len() {
            asset_ctxs[idx]
                .get("markPx")
                .and_then(|v| v.as_str())
                .and_then(|s| {
                    s.find('.').map(|dot| {
                        let frac_len = s.len() - dot - 1;
                        frac_len as u32
                    })
                })
                .unwrap_or(2)
        } else {
            2
        };

        // Minimum order size = 1 * 10^(-sz_decimals)
        let min_size = if sz_decimals == 0 {
            Decimal::ONE
        } else {
            Decimal::ONE / Decimal::from(10u64.pow(sz_decimals))
        };

        result.push(HlAssetInfo::new(
            coin,
            idx as u32,
            min_size,
            sz_decimals,
            px_decimals,
        ));
    }

    Ok(result)
}

/// Parse a `metaAndAssetCtxs` response into a list of `HlFundingRate`.
///
/// The response is a two-element array `[metaObj, [assetCtx, ...]]`.
/// Each asset context at index `i` contains:
///   `{ "funding": "<rate>", "nextFundingTime": <epoch_ms>, ... }`
/// The coin name is read from `metaObj.universe[i].name`.
pub fn parse_funding_rates(response: &serde_json::Value) -> Result<Vec<HlFundingRate>, HlError> {
    let arr = response
        .as_array()
        .ok_or_else(|| parse_err("metaAndAssetCtxs response is not an array"))?;

    if arr.len() < 2 {
        return Err(parse_err(
            "metaAndAssetCtxs array has fewer than 2 elements",
        ));
    }

    let meta_obj = &arr[0];
    let asset_ctxs = arr[1]
        .as_array()
        .ok_or_else(|| parse_err("metaAndAssetCtxs[1] is not an array"))?;

    let universe = meta_obj
        .get("universe")
        .and_then(|u| u.as_array())
        .ok_or_else(|| parse_err("metaAndAssetCtxs[0].universe is missing or not an array"))?;

    let mut result = Vec::with_capacity(asset_ctxs.len());
    for (idx, ctx) in asset_ctxs.iter().enumerate() {
        let coin = universe
            .get(idx)
            .and_then(|a| a.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let funding_rate = parse_str_decimal(ctx.get("funding"), "funding")?;

        let next_funding_time = ctx
            .get("nextFundingTime")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        result.push(HlFundingRate::new(coin, funding_rate, next_funding_time));
    }

    Ok(result)
}

/// Parse a `spotMeta` response into an `HlSpotMeta`.
///
/// The response contains a `tokens` array:
///   `{ "tokens": [{ "name": "PURR", "index": 1, "szDecimals": 0, "weiDecimals": 18 }, ...], ... }`
///
/// Extra top-level fields (e.g. `universe`) are ignored.
pub fn parse_spot_meta(response: &serde_json::Value) -> Result<HlSpotMeta, HlError> {
    let tokens_arr = response
        .get("tokens")
        .and_then(|v| v.as_array())
        .ok_or_else(|| parse_err("spotMeta response missing 'tokens' array"))?;

    let mut tokens = Vec::with_capacity(tokens_arr.len());
    for item in tokens_arr {
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| parse_err("missing 'name' in spot token entry"))?
            .to_string();

        let index = item
            .get("index")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| parse_err("missing 'index' in spot token entry"))?
            as u32;

        let sz_decimals = item.get("szDecimals").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

        let wei_decimals = item
            .get("weiDecimals")
            .and_then(|v| v.as_u64())
            .unwrap_or(18) as u32;

        tokens.push(HlSpotAssetInfo::new(name, index, sz_decimals, wei_decimals));
    }

    Ok(HlSpotMeta::new(tokens))
}

/// Parse a `perpDexStatus` JSON response into an [`HlPerpDexStatus`].
///
/// The response is an object:
///   `{ "name": "...", "isActive": true, "numAssets": 5, "totalOi": "1000000.0" }`
///
/// If the `name` field is missing from the response, `dex_name` is used as fallback.
pub fn parse_perp_dex_status(
    response: &serde_json::Value,
    dex_name: &str,
) -> Result<HlPerpDexStatus, HlError> {
    let name = response
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(dex_name)
        .to_string();

    let is_active = response
        .get("isActive")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| parse_err("missing 'isActive' in perpDexStatus"))?;

    let num_assets = response
        .get("numAssets")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| parse_err("missing 'numAssets' in perpDexStatus"))?
        as u32;

    let total_oi = parse_str_decimal(response.get("totalOi"), "totalOi")?;

    Ok(HlPerpDexStatus::new(name, is_active, num_assets, total_oi))
}

/// Parse a `perpsAtOpenInterestCap` JSON response into a list of coin names.
///
/// The response is a JSON array of strings: `["BTC", "ETH", ...]`
pub fn parse_perps_at_oi_cap(response: &serde_json::Value) -> Result<Vec<String>, HlError> {
    let arr = response
        .as_array()
        .ok_or_else(|| parse_err("perpsAtOpenInterestCap response is not an array"))?;

    let mut coins = Vec::with_capacity(arr.len());
    for item in arr {
        let coin = item
            .as_str()
            .ok_or_else(|| parse_err("perpsAtOpenInterestCap entry is not a string"))?;
        coins.push(coin.to_string());
    }

    Ok(coins)
}

/// Parse a `recentTrades` response into a `Vec<HlTrade>`.
///
/// The response is an array of trade objects:
/// `[{"coin":"BTC","side":"B","px":"94000.0","sz":"0.1","time":1700000000000}, ...]`
pub fn parse_recent_trades(response: &serde_json::Value) -> Result<Vec<HlTrade>, HlError> {
    let arr = response
        .as_array()
        .ok_or_else(|| parse_err("recentTrades response is not an array"))?;

    let mut trades = Vec::with_capacity(arr.len());
    for item in arr {
        let coin = item
            .get("coin")
            .and_then(|v| v.as_str())
            .ok_or_else(|| parse_err("missing 'coin' in trade entry"))?
            .to_string();

        let side = match item.get("side").and_then(|v| v.as_str()) {
            Some("B") => TradeSide::Buy,
            Some("A") => TradeSide::Sell,
            Some(other) => return Err(parse_err(format!("unknown trade side: {}", other))),
            None => return Err(parse_err("missing 'side' in trade entry")),
        };

        let px = parse_str_decimal(item.get("px"), "px")?;
        let sz = parse_str_decimal(item.get("sz"), "sz")?;

        let time = item
            .get("time")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| parse_err("missing 'time' in trade entry"))?;

        trades.push(HlTrade::new(coin, side, px, sz, time));
    }

    Ok(trades)
}

/// Parse an `allMids` response into a `HashMap<String, Decimal>`.
///
/// The response is a flat object mapping coin symbols to mid price strings:
/// `{"BTC": "94000.5", "ETH": "3500.0", ...}`
pub fn parse_all_mids(response: &serde_json::Value) -> Result<HashMap<String, Decimal>, HlError> {
    let obj = response
        .as_object()
        .ok_or_else(|| parse_err("allMids response is not an object"))?;

    let mut mids = HashMap::with_capacity(obj.len());
    for (coin, val) in obj {
        let px = parse_str_decimal(Some(val), coin)?;
        mids.insert(coin.clone(), px);
    }

    Ok(mids)
}

/// Convert a candle interval string to its duration in milliseconds.
fn interval_to_ms(interval: &str) -> Result<u64, HlError> {
    match interval {
        "1m" => Ok(60 * 1_000),
        "3m" => Ok(3 * 60 * 1_000),
        "5m" => Ok(5 * 60 * 1_000),
        "15m" => Ok(15 * 60 * 1_000),
        "30m" => Ok(30 * 60 * 1_000),
        "1h" => Ok(60 * 60 * 1_000),
        "2h" => Ok(2 * 60 * 60 * 1_000),
        "4h" => Ok(4 * 60 * 60 * 1_000),
        "8h" => Ok(8 * 60 * 60 * 1_000),
        "12h" => Ok(12 * 60 * 60 * 1_000),
        "1d" => Ok(24 * 60 * 60 * 1_000),
        "3d" => Ok(3 * 24 * 60 * 60 * 1_000),
        "1w" => Ok(7 * 24 * 60 * 60 * 1_000),
        "1M" => Ok(30 * 24 * 60 * 60 * 1_000),
        _ => Err(parse_err(format!(
            "unsupported interval '{}'; valid values: 1m, 3m, 5m, 15m, 30m, 1h, 2h, 4h, 8h, 12h, 1d, 3d, 1w, 1M",
            interval
        ))),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_candles_valid() {
        let json = serde_json::json!([
            {
                "t": 1709985600000_u64,
                "o": "94200.0",
                "h": "95400.0",
                "l": "93800.0",
                "c": "95234.0",
                "v": "12400.5"
            },
            {
                "t": 1709989200000_u64,
                "o": "95234.0",
                "h": "95500.0",
                "l": "94900.0",
                "c": "95100.0",
                "v": "8200.3"
            }
        ]);

        let candles = parse_candles(&json, 10).unwrap();
        assert_eq!(candles.len(), 2);
        assert_eq!(candles[0].open, Decimal::from_str("94200.0").unwrap());
        assert_eq!(candles[0].close, Decimal::from_str("95234.0").unwrap());
        assert_eq!(candles[0].high, Decimal::from_str("95400.0").unwrap());
        assert_eq!(candles[0].low, Decimal::from_str("93800.0").unwrap());
        assert_eq!(candles[0].volume, Decimal::from_str("12400.5").unwrap());
        assert_eq!(candles[0].timestamp, 1709985600000);
    }

    #[test]
    fn test_parse_candles_limit_truncates() {
        let json = serde_json::json!([
            { "t": 1_u64, "o": "1", "h": "1", "l": "1", "c": "1", "v": "1" },
            { "t": 2_u64, "o": "2", "h": "2", "l": "2", "c": "2", "v": "2" },
            { "t": 3_u64, "o": "3", "h": "3", "l": "3", "c": "3", "v": "3" }
        ]);
        let candles = parse_candles(&json, 2).unwrap();
        assert_eq!(candles.len(), 2);
        assert_eq!(candles[0].timestamp, 2);
        assert_eq!(candles[1].timestamp, 3);
    }

    #[test]
    fn test_parse_candles_empty() {
        let json = serde_json::json!([]);
        let candles = parse_candles(&json, 10).unwrap();
        assert!(candles.is_empty());
    }

    #[test]
    fn test_parse_candles_not_array() {
        let json = serde_json::json!({"error": "bad"});
        assert!(parse_candles(&json, 10).is_err());
    }

    #[test]
    fn test_parse_orderbook_valid() {
        let json = serde_json::json!({
            "levels": [
                [
                    {"px": "94000.0", "sz": "0.5", "n": 1},
                    {"px": "93999.0", "sz": "1.0", "n": 2}
                ],
                [
                    {"px": "94001.0", "sz": "0.3", "n": 1},
                    {"px": "94002.0", "sz": "0.8", "n": 1}
                ]
            ],
            "time": 1709985600000_u64
        });

        let book = parse_orderbook(&json, "BTC").unwrap();
        assert_eq!(book.coin, "BTC");
        assert_eq!(book.bids.len(), 2);
        assert_eq!(book.asks.len(), 2);
        assert_eq!(book.bids[0].0, Decimal::from_str("94000.0").unwrap());
        assert_eq!(book.asks[0].0, Decimal::from_str("94001.0").unwrap());
        assert_eq!(book.timestamp, 1709985600000);
    }

    #[test]
    fn test_parse_orderbook_missing_levels() {
        let json = serde_json::json!({"coin": "BTC"});
        assert!(parse_orderbook(&json, "BTC").is_err());
    }

    #[test]
    fn test_parse_asset_info_valid() {
        let json = serde_json::json!([
            {
                "universe": [
                    {"name": "BTC", "szDecimals": 5, "maxLeverage": 50},
                    {"name": "ETH", "szDecimals": 4, "maxLeverage": 25}
                ]
            },
            [
                {"markPx": "94000.00", "funding": "0.0001"},
                {"markPx": "3500.0000", "funding": "0.00005"}
            ]
        ]);

        let infos = parse_asset_info(&json).unwrap();
        assert_eq!(infos.len(), 2);
        assert_eq!(infos[0].coin, "BTC");
        assert_eq!(infos[0].asset_id, 0);
        assert_eq!(infos[0].sz_decimals, 5);
        assert_eq!(infos[0].px_decimals, 2);
        assert_eq!(infos[1].coin, "ETH");
        assert_eq!(infos[1].asset_id, 1);
        assert_eq!(infos[1].px_decimals, 4);
    }

    #[test]
    fn test_parse_str_decimal_string_value() {
        let v = serde_json::json!("123.45");
        assert_eq!(
            parse_str_decimal(Some(&v), "test").unwrap(),
            Decimal::from_str("123.45").unwrap()
        );
    }

    #[test]
    fn test_parse_str_decimal_number_value() {
        let v = serde_json::json!(42.0);
        assert_eq!(
            parse_str_decimal(Some(&v), "test").unwrap(),
            Decimal::from_str("42.0").unwrap()
        );
    }

    #[test]
    fn test_parse_str_decimal_invalid_string() {
        let v = serde_json::json!("not_a_number");
        let err = parse_str_decimal(Some(&v), "price").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("price"),
            "error should mention field name: {msg}"
        );
    }

    #[test]
    fn test_parse_str_decimal_null() {
        let v = serde_json::Value::Null;
        assert!(parse_str_decimal(Some(&v), "test").is_err());
    }

    #[test]
    fn test_parse_str_decimal_missing() {
        assert!(parse_str_decimal(None, "test").is_err());
    }

    #[test]
    fn test_parse_candles_missing_ohlc_field_errors() {
        // Candle entry with missing 'c' (close) field should error.
        let json = serde_json::json!([{
            "t": 1_u64,
            "o": "1.0",
            "h": "2.0",
            "l": "0.5"
            // "c" missing, "v" missing
        }]);
        let err = parse_candles(&json, 10).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("'c'"), "should mention missing field: {msg}");
    }

    #[test]
    fn test_parse_candles_unparseable_price_errors() {
        let json = serde_json::json!([{
            "t": 1_u64,
            "o": "bad",
            "h": "2.0",
            "l": "0.5",
            "c": "1.0",
            "v": "10"
        }]);
        assert!(parse_candles(&json, 10).is_err());
    }

    #[test]
    fn test_parse_candles_missing_volume_defaults_to_zero() {
        let json = serde_json::json!([{
            "t": 1_u64,
            "o": "1.0",
            "h": "2.0",
            "l": "0.5",
            "c": "1.5"
            // "v" missing — should default to 0.0
        }]);
        let candles = parse_candles(&json, 10).unwrap();
        assert_eq!(candles[0].volume, Decimal::ZERO);
    }

    #[test]
    fn test_parse_orderbook_unparseable_price_errors() {
        let json = serde_json::json!({
            "levels": [
                [{"px": "not_a_number", "sz": "1.0"}],
                [{"px": "100.0", "sz": "1.0"}]
            ]
        });
        assert!(parse_orderbook(&json, "BTC").is_err());
    }

    #[test]
    fn test_parse_funding_rates_valid() {
        let json = serde_json::json!([
            {
                "universe": [
                    {"name": "BTC"},
                    {"name": "ETH"}
                ]
            },
            [
                {"funding": "0.0001", "nextFundingTime": 1709989200000_u64},
                {"funding": "-0.00005", "nextFundingTime": 1709989200000_u64}
            ]
        ]);

        let rates = parse_funding_rates(&json).unwrap();
        assert_eq!(rates.len(), 2);
        assert_eq!(rates[0].coin, "BTC");
        assert_eq!(rates[0].funding_rate, Decimal::from_str("0.0001").unwrap());
        assert_eq!(rates[0].next_funding_time, 1709989200000);
        assert_eq!(rates[1].coin, "ETH");
        assert_eq!(
            rates[1].funding_rate,
            Decimal::from_str("-0.00005").unwrap()
        );
    }

    #[test]
    fn test_interval_to_ms_existing() {
        assert_eq!(interval_to_ms("1m").unwrap(), 60 * 1_000);
        assert_eq!(interval_to_ms("5m").unwrap(), 5 * 60 * 1_000);
        assert_eq!(interval_to_ms("15m").unwrap(), 15 * 60 * 1_000);
        assert_eq!(interval_to_ms("1h").unwrap(), 60 * 60 * 1_000);
        assert_eq!(interval_to_ms("4h").unwrap(), 4 * 60 * 60 * 1_000);
        assert_eq!(interval_to_ms("1d").unwrap(), 24 * 60 * 60 * 1_000);
    }

    #[test]
    fn test_interval_to_ms_extended() {
        assert_eq!(interval_to_ms("3m").unwrap(), 3 * 60 * 1_000);
        assert_eq!(interval_to_ms("30m").unwrap(), 30 * 60 * 1_000);
        assert_eq!(interval_to_ms("2h").unwrap(), 2 * 60 * 60 * 1_000);
        assert_eq!(interval_to_ms("8h").unwrap(), 8 * 60 * 60 * 1_000);
        assert_eq!(interval_to_ms("12h").unwrap(), 12 * 60 * 60 * 1_000);
        assert_eq!(interval_to_ms("3d").unwrap(), 3 * 24 * 60 * 60 * 1_000);
        assert_eq!(interval_to_ms("1w").unwrap(), 7 * 24 * 60 * 60 * 1_000);
        assert_eq!(interval_to_ms("1M").unwrap(), 30 * 24 * 60 * 60 * 1_000);
    }

    #[test]
    fn test_interval_to_ms_invalid() {
        let err = interval_to_ms("2m").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("unsupported interval"), "should error: {msg}");
    }

    #[test]
    fn test_parse_recent_trades_valid() {
        let json = serde_json::json!([
            {
                "coin": "BTC",
                "side": "B",
                "px": "94000.0",
                "sz": "0.1",
                "time": 1700000000000_u64
            },
            {
                "coin": "BTC",
                "side": "A",
                "px": "94001.5",
                "sz": "0.25",
                "time": 1700000001000_u64
            }
        ]);

        let trades = parse_recent_trades(&json).unwrap();
        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].coin, "BTC");
        assert_eq!(trades[0].side, TradeSide::Buy);
        assert_eq!(trades[0].px, Decimal::from_str("94000.0").unwrap());
        assert_eq!(trades[0].sz, Decimal::from_str("0.1").unwrap());
        assert_eq!(trades[0].time, 1700000000000);
        assert_eq!(trades[1].side, TradeSide::Sell);
        assert_eq!(trades[1].px, Decimal::from_str("94001.5").unwrap());
        assert_eq!(trades[1].sz, Decimal::from_str("0.25").unwrap());
        assert_eq!(trades[1].time, 1700000001000);
    }

    #[test]
    fn test_parse_recent_trades_empty() {
        let json = serde_json::json!([]);
        let trades = parse_recent_trades(&json).unwrap();
        assert!(trades.is_empty());
    }

    #[test]
    fn test_parse_recent_trades_not_array() {
        let json = serde_json::json!({"error": "bad"});
        assert!(parse_recent_trades(&json).is_err());
    }

    #[test]
    fn test_parse_recent_trades_missing_coin() {
        let json = serde_json::json!([{
            "side": "B",
            "px": "100.0",
            "sz": "1.0",
            "time": 1_u64
        }]);
        assert!(parse_recent_trades(&json).is_err());
    }

    #[test]
    fn test_parse_recent_trades_missing_side() {
        let json = serde_json::json!([{
            "coin": "BTC",
            "px": "100.0",
            "sz": "1.0",
            "time": 1_u64
        }]);
        assert!(parse_recent_trades(&json).is_err());
    }

    #[test]
    fn test_parse_all_mids_valid() {
        let json = serde_json::json!({
            "BTC": "94000.5",
            "ETH": "3500.0",
            "SOL": "145.25"
        });

        let mids = parse_all_mids(&json).unwrap();
        assert_eq!(mids.len(), 3);
        assert_eq!(mids["BTC"], Decimal::from_str("94000.5").unwrap());
        assert_eq!(mids["ETH"], Decimal::from_str("3500.0").unwrap());
        assert_eq!(mids["SOL"], Decimal::from_str("145.25").unwrap());
    }

    #[test]
    fn test_parse_all_mids_empty() {
        let json = serde_json::json!({});
        let mids = parse_all_mids(&json).unwrap();
        assert!(mids.is_empty());
    }

    #[test]
    fn test_parse_all_mids_not_object() {
        let json = serde_json::json!([]);
        assert!(parse_all_mids(&json).is_err());
    }

    #[test]
    fn test_parse_all_mids_invalid_value() {
        let json = serde_json::json!({
            "BTC": "not_a_number"
        });
        assert!(parse_all_mids(&json).is_err());
    }

    // -----------------------------------------------------------------------
    // Mock transport tests — demonstrate unit testing without real HTTP calls
    // -----------------------------------------------------------------------

    use async_trait::async_trait;
    use hl_client::HttpTransport;
    use hl_types::Signature;
    use std::sync::Mutex;

    /// A mock HTTP transport that returns pre-configured JSON responses.
    struct MockTransport {
        responses: Mutex<Vec<serde_json::Value>>,
    }

    impl MockTransport {
        fn new(responses: Vec<serde_json::Value>) -> Self {
            Self {
                responses: Mutex::new(responses),
            }
        }
    }

    #[async_trait]
    impl HttpTransport for MockTransport {
        async fn post_info(
            &self,
            _request: serde_json::Value,
        ) -> Result<serde_json::Value, HlError> {
            let mut queue = self.responses.lock().unwrap();
            if queue.is_empty() {
                return Err(HlError::http("no more mock responses"));
            }
            Ok(queue.remove(0))
        }

        async fn post_action(
            &self,
            _action: serde_json::Value,
            _signature: &Signature,
            _nonce: u64,
            _vault_address: Option<&str>,
        ) -> Result<serde_json::Value, HlError> {
            unimplemented!("mock does not support post_action")
        }

        fn is_mainnet(&self) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn mock_transport_orderbook() {
        let response = serde_json::json!({
            "levels": [
                [
                    {"px": "60000.0", "sz": "1.0", "n": 1},
                    {"px": "59999.0", "sz": "2.0", "n": 2}
                ],
                [
                    {"px": "60001.0", "sz": "0.5", "n": 1},
                    {"px": "60002.0", "sz": "1.5", "n": 1}
                ]
            ],
            "time": 1700000000000_u64
        });

        let transport = Arc::new(MockTransport::new(vec![response]));
        let market = MarketData::new(transport);

        let book = market.orderbook("BTC").await.unwrap();
        assert_eq!(book.coin, "BTC");
        assert_eq!(book.bids.len(), 2);
        assert_eq!(book.asks.len(), 2);
        assert_eq!(book.bids[0].0, Decimal::from_str("60000.0").unwrap());
        assert_eq!(book.asks[0].0, Decimal::from_str("60001.0").unwrap());
    }

    #[tokio::test]
    async fn mock_transport_mid_price() {
        let response = serde_json::json!({
            "levels": [
                [{"px": "100.0", "sz": "1.0", "n": 1}],
                [{"px": "102.0", "sz": "1.0", "n": 1}]
            ],
            "time": 1700000000000_u64
        });

        let transport = Arc::new(MockTransport::new(vec![response]));
        let market = MarketData::new(transport);

        let mid = market.mid_price("TEST").await.unwrap();
        assert_eq!(mid, Decimal::from_str("101.0").unwrap());
    }

    #[test]
    fn test_parse_spot_meta_valid() {
        let json = serde_json::json!({
            "tokens": [
                { "name": "PURR", "index": 1, "szDecimals": 0, "weiDecimals": 18 },
                { "name": "USDC", "index": 2, "szDecimals": 2, "weiDecimals": 6 }
            ],
            "universe": []
        });
        let meta = parse_spot_meta(&json).unwrap();
        assert_eq!(meta.tokens.len(), 2);
        assert_eq!(meta.tokens[0].name, "PURR");
        assert_eq!(meta.tokens[0].index, 1);
        assert_eq!(meta.tokens[0].sz_decimals, 0);
        assert_eq!(meta.tokens[0].wei_decimals, 18);
        assert_eq!(meta.tokens[1].name, "USDC");
        assert_eq!(meta.tokens[1].index, 2);
        assert_eq!(meta.tokens[1].sz_decimals, 2);
        assert_eq!(meta.tokens[1].wei_decimals, 6);
    }

    #[test]
    fn test_parse_spot_meta_empty_tokens() {
        let json = serde_json::json!({ "tokens": [] });
        let meta = parse_spot_meta(&json).unwrap();
        assert!(meta.tokens.is_empty());
    }

    #[test]
    fn test_parse_spot_meta_missing_tokens() {
        let json = serde_json::json!({ "universe": [] });
        assert!(parse_spot_meta(&json).is_err());
    }

    #[test]
    fn test_parse_spot_meta_missing_name_errors() {
        let json = serde_json::json!({
            "tokens": [{ "index": 1, "szDecimals": 0, "weiDecimals": 18 }]
        });
        assert!(parse_spot_meta(&json).is_err());
    }

    #[tokio::test]
    async fn mock_transport_spot_meta() {
        let response = serde_json::json!({
            "tokens": [
                { "name": "PURR", "index": 1, "szDecimals": 0, "weiDecimals": 18 }
            ],
            "universe": []
        });

        let transport = Arc::new(MockTransport::new(vec![response]));
        let market = MarketData::new(transport);

        let meta = market.spot_meta().await.unwrap();
        assert_eq!(meta.tokens.len(), 1);
        assert_eq!(meta.tokens[0].name, "PURR");
    }

    #[tokio::test]
    async fn mock_transport_recent_trades() {
        let response = serde_json::json!([
            {
                "coin": "BTC",
                "side": "B",
                "px": "94000.0",
                "sz": "0.1",
                "time": 1700000000000_u64
            }
        ]);

        let transport = Arc::new(MockTransport::new(vec![response]));
        let market = MarketData::new(transport);

        let trades = market.recent_trades("BTC-PERP").await.unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].coin, "BTC");
        assert_eq!(trades[0].side, TradeSide::Buy);
        assert_eq!(trades[0].px, Decimal::from_str("94000.0").unwrap());
    }

    #[tokio::test]
    async fn mock_transport_all_mids() {
        let response = serde_json::json!({
            "BTC": "94000.5",
            "ETH": "3500.0"
        });

        let transport = Arc::new(MockTransport::new(vec![response]));
        let market = MarketData::new(transport);

        let mids = market.all_mids().await.unwrap();
        assert_eq!(mids.len(), 2);
        assert_eq!(mids["BTC"], Decimal::from_str("94000.5").unwrap());
        assert_eq!(mids["ETH"], Decimal::from_str("3500.0").unwrap());
    }

    #[tokio::test]
    async fn mock_transport_error_propagation() {
        // Empty response queue triggers an error
        let transport = Arc::new(MockTransport::new(vec![]));
        let market = MarketData::new(transport);

        let result = market.orderbook("BTC").await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // HIP-3 multi-DEX parser tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_perp_dex_status_valid() {
        let json = serde_json::json!({
            "name": "HyperBTC",
            "isActive": true,
            "numAssets": 5,
            "totalOi": "1000000.0"
        });
        let status = parse_perp_dex_status(&json, "HyperBTC").unwrap();
        assert_eq!(status.name, "HyperBTC");
        assert!(status.is_active);
        assert_eq!(status.num_assets, 5);
        assert_eq!(status.total_oi, Decimal::from_str("1000000.0").unwrap());
    }

    #[test]
    fn test_parse_perp_dex_status_inactive() {
        let json = serde_json::json!({
            "name": "TestDex",
            "isActive": false,
            "numAssets": 0,
            "totalOi": "0"
        });
        let status = parse_perp_dex_status(&json, "TestDex").unwrap();
        assert!(!status.is_active);
        assert_eq!(status.num_assets, 0);
        assert_eq!(status.total_oi, Decimal::ZERO);
    }

    #[test]
    fn test_parse_perp_dex_status_name_fallback() {
        // If "name" is missing from response, dex_name parameter is used.
        let json = serde_json::json!({
            "isActive": true,
            "numAssets": 3,
            "totalOi": "500.0"
        });
        let status = parse_perp_dex_status(&json, "FallbackName").unwrap();
        assert_eq!(status.name, "FallbackName");
    }

    #[test]
    fn test_parse_perp_dex_status_missing_is_active_errors() {
        let json = serde_json::json!({
            "name": "X",
            "numAssets": 1,
            "totalOi": "100.0"
        });
        let err = parse_perp_dex_status(&json, "X").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("isActive"),
            "should mention missing field: {msg}"
        );
    }

    #[test]
    fn test_parse_perp_dex_status_missing_num_assets_errors() {
        let json = serde_json::json!({
            "name": "X",
            "isActive": true,
            "totalOi": "100.0"
        });
        let err = parse_perp_dex_status(&json, "X").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("numAssets"),
            "should mention missing field: {msg}"
        );
    }

    #[test]
    fn test_parse_perp_dex_status_missing_total_oi_errors() {
        let json = serde_json::json!({
            "name": "X",
            "isActive": true,
            "numAssets": 1
        });
        assert!(parse_perp_dex_status(&json, "X").is_err());
    }

    #[test]
    fn test_parse_perp_dex_status_serde_roundtrip() {
        let status = HlPerpDexStatus::new(
            "MyDex".into(),
            true,
            10,
            Decimal::from_str("999999.99").unwrap(),
        );
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("isActive"));
        assert!(json.contains("numAssets"));
        assert!(json.contains("totalOi"));
        let parsed: HlPerpDexStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }

    #[test]
    fn test_parse_perps_at_oi_cap_valid() {
        let json = serde_json::json!(["BTC", "ETH", "SOL"]);
        let coins = parse_perps_at_oi_cap(&json).unwrap();
        assert_eq!(coins, vec!["BTC", "ETH", "SOL"]);
    }

    #[test]
    fn test_parse_perps_at_oi_cap_empty() {
        let json = serde_json::json!([]);
        let coins = parse_perps_at_oi_cap(&json).unwrap();
        assert!(coins.is_empty());
    }

    #[test]
    fn test_parse_perps_at_oi_cap_not_array_errors() {
        let json = serde_json::json!({"not": "array"});
        assert!(parse_perps_at_oi_cap(&json).is_err());
    }

    #[test]
    fn test_parse_perps_at_oi_cap_non_string_entry_errors() {
        let json = serde_json::json!(["BTC", 42]);
        assert!(parse_perps_at_oi_cap(&json).is_err());
    }

    #[tokio::test]
    async fn mock_transport_perp_dex_status() {
        let response = serde_json::json!({
            "name": "HyperBTC",
            "isActive": true,
            "numAssets": 5,
            "totalOi": "1000000.0"
        });

        let transport = Arc::new(MockTransport::new(vec![response]));
        let market = MarketData::new(transport);

        let status = market.perp_dex_status("HyperBTC").await.unwrap();
        assert_eq!(status.name, "HyperBTC");
        assert!(status.is_active);
        assert_eq!(status.num_assets, 5);
    }

    #[tokio::test]
    async fn mock_transport_perps_at_oi_cap() {
        let response = serde_json::json!(["BTC", "ETH"]);

        let transport = Arc::new(MockTransport::new(vec![response]));
        let market = MarketData::new(transport);

        let coins = market.perps_at_oi_cap().await.unwrap();
        assert_eq!(coins, vec!["BTC", "ETH"]);
    }
}
