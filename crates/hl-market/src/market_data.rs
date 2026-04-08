use hl_client::HyperliquidClient;
use hl_types::{HlAssetInfo, HlCandle, HlError, HlFundingRate, HlOrderbook, normalize_coin};

pub struct MarketData {
    client: HyperliquidClient,
}

impl MarketData {
    pub fn new(client: HyperliquidClient) -> Self {
        Self { client }
    }

    /// Fetch OHLCV candle snapshots for a given coin, interval, and limit.
    ///
    /// `interval` must be one of: `1m`, `5m`, `15m`, `1h`, `4h`, `1d`.
    /// `limit` caps the number of candles returned (most recent).
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
    pub async fn orderbook(&self, coin: &str) -> Result<HlOrderbook, HlError> {
        let coin = normalize_coin(coin).to_uppercase();
        let payload = serde_json::json!({ "type": "l2Book", "coin": coin });
        let resp = self.client.post_info(payload).await?;
        parse_orderbook(&resp, &coin)
    }

    /// Fetch static asset metadata for all perpetuals.
    pub async fn asset_info(&self) -> Result<Vec<HlAssetInfo>, HlError> {
        let payload = serde_json::json!({ "type": "metaAndAssetCtxs" });
        let resp = self.client.post_info(payload).await?;
        parse_asset_info(&resp)
    }

    /// Fetch current funding rates for all perpetuals.
    pub async fn funding_rates(&self) -> Result<Vec<HlFundingRate>, HlError> {
        let payload = serde_json::json!({ "type": "metaAndAssetCtxs" });
        let resp = self.client.post_info(payload).await?;
        parse_funding_rates(&resp)
    }

    /// Compute the mid-price for a coin from its current orderbook.
    ///
    /// Returns `Err` if either the bid or ask side of the book is empty.
    pub async fn mid_price(&self, coin: &str) -> Result<f64, HlError> {
        let book = self.orderbook(coin).await?;
        let best_bid = book.bids.first().map(|(p, _)| *p);
        let best_ask = book.asks.first().map(|(p, _)| *p);
        match (best_bid, best_ask) {
            (Some(bid), Some(ask)) => Ok((bid + ask) / 2.0),
            _ => Err(HlError::Parse(format!(
                "empty orderbook for {coin}, cannot compute mid price"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse a JSON value that might be a string-encoded float or a number.
fn parse_str_f64(val: Option<&serde_json::Value>) -> f64 {
    match val {
        Some(serde_json::Value::String(s)) => s.parse::<f64>().unwrap_or(0.0),
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        _ => 0.0,
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

        candles.push(HlCandle {
            timestamp: time_ms,
            open: parse_str_f64(item.get("o")),
            high: parse_str_f64(item.get("h")),
            low: parse_str_f64(item.get("l")),
            close: parse_str_f64(item.get("c")),
            volume: parse_str_f64(item.get("v")),
        });
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

    let parse_levels = |arr: &serde_json::Value| -> Vec<(f64, f64)> {
        arr.as_array()
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| {
                        let px = parse_str_f64(entry.get("px"));
                        let sz = parse_str_f64(entry.get("sz"));
                        if px > 0.0 { Some((px, sz)) } else { None }
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    let bids = parse_levels(&levels[0]);
    let asks = parse_levels(&levels[1]);

    let timestamp = response
        .get("time")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() as u64);

    Ok(HlOrderbook {
        coin: coin.to_string(),
        bids,
        asks,
        timestamp,
    })
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
            1.0
        } else {
            10_f64.powi(-(sz_decimals as i32))
        };

        result.push(HlAssetInfo {
            coin,
            asset_id: idx as u32,
            min_size,
            sz_decimals,
            px_decimals,
        });
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

        let funding_rate = parse_str_f64(ctx.get("funding"));

        let next_funding_time = ctx
            .get("nextFundingTime")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        result.push(HlFundingRate {
            coin,
            funding_rate,
            next_funding_time,
        });
    }

    Ok(result)
}

/// Convert a candle interval string to its duration in milliseconds.
fn interval_to_ms(interval: &str) -> Result<u64, HlError> {
    match interval {
        "1m" => Ok(60 * 1_000),
        "5m" => Ok(5 * 60 * 1_000),
        "15m" => Ok(15 * 60 * 1_000),
        "1h" => Ok(60 * 60 * 1_000),
        "4h" => Ok(4 * 60 * 60 * 1_000),
        "1d" => Ok(24 * 60 * 60 * 1_000),
        _ => Err(parse_err(format!(
            "unsupported interval '{}'; valid values: 1m, 5m, 15m, 1h, 4h, 1d",
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
        assert_eq!(candles[0].open, 94200.0);
        assert_eq!(candles[0].close, 95234.0);
        assert_eq!(candles[0].high, 95400.0);
        assert_eq!(candles[0].low, 93800.0);
        assert_eq!(candles[0].volume, 12400.5);
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
        assert_eq!(book.bids[0].0, 94000.0);
        assert_eq!(book.asks[0].0, 94001.0);
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
        assert_eq!(rates[0].funding_rate, 0.0001);
        assert_eq!(rates[0].next_funding_time, 1709989200000);
        assert_eq!(rates[1].coin, "ETH");
        assert_eq!(rates[1].funding_rate, -0.00005);
    }
}
