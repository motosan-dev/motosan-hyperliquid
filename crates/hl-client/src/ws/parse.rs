use hl_types::HlCandle;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::str::FromStr;

use super::types::*;

/// Typed WebSocket message parsed from raw JSON.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WsMessage {
    /// All mid-price updates.
    AllMids(AllMidsData),
    /// L2 orderbook snapshot.
    L2Book(L2BookData),
    /// Recent trades.
    Trades(TradesData),
    /// Candle (OHLCV) update.
    Candle(CandleData),
    /// Best bid/offer update.
    Bbo(BboData),
    /// Order status change events.
    OrderUpdates(Vec<OrderUpdateData>),
    /// Aggregate user events.
    UserEvents(UserEventsData),
    /// User fill events.
    UserFills(UserFillsData),
    /// User funding payment events.
    UserFundings(UserFundingsData),
    /// Aggregate user data (web data v3).
    WebData3(WebData3Data),
    /// Clearinghouse state update.
    ClearinghouseState(ClearinghouseStateData),
    /// Active asset context (funding, OI, mark price).
    ActiveAssetCtx(ActiveAssetCtxData),
    /// Active asset data (leverage and sizing).
    ActiveAssetData(ActiveAssetDataMsg),
    /// TWAP order execution history.
    UserTwapHistory(UserTwapHistoryData),
    /// TWAP slice fill events.
    UserTwapSliceFills(UserTwapSliceFillsData),
    /// Subscription confirmation from the server.
    SubscriptionResponse,
    /// Pong response to a ping.
    Pong,
    /// Unrecognized message (forward-compatible).
    Unknown(serde_json::Value),
}

