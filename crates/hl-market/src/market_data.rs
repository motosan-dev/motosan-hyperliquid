use std::str::FromStr;
use std::sync::Arc;

use rust_decimal::Decimal;

use hl_client::{HttpTransport, HyperliquidClient};
use hl_types::{normalize_coin, HlAssetInfo, HlCandle, HlError, HlFundingRate, HlOrderbook};

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

        candles.push(HlCandle {
            timestamp: time_ms,
            open: parse_str_decimal(item.get("o"), "o")?,
            high: parse_str_decimal(item.get("h"), "h")?,
            low: parse_str_decimal(item.get("l"), "l")?,
            close: parse_str_decimal(item.get("c"), "c")?,
            // Volume of 0 is valid (no trades in interval), so treat missing as 0.
            volume: parse_str_decimal(item.get("v"), "v").unwrap_or(Decimal::ZERO),
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
            Decimal::ONE
        } else {
            Decimal::ONE / Decimal::from(10u64.pow(sz_decimals))
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

        let funding_rate = parse_str_decimal(ctx.get("funding"), "funding")?;

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
                return Err(HlError::Http("no more mock responses".into()));
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

    #[tokio::test]
    async fn mock_transport_error_propagation() {
        // Empty response queue triggers an error
        let transport = Arc::new(MockTransport::new(vec![]));
        let market = MarketData::new(transport);

        let result = market.orderbook("BTC").await;
        assert!(result.is_err());
    }
}