impl WsMessage {
    /// Parse a raw JSON value into a typed [`WsMessage`].
    pub fn parse(value: serde_json::Value) -> Self {
        if value.get("method").and_then(|m| m.as_str()) == Some("pong") {
            return WsMessage::Pong;
        }

        let channel = value.get("channel").and_then(|c| c.as_str()).unwrap_or("");
        let data = value
            .get("data")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        match channel {
            "allMids" => {
                let mids_val = data.get("mids").cloned().unwrap_or(data.clone());
                let mut mids = HashMap::new();
                if let Some(obj) = mids_val.as_object() {
                    for (k, v) in obj {
                        if let Some(s) = v.as_str() {
                            if let Ok(d) = Decimal::from_str(s) {
                                mids.insert(k.clone(), d);
                            }
                        }
                    }
                }
                WsMessage::AllMids(AllMidsData { mids })
            }
            "l2Book" => {
                let raw_levels = data
                    .get("levels")
                    .and_then(|l| l.as_array())
                    .cloned()
                    .unwrap_or_default();
                let levels: Vec<Vec<PriceLevel>> = raw_levels
                    .iter()
                    .map(|side| {
                        side.as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|entry| {
                                        let px =
                                            Decimal::from_str(entry.get("px")?.as_str()?).ok()?;
                                        let sz =
                                            Decimal::from_str(entry.get("sz")?.as_str()?).ok()?;
                                        let n = entry
                                            .get("n")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or_default()
                                            as u32;
                                        Some(PriceLevel { px, sz, n })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default()
                    })
                    .collect();
                WsMessage::L2Book(L2BookData {
                    coin: data
                        .get("coin")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .into(),
                    levels,
                    time: data.get("time").and_then(|t| t.as_u64()).unwrap_or(0),
                })
            }
            "trades" => {
                let raw_trades = data.as_array().cloned().unwrap_or_default();
                let coin = raw_trades
                    .first()
                    .and_then(|t| t.get("coin"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();
                let trades = raw_trades
                    .iter()
                    .filter_map(|t| {
                        let coin = t.get("coin")?.as_str()?.to_string();
                        let side = match t.get("side")?.as_str()? {
                            "B" => hl_types::TradeSide::Buy,
                            "A" => hl_types::TradeSide::Sell,
                            _ => return None,
                        };
                        let px = Decimal::from_str(t.get("px")?.as_str()?).ok()?;
                        let sz = Decimal::from_str(t.get("sz")?.as_str()?).ok()?;
                        let time = t.get("time").and_then(|v| v.as_u64()).unwrap_or(0);
                        let hash = t
                            .get("hash")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        Some(WsTrade {
                            coin,
                            side,
                            px,
                            sz,
                            time,
                            hash,
                        })
                    })
                    .collect();
                WsMessage::Trades(TradesData { coin, trades })
            }
            "candle" => {
                let coin: String = data.get("s").and_then(|c| c.as_str()).unwrap_or("").into();
                let parse_dec = |key: &str| -> Decimal {
                    data.get(key)
                        .and_then(|v| v.as_str())
                        .and_then(|s| Decimal::from_str(s).ok())
                        .unwrap_or_default()
                };
                let candle = HlCandle::new(
                    data.get("t").and_then(|v| v.as_u64()).unwrap_or_default(),
                    parse_dec("o"),
                    parse_dec("h"),
                    parse_dec("l"),
                    parse_dec("c"),
                    parse_dec("v"),
                );
                WsMessage::Candle(CandleData { coin, candle })
            }
            "bbo" => {
                let coin = data
                    .get("coin")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();
                let parse_decimal = |key: &str| -> Decimal {
                    data.get(key)
                        .and_then(|v| v.as_str())
                        .and_then(|s| Decimal::from_str(s).ok())
                        .unwrap_or_default()
                };
                WsMessage::Bbo(BboData {
                    coin,
                    bid_px: parse_decimal("bidPx"),
                    bid_sz: parse_decimal("bidSz"),
                    ask_px: parse_decimal("askPx"),
                    ask_sz: parse_decimal("askSz"),
                    time: data.get("time").and_then(|t| t.as_u64()).unwrap_or(0),
                })
            }
            "orderUpdates" => {
                let updates = data
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|item| {
                                let o = item.get("order")?;
                                let parse_order_dec = |key: &str| -> Decimal {
                                    o.get(key)
                                        .and_then(|v| v.as_str())
                                        .and_then(|s| Decimal::from_str(s).ok())
                                        .unwrap_or_default()
                                };
                                let side = match o.get("side")?.as_str()? {
                                    "B" => hl_types::TradeSide::Buy,
                                    "A" => hl_types::TradeSide::Sell,
                                    _ => return None,
                                };
                                let order = WsOrderUpdate {
                                    oid: o.get("oid").and_then(|v| v.as_u64()).unwrap_or_default(),
                                    coin: o
                                        .get("coin")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    side,
                                    limit_px: parse_order_dec("limitPx"),
                                    sz: parse_order_dec("sz"),
                                    orig_sz: parse_order_dec("origSz"),
                                    cloid: o
                                        .get("cloid")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                };
                                Some(OrderUpdateData {
                                    order,
                                    status: item
                                        .get("status")
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("")
                                        .into(),
                                    timestamp: item
                                        .get("statusTimestamp")
                                        .and_then(|t| t.as_u64())
                                        .unwrap_or(0),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                WsMessage::OrderUpdates(updates)
            }
            "user" => WsMessage::UserEvents(UserEventsData {
                events: data.as_array().cloned().unwrap_or_default(),
            }),
            "userFills" => WsMessage::UserFills(UserFillsData {
                user: data
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .into(),
                fills: data
                    .get("fills")
                    .and_then(|f| f.as_array())
                    .cloned()
                    .unwrap_or_default(),
            }),
            "userFundings" => WsMessage::UserFundings(UserFundingsData {
                user: data
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .into(),
                funding: data,
            }),
            "webData3" => WsMessage::WebData3(WebData3Data {
                user: data
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .into(),
                data: data.clone(),
            }),
            "clearinghouseState" => WsMessage::ClearinghouseState(ClearinghouseStateData {
                user: data
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .into(),
                data: data.clone(),
            }),
            "activeAssetCtx" => WsMessage::ActiveAssetCtx(ActiveAssetCtxData {
                coin: data
                    .get("coin")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .into(),
                ctx: data.get("ctx").cloned().unwrap_or_default(),
            }),
            "activeAssetData" => WsMessage::ActiveAssetData(ActiveAssetDataMsg {
                coin: data
                    .get("coin")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .into(),
                data: data.clone(),
            }),
            "userTwapHistory" => WsMessage::UserTwapHistory(UserTwapHistoryData {
                user: data
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .into(),
                history: data
                    .get("history")
                    .and_then(|h| h.as_array())
                    .cloned()
                    .unwrap_or_default(),
            }),
            "userTwapSliceFills" => WsMessage::UserTwapSliceFills(UserTwapSliceFillsData {
                user: data
                    .get("user")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .into(),
                fills: data
                    .get("fills")
                    .and_then(|f| f.as_array())
                    .cloned()
                    .unwrap_or_default(),
            }),
            "subscriptionResponse" => WsMessage::SubscriptionResponse,
            "pong" => WsMessage::Pong,
            _ => WsMessage::Unknown(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_l2_book_message() {
        let raw = serde_json::json!({
            "channel": "l2Book",
            "data": {
                "coin": "BTC",
                "levels": [
                    [{"px":"90000","sz":"1.0","n":3}],
                    [{"px":"90001","sz":"0.5","n":1}]
                ],
                "time": 1_700_000_000_000u64
            }
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::L2Book(d) => {
                assert_eq!(d.coin, "BTC");
                assert_eq!(d.time, 1_700_000_000_000);
                assert_eq!(d.levels.len(), 2);
                // bids
                assert_eq!(d.levels[0].len(), 1);
                assert_eq!(d.levels[0][0].px, Decimal::from_str("90000").unwrap());
                assert_eq!(d.levels[0][0].sz, Decimal::from_str("1.0").unwrap());
                assert_eq!(d.levels[0][0].n, 3);
                // asks
                assert_eq!(d.levels[1].len(), 1);
                assert_eq!(d.levels[1][0].px, Decimal::from_str("90001").unwrap());
                assert_eq!(d.levels[1][0].sz, Decimal::from_str("0.5").unwrap());
                assert_eq!(d.levels[1][0].n, 1);
            }
            other => panic!("expected L2Book, got: {other:?}"),
        }
    }

    #[test]
    fn parse_l2_book_missing_n_defaults_to_zero() {
        let raw = serde_json::json!({
            "channel": "l2Book",
            "data": {
                "coin": "ETH",
                "levels": [[{"px":"3000","sz":"2.5"}]],
                "time": 100
            }
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::L2Book(d) => {
                assert_eq!(d.levels[0][0].n, 0);
                assert_eq!(d.levels[0][0].px, Decimal::from_str("3000").unwrap());
            }
            other => panic!("expected L2Book, got: {other:?}"),
        }
    }

    #[test]
    fn parse_l2_book_empty_levels() {
        let raw = serde_json::json!({
            "channel": "l2Book",
            "data": {"coin": "SOL", "levels": [], "time": 0}
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::L2Book(d) => {
                assert!(d.levels.is_empty());
            }
            other => panic!("expected L2Book, got: {other:?}"),
        }
    }

    #[test]
    fn parse_l2_book_skips_malformed_entries() {
        let raw = serde_json::json!({
            "channel": "l2Book",
            "data": {
                "coin": "BTC",
                "levels": [[{"px":"90000","sz":"1.0","n":2},{"bad":"field"}]],
                "time": 0
            }
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::L2Book(d) => {
                assert_eq!(d.levels[0].len(), 1);
            }
            other => panic!("expected L2Book, got: {other:?}"),
        }
    }

    #[test]
    fn parse_user_fills_message() {
        let raw = serde_json::json!({
            "channel": "userFills",
            "data": {"user": "0xABC", "fills": [{"coin":"BTC"}]}
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::UserFills(d) => {
                assert_eq!(d.user, "0xABC");
                assert_eq!(d.fills.len(), 1);
            }
            other => panic!("expected UserFills, got: {other:?}"),
        }
    }

    #[test]
    fn parse_order_updates_message() {
        let raw = serde_json::json!({
            "channel": "orderUpdates",
            "data": [{
                "order": {
                    "oid": 123,
                    "coin": "BTC",
                    "side": "B",
                    "limitPx": "90000.0",
                    "sz": "1.5",
                    "origSz": "2.0",
                    "cloid": "my-order-1"
                },
                "status": "filled",
                "statusTimestamp": 1_700_000_000_000u64
            }]
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::OrderUpdates(u) => {
                assert_eq!(u.len(), 1);
                assert_eq!(u[0].status, "filled");
                assert_eq!(u[0].order.oid, 123);
                assert_eq!(u[0].order.coin, "BTC");
                assert_eq!(u[0].order.side, hl_types::TradeSide::Buy);
                assert_eq!(u[0].order.limit_px, Decimal::from_str("90000.0").unwrap());
                assert_eq!(u[0].order.sz, Decimal::from_str("1.5").unwrap());
                assert_eq!(u[0].order.orig_sz, Decimal::from_str("2.0").unwrap());
                assert_eq!(u[0].order.cloid, Some("my-order-1".to_string()));
            }
            other => panic!("expected OrderUpdates, got: {other:?}"),
        }
    }

    #[test]
    fn parse_order_updates_missing_cloid() {
        let raw = serde_json::json!({
            "channel": "orderUpdates",
            "data": [{
                "order": {
                    "oid": 456,
                    "coin": "ETH",
                    "side": "A",
                    "limitPx": "3000",
                    "sz": "0.5",
                    "origSz": "0.5"
                },
                "status": "open",
                "statusTimestamp": 100
            }]
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::OrderUpdates(u) => {
                assert_eq!(u[0].order.oid, 456);
                assert_eq!(u[0].order.cloid, None);
            }
            other => panic!("expected OrderUpdates, got: {other:?}"),
        }
    }

    #[test]
    fn parse_order_updates_missing_order_fields_skipped() {
        let raw = serde_json::json!({
            "channel": "orderUpdates",
            "data": [{"order": {}, "status": "canceled", "statusTimestamp": 0}]
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::OrderUpdates(u) => {
                // Entries with missing required fields (e.g. side) are skipped
                assert!(u.is_empty());
            }
            other => panic!("expected OrderUpdates, got: {other:?}"),
        }
    }

    #[test]
    fn parse_subscription_response() {
        let raw = serde_json::json!({
            "channel": "subscriptionResponse",
            "data": {"method": "subscribe"}
        });
        assert!(matches!(
            WsMessage::parse(raw),
            WsMessage::SubscriptionResponse
        ));
    }

    #[test]
    fn parse_unknown_channel() {
        let raw = serde_json::json!({"channel": "futureChannel", "data": {}});
        assert!(matches!(WsMessage::parse(raw), WsMessage::Unknown(_)));
    }

    #[test]
    fn parse_malformed_returns_unknown() {
        let raw = serde_json::json!("just a string");
        assert!(matches!(WsMessage::parse(raw), WsMessage::Unknown(_)));
    }

    #[test]
    fn parse_all_mids_message() {
        let raw = serde_json::json!({
            "channel": "allMids",
            "data": {"mids": {"BTC": "90000", "ETH": "3000"}}
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::AllMids(data) => {
                assert_eq!(data.mids.len(), 2);
                assert_eq!(
                    *data.mids.get("BTC").unwrap(),
                    Decimal::from_str("90000").unwrap()
                );
                assert_eq!(
                    *data.mids.get("ETH").unwrap(),
                    Decimal::from_str("3000").unwrap()
                );
            }
            other => panic!("expected AllMids, got: {:?}", other),
        }
    }

    #[test]
    fn parse_all_mids_empty() {
        let raw = serde_json::json!({
            "channel": "allMids",
            "data": {"mids": {}}
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::AllMids(data) => assert!(data.mids.is_empty()),
            other => panic!("expected AllMids, got: {:?}", other),
        }
    }

    #[test]
    fn parse_all_mids_skips_unparseable_values() {
        let raw = serde_json::json!({
            "channel": "allMids",
            "data": {"mids": {"BTC": "90000", "BAD": "not_a_number"}}
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::AllMids(data) => {
                assert_eq!(data.mids.len(), 1);
                assert!(data.mids.contains_key("BTC"));
                assert!(!data.mids.contains_key("BAD"));
            }
            other => panic!("expected AllMids, got: {:?}", other),
        }
    }

    #[test]
    fn parse_trades_message() {
        let raw = serde_json::json!({
            "channel": "trades",
            "data": [
                {"coin": "ETH", "side": "B", "px": "3000.50", "sz": "1.2", "time": 1700000000000u64, "hash": "0xabc"},
                {"coin": "ETH", "side": "A", "px": "3001.00", "sz": "0.5", "time": 1700000000001u64, "hash": "0xdef"}
            ]
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::Trades(data) => {
                assert_eq!(data.coin, "ETH");
                assert_eq!(data.trades.len(), 2);
                assert_eq!(data.trades[0].coin, "ETH");
                assert_eq!(data.trades[0].side, hl_types::TradeSide::Buy);
                assert_eq!(data.trades[0].px, Decimal::from_str("3000.50").unwrap());
                assert_eq!(data.trades[0].sz, Decimal::from_str("1.2").unwrap());
                assert_eq!(data.trades[0].time, 1700000000000);
                assert_eq!(data.trades[0].hash, "0xabc");
                assert_eq!(data.trades[1].side, hl_types::TradeSide::Sell);
            }
            other => panic!("expected Trades, got: {:?}", other),
        }
    }

    #[test]
    fn parse_trades_skips_malformed_entries() {
        let raw = serde_json::json!({
            "channel": "trades",
            "data": [
                {"coin": "ETH", "side": "B", "px": "3000", "sz": "1.0", "time": 100u64, "hash": "0x1"},
                {"coin": "ETH", "bad_field": true}
            ]
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::Trades(data) => {
                assert_eq!(data.trades.len(), 1);
                assert_eq!(data.trades[0].px, Decimal::from_str("3000").unwrap());
            }
            other => panic!("expected Trades, got: {:?}", other),
        }
    }

    #[test]
    fn parse_trades_empty_array() {
        let raw = serde_json::json!({
            "channel": "trades",
            "data": []
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::Trades(data) => {
                assert_eq!(data.coin, "");
                assert!(data.trades.is_empty());
            }
            other => panic!("expected Trades, got: {:?}", other),
        }
    }

    #[test]
    fn parse_bbo_message() {
        let raw = serde_json::json!({
            "channel": "bbo",
            "data": {
                "coin": "SOL",
                "bidPx": "150.25",
                "bidSz": "100.0",
                "askPx": "150.30",
                "askSz": "50.0",
                "time": 1700000000000u64
            }
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::Bbo(data) => {
                assert_eq!(data.coin, "SOL");
                assert_eq!(data.bid_px, Decimal::from_str("150.25").unwrap());
                assert_eq!(data.bid_sz, Decimal::from_str("100.0").unwrap());
                assert_eq!(data.ask_px, Decimal::from_str("150.30").unwrap());
                assert_eq!(data.ask_sz, Decimal::from_str("50.0").unwrap());
                assert_eq!(data.time, 1700000000000);
            }
            other => panic!("expected Bbo, got: {:?}", other),
        }
    }

    #[test]
    fn parse_bbo_missing_fields_defaults_to_zero() {
        let raw = serde_json::json!({
            "channel": "bbo",
            "data": {"coin": "BTC"}
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::Bbo(data) => {
                assert_eq!(data.coin, "BTC");
                assert_eq!(data.bid_px, Decimal::default());
                assert_eq!(data.ask_px, Decimal::default());
                assert_eq!(data.time, 0);
            }
            other => panic!("expected Bbo, got: {:?}", other),
        }
    }

    #[test]
    fn parse_candle_message() {
        let raw = serde_json::json!({
            "channel": "candle",
            "data": {
                "s": "BTC",
                "t": 1_700_000_000_000u64,
                "o": "90000.0",
                "h": "91000.0",
                "l": "89000.0",
                "c": "90500.0",
                "v": "1234.56"
            }
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::Candle(d) => {
                assert_eq!(d.coin, "BTC");
                assert_eq!(d.candle.timestamp, 1_700_000_000_000);
                assert_eq!(d.candle.open, Decimal::from_str("90000.0").unwrap());
                assert_eq!(d.candle.high, Decimal::from_str("91000.0").unwrap());
                assert_eq!(d.candle.low, Decimal::from_str("89000.0").unwrap());
                assert_eq!(d.candle.close, Decimal::from_str("90500.0").unwrap());
                assert_eq!(d.candle.volume, Decimal::from_str("1234.56").unwrap());
            }
            other => panic!("expected Candle, got: {other:?}"),
        }
    }

    #[test]
    fn parse_candle_missing_fields_defaults() {
        let raw = serde_json::json!({
            "channel": "candle",
            "data": {"s": "ETH"}
        });
        let msg = WsMessage::parse(raw);
        match msg {
            WsMessage::Candle(d) => {
                assert_eq!(d.coin, "ETH");
                assert_eq!(d.candle.timestamp, 0);
                assert_eq!(d.candle.open, Decimal::default());
                assert_eq!(d.candle.volume, Decimal::default());
            }
            other => panic!("expected Candle, got: {other:?}"),
        }
    }

    #[test]
    fn parse_pong_message() {
        let raw = serde_json::json!({"channel": "pong"});
        assert!(matches!(WsMessage::parse(raw), WsMessage::Pong));
    }

    #[test]
    fn parse_pong_method_message() {
        let raw = serde_json::json!({"method": "pong"});
        assert!(matches!(WsMessage::parse(raw), WsMessage::Pong));
    }
}
