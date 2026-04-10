use rust_decimal::Decimal;

use hl_types::{
    parse_str_decimal, HlAccountState, HlBorrowLendState, HlError, HlFill, HlFundingEntry,
    HlHistoricalOrder, HlOpenOrder, HlOrderDetail, HlPosition, HlRateLimitStatus, HlSpotBalance,
    HlStakingDelegation, HlUserFees, HlUserFundingEntry,
};

/// A small threshold used to detect zero-size (closed) positions.
const ZERO_SIZE_THRESHOLD: Decimal = Decimal::from_parts(1, 0, 0, false, 12); // 1e-12

/// Parse a `clearinghouseState` JSON response into an [`HlAccountState`].
///
/// Hyperliquid returns numeric fields as quoted strings, e.g. `"szi": "0.001"`.
/// Zero-size positions (|szi| < 1e-12) are skipped.
pub(crate) fn parse_account_state(resp: &serde_json::Value) -> Result<HlAccountState, HlError> {
    let margin_summary = resp
        .get("marginSummary")
        .ok_or_else(|| HlError::Parse("missing 'marginSummary' in clearinghouseState".into()))?;

    let equity: Decimal = parse_str_decimal(margin_summary.get("accountValue"), "accountValue")?;

    let margin_available: Decimal = parse_str_decimal(
        margin_summary
            .get("totalRawUsd")
            .or_else(|| margin_summary.get("availableMargin")),
        "totalRawUsd/availableMargin",
    )?;

    let mut positions = Vec::new();

    if let Some(asset_positions) = resp["assetPositions"].as_array() {
        for pos in asset_positions {
            let p = &pos["position"];

            // Size: parse with error propagation. A size of 0.0 is valid
            // (means the position is closed), so we skip it rather than error.
            let size: Decimal = parse_str_decimal(p.get("szi"), "szi")?;
            if size.abs() < ZERO_SIZE_THRESHOLD {
                continue;
            }

            let coin = match p.get("coin").and_then(|v| v.as_str()) {
                Some(c) if !c.is_empty() => c.to_string(),
                _ => {
                    tracing::warn!("Skipping position with missing or empty coin field");
                    continue;
                }
            };

            let entry_px: Decimal = parse_str_decimal(p.get("entryPx"), "entryPx")?;
            let unrealized_pnl: Decimal =
                parse_str_decimal(p.get("unrealizedPnl"), "unrealizedPnl")?;
            let leverage: Decimal = parse_str_decimal(
                p.get("leverage").and_then(|l| l.get("value")),
                "leverage.value",
            )
            // Leverage defaults to 1.0 if unparseable (cross-margin mode).
            .unwrap_or(Decimal::ONE);
            let liquidation_px: Option<Decimal> = match p.get("liquidationPx") {
                Some(serde_json::Value::Null) | None => None,
                Some(v) => Some(parse_str_decimal(Some(v), "liquidationPx")?),
            };

            positions.push(HlPosition::new(
                coin,
                size,
                entry_px,
                unrealized_pnl,
                leverage,
                liquidation_px,
            ));
        }
    }

    Ok(HlAccountState::new(equity, margin_available, positions))
}

/// Parse a `userFills` JSON response into a [`Vec<HlFill>`].
///
/// Hyperliquid returns numeric fields as quoted strings.
/// The `side` field is `"B"` (buy) or `"A"` (ask/sell).
pub(crate) fn parse_fills(resp: &serde_json::Value) -> Result<Vec<HlFill>, HlError> {
    let arr = resp
        .as_array()
        .ok_or_else(|| HlError::Parse("expected array for userFills".into()))?;

    let mut fills = Vec::with_capacity(arr.len());

    for fill in arr {
        let coin = match fill["coin"].as_str() {
            Some(c) if !c.is_empty() => c.to_string(),
            _ => {
                tracing::warn!("Skipping fill with missing or empty coin field");
                continue;
            }
        };

        let px: Decimal = parse_str_decimal(fill.get("px"), "px")?;
        let sz: Decimal = parse_str_decimal(fill.get("sz"), "sz")?;
        let is_buy = fill
            .get("side")
            .and_then(|v| v.as_str())
            .ok_or_else(|| HlError::Parse("missing 'side' in fill".into()))?
            == "B";
        let timestamp: u64 = fill
            .get("time")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| HlError::Parse("missing or invalid 'time' in fill".into()))?;
        let fee: Decimal = parse_str_decimal(fill.get("fee"), "fee")?;
        // closedPnl may be 0.0 legitimately (no realized PnL), default to 0.0 if missing.
        let closed_pnl: Decimal =
            parse_str_decimal(fill.get("closedPnl"), "closedPnl").unwrap_or(Decimal::ZERO);

        fills.push(HlFill::new(
            coin, px, sz, is_buy, timestamp, fee, closed_pnl,
        ));
    }

    Ok(fills)
}

/// Parse a `spotClearinghouseState` JSON response into a [`Vec<HlSpotBalance>`].
///
/// The response contains a `balances` array:
///   `{ "balances": [{ "coin": "PURR", "token": 1, "hold": "0", "total": "1000.0" }, ...] }`
pub(crate) fn parse_spot_state(resp: &serde_json::Value) -> Result<Vec<HlSpotBalance>, HlError> {
    let balances_arr = resp
        .get("balances")
        .and_then(|v| v.as_array())
        .ok_or_else(|| HlError::Parse("missing 'balances' in spotClearinghouseState".into()))?;

    let mut balances = Vec::with_capacity(balances_arr.len());
    for item in balances_arr {
        let coin = match item.get("coin").and_then(|v| v.as_str()) {
            Some(c) if !c.is_empty() => c.to_string(),
            _ => {
                tracing::warn!("Skipping spot balance with missing or empty coin field");
                continue;
            }
        };

        let token = item
            .get("token")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| HlError::Parse("missing 'token' in spot balance".into()))?
            as u32;

        let hold = parse_str_decimal(item.get("hold"), "hold")?;

        let total = parse_str_decimal(item.get("total"), "total")?;

        balances.push(HlSpotBalance::new(coin, token, hold, total));
    }

    Ok(balances)
}

/// Parse a `stakingDelegations` JSON response into a [`Vec<HlStakingDelegation>`].
///
/// Hyperliquid returns: `[{"validator": "0x...", "amount": "1000.0", "rewards": "5.0"}, ...]`
pub(crate) fn parse_staking_delegations(
    resp: &serde_json::Value,
) -> Result<Vec<HlStakingDelegation>, HlError> {
    let arr = resp
        .as_array()
        .ok_or_else(|| HlError::Parse("expected array for stakingDelegations".into()))?;

    let mut delegations = Vec::with_capacity(arr.len());
    for item in arr {
        let validator = match item.get("validator").and_then(|v| v.as_str()) {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => {
                tracing::warn!("Skipping delegation with missing or empty validator");
                continue;
            }
        };
        let amount = parse_str_decimal(item.get("amount"), "amount")?;
        let rewards = parse_str_decimal(item.get("rewards"), "rewards")?;
        delegations.push(HlStakingDelegation::new(validator, amount, rewards));
    }

    Ok(delegations)
}

/// Parse borrow/lend state from a `spotClearinghouseState` JSON response.
///
/// Extracts entries from the `balances` array that have `supply`, `borrow`, and
/// `apy` fields. Entries without these fields are skipped (they are plain spot
/// balances, not borrow/lend positions).
pub(crate) fn parse_borrow_lend_state(
    resp: &serde_json::Value,
) -> Result<Vec<HlBorrowLendState>, HlError> {
    let balances_arr = resp
        .get("balances")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            HlError::Parse("missing 'balances' in spotClearinghouseState for borrow/lend".into())
        })?;

    let mut states = Vec::new();
    for item in balances_arr {
        // Only process entries that have borrow/lend fields.
        let (supply_val, borrow_val, apy_val) =
            match (item.get("supply"), item.get("borrow"), item.get("apy")) {
                (Some(s), Some(b), Some(a)) => (s, b, a),
                _ => continue, // Not a borrow/lend entry, skip.
            };

        let coin = match item.get("coin").and_then(|v| v.as_str()) {
            Some(c) if !c.is_empty() => c.to_string(),
            _ => {
                tracing::warn!("Skipping borrow/lend entry with missing or empty coin");
                continue;
            }
        };

        let supply = parse_str_decimal(Some(supply_val), "supply")?;
        let borrow = parse_str_decimal(Some(borrow_val), "borrow")?;
        let apy = parse_str_decimal(Some(apy_val), "apy")?;

        states.push(HlBorrowLendState::new(coin, supply, borrow, apy));
    }

    Ok(states)
}

/// Parse a `userFees` JSON response into an [`HlUserFees`].
///
/// The API returns something like:
///   `{"userCrossRate": "0.0002", "userAddRate": "0.0005", ...}`
/// We map `userCrossRate` → `maker_rate` and `userAddRate` → `taker_rate`.
pub(crate) fn parse_user_fees(resp: &serde_json::Value) -> Result<HlUserFees, HlError> {
    let fee_tier = resp
        .get("feeTier")
        .or_else(|| resp.get("userFeeTier"))
        .and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .unwrap_or_default();

    let maker_rate = parse_str_decimal(resp.get("userCrossRate"), "userCrossRate")?;

    let taker_rate = parse_str_decimal(resp.get("userAddRate"), "userAddRate")?;

    Ok(HlUserFees::new(fee_tier, maker_rate, taker_rate))
}

/// Helper to parse common order fields from a JSON value.
///
/// Extracts `oid`, `coin`, `side`, `limitPx`, `sz`, `timestamp`, `orderType`, and `cloid`.
#[allow(clippy::type_complexity)]
fn parse_order_fields(
    item: &serde_json::Value,
    context: &str,
) -> Result<
    (
        u64,
        String,
        String,
        Decimal,
        Decimal,
        u64,
        String,
        Option<String>,
    ),
    HlError,
> {
    let oid = item
        .get("oid")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| HlError::Parse(format!("missing or invalid 'oid' in {context}")))?;
    let coin = item
        .get("coin")
        .and_then(|v| v.as_str())
        .ok_or_else(|| HlError::Parse(format!("missing 'coin' in {context}")))?
        .to_string();
    let side = item
        .get("side")
        .and_then(|v| v.as_str())
        .ok_or_else(|| HlError::Parse(format!("missing 'side' in {context}")))?
        .to_string();
    let limit_px = parse_str_decimal(item.get("limitPx"), "limitPx")?;
    let sz = parse_str_decimal(item.get("sz"), "sz")?;
    let timestamp = item
        .get("timestamp")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| HlError::Parse(format!("missing or invalid 'timestamp' in {context}")))?;
    let order_type = item
        .get("orderType")
        .and_then(|v| v.as_str())
        .unwrap_or("Limit")
        .to_string();
    let cloid = item
        .get("cloid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Ok((oid, coin, side, limit_px, sz, timestamp, order_type, cloid))
}

/// Parse an `openOrders` JSON response into a [`Vec<HlOpenOrder>`].
///
/// Hyperliquid returns: `[{"oid": 123, "coin": "BTC", "side": "B", "limitPx": "60000.0", ...}, ...]`
pub(crate) fn parse_open_orders(resp: &serde_json::Value) -> Result<Vec<HlOpenOrder>, HlError> {
    let arr = resp
        .as_array()
        .ok_or_else(|| HlError::Parse("expected array for openOrders".into()))?;

    let mut orders = Vec::with_capacity(arr.len());
    for item in arr {
        let (oid, coin, side, limit_px, sz, timestamp, order_type, cloid) =
            parse_order_fields(item, "openOrder")?;
        orders.push(HlOpenOrder::new(
            oid, coin, side, limit_px, sz, timestamp, order_type, cloid,
        ));
    }
    Ok(orders)
}

/// Parse an `orderStatus` JSON response into an [`HlOrderDetail`].
///
/// The API may return `{"order": {...}, "status": "..."}` or an error-like object.
pub(crate) fn parse_order_status(resp: &serde_json::Value) -> Result<HlOrderDetail, HlError> {
    // The API wraps the order in an "order" field with a "status" alongside it.
    let order_val = resp.get("order").unwrap_or(resp);
    let status = resp
        .get("status")
        .and_then(|v| v.as_str())
        .or_else(|| order_val.get("status").and_then(|v| v.as_str()))
        .unwrap_or("unknown")
        .to_string();

    let (oid, coin, side, limit_px, sz, timestamp, order_type, cloid) =
        parse_order_fields(order_val, "orderStatus")?;

    Ok(HlOrderDetail::new(
        oid, coin, side, limit_px, sz, timestamp, order_type, cloid, status,
    ))
}

/// Parse a `fundingHistory` JSON response into a [`Vec<HlFundingEntry>`].
///
/// Hyperliquid returns: `[{"coin": "BTC", "fundingRate": "0.0001", "premium": "0.00005", "time": 170...}, ...]`
pub(crate) fn parse_funding_history(
    resp: &serde_json::Value,
) -> Result<Vec<HlFundingEntry>, HlError> {
    let arr = resp
        .as_array()
        .ok_or_else(|| HlError::Parse("expected array for fundingHistory".into()))?;

    let mut entries = Vec::with_capacity(arr.len());
    for item in arr {
        let coin = item
            .get("coin")
            .and_then(|v| v.as_str())
            .ok_or_else(|| HlError::Parse("missing 'coin' in fundingHistory entry".into()))?
            .to_string();
        let funding_rate = parse_str_decimal(item.get("fundingRate"), "fundingRate")?;
        let premium = parse_str_decimal(item.get("premium"), "premium")?;
        let time = item
            .get("time")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| HlError::Parse("missing or invalid 'time' in fundingHistory".into()))?;
        entries.push(HlFundingEntry::new(coin, funding_rate, premium, time));
    }
    Ok(entries)
}

/// Parse a `userFunding` JSON response into a [`Vec<HlUserFundingEntry>`].
///
/// Hyperliquid returns: `[{"coin": "BTC", "usdc": "-1.5", "szi": "0.5", "fundingRate": "0.0001", "time": 170...}, ...]`
pub(crate) fn parse_user_funding(
    resp: &serde_json::Value,
) -> Result<Vec<HlUserFundingEntry>, HlError> {
    let arr = resp
        .as_array()
        .ok_or_else(|| HlError::Parse("expected array for userFunding".into()))?;

    let mut entries = Vec::with_capacity(arr.len());
    for item in arr {
        let coin = item
            .get("coin")
            .and_then(|v| v.as_str())
            .ok_or_else(|| HlError::Parse("missing 'coin' in userFunding entry".into()))?
            .to_string();
        let usdc = parse_str_decimal(item.get("usdc"), "usdc")?;
        let szi = parse_str_decimal(item.get("szi"), "szi")?;
        let funding_rate = parse_str_decimal(item.get("fundingRate"), "fundingRate")?;
        let time = item
            .get("time")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| HlError::Parse("missing or invalid 'time' in userFunding".into()))?;
        entries.push(HlUserFundingEntry::new(coin, usdc, szi, funding_rate, time));
    }
    Ok(entries)
}

/// Parse a `historicalOrders` JSON response into a [`Vec<HlHistoricalOrder>`].
///
/// Each entry has the same fields as open orders, plus a `status` field.
pub(crate) fn parse_historical_orders(
    resp: &serde_json::Value,
) -> Result<Vec<HlHistoricalOrder>, HlError> {
    let arr = resp
        .as_array()
        .ok_or_else(|| HlError::Parse("expected array for historicalOrders".into()))?;

    let mut orders = Vec::with_capacity(arr.len());
    for item in arr {
        let (oid, coin, side, limit_px, sz, timestamp, order_type, cloid) =
            parse_order_fields(item, "historicalOrder")?;
        let status = item
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        orders.push(HlHistoricalOrder::new(
            oid, coin, side, limit_px, sz, timestamp, order_type, cloid, status,
        ));
    }
    Ok(orders)
}

/// Parse a `userRateLimit` JSON response into an [`HlRateLimitStatus`].
///
/// The API returns something like:
///   `{"cumVlm": "...", "nRequestsUsed": 42, "nRequestsCap": 1200, ...}`
pub(crate) fn parse_rate_limit_status(
    resp: &serde_json::Value,
) -> Result<HlRateLimitStatus, HlError> {
    let used = resp
        .get("nRequestsUsed")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            HlError::Parse("missing or invalid 'nRequestsUsed' in userRateLimit".into())
        })?;

    let limit = resp
        .get("nRequestsCap")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            HlError::Parse("missing or invalid 'nRequestsCap' in userRateLimit".into())
        })?;

    let window_ms = resp
        .get("windowMs")
        .and_then(|v| v.as_u64())
        .unwrap_or(60_000); // default 60s window if not provided

    Ok(HlRateLimitStatus::new(used, limit, window_ms))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn make_clearinghouse_resp() -> serde_json::Value {
        serde_json::json!({
            "marginSummary": {
                "accountValue": "50000.00",
                "totalMarginUsed": "10000.00",
                "totalRawUsd": "40000.00"
            },
            "assetPositions": [
                {
                    "position": {
                        "coin": "BTC",
                        "szi": "0.5",
                        "entryPx": "60000.0",
                        "unrealizedPnl": "-200.0",
                        "leverage": { "type": "cross", "value": 5 },
                        "liquidationPx": "55000.0"
                    }
                },
                {
                    "position": {
                        "coin": "ETH",
                        "szi": "-2.0",
                        "entryPx": "3000.0",
                        "unrealizedPnl": "100.0",
                        "leverage": { "type": "cross", "value": 3 },
                        "liquidationPx": null
                    }
                },
                {
                    "position": {
                        "coin": "DOGE",
                        "szi": "0.0",
                        "entryPx": "0.1",
                        "unrealizedPnl": "0.0",
                        "leverage": { "type": "cross", "value": 1 }
                    }
                }
            ]
        })
    }

    #[test]
    fn parse_account_state_equity() {
        let resp = make_clearinghouse_resp();
        let state = parse_account_state(&resp).unwrap();
        assert_eq!(state.equity, Decimal::from_str("50000.00").unwrap());
    }

    #[test]
    fn parse_account_state_skips_zero_size() {
        let resp = make_clearinghouse_resp();
        let state = parse_account_state(&resp).unwrap();
        // DOGE has szi=0.0 and should be skipped
        assert_eq!(state.positions.len(), 2);
        assert!(!state.positions.iter().any(|p| p.coin == "DOGE"));
    }

    #[test]
    fn parse_account_state_btc_position() {
        let resp = make_clearinghouse_resp();
        let state = parse_account_state(&resp).unwrap();
        let btc = state.positions.iter().find(|p| p.coin == "BTC").unwrap();
        assert_eq!(btc.size, Decimal::from_str("0.5").unwrap());
        assert_eq!(btc.entry_px, Decimal::from_str("60000.0").unwrap());
        assert_eq!(btc.unrealized_pnl, Decimal::from_str("-200.0").unwrap());
        assert_eq!(btc.leverage, Decimal::from_str("5").unwrap());
        assert_eq!(
            btc.liquidation_px,
            Some(Decimal::from_str("55000.0").unwrap())
        );
    }

    #[test]
    fn parse_account_state_eth_position_no_liquidation() {
        let resp = make_clearinghouse_resp();
        let state = parse_account_state(&resp).unwrap();
        let eth = state.positions.iter().find(|p| p.coin == "ETH").unwrap();
        assert_eq!(eth.size, Decimal::from_str("-2.0").unwrap());
        assert!(eth.liquidation_px.is_none());
    }

    #[test]
    fn parse_fills_basic() {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let resp = serde_json::json!([
            {
                "coin": "BTC",
                "px": "60100.5",
                "sz": "0.1",
                "side": "B",
                "time": now_ms,
                "fee": "1.50",
                "closedPnl": "0.0"
            },
            {
                "coin": "ETH",
                "px": "3010.0",
                "sz": "1.0",
                "side": "A",
                "time": now_ms,
                "fee": "0.75",
                "closedPnl": "-50.0"
            }
        ]);

        let fills = parse_fills(&resp).unwrap();
        assert_eq!(fills.len(), 2);

        let btc = &fills[0];
        assert_eq!(btc.coin, "BTC");
        assert_eq!(btc.px, Decimal::from_str("60100.5").unwrap());
        assert_eq!(btc.sz, Decimal::from_str("0.1").unwrap());
        assert!(btc.is_buy);
        assert_eq!(btc.timestamp, now_ms);
        assert_eq!(btc.fee, Decimal::from_str("1.50").unwrap());
        assert_eq!(btc.closed_pnl, Decimal::ZERO);

        let eth = &fills[1];
        assert_eq!(eth.coin, "ETH");
        assert!(!eth.is_buy);
        assert_eq!(eth.closed_pnl, Decimal::from_str("-50.0").unwrap());
    }

    #[test]
    fn parse_fills_expects_array() {
        let resp = serde_json::json!({"not": "an array"});
        assert!(parse_fills(&resp).is_err());
    }

    #[test]
    fn parse_fills_skips_missing_coin() {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let resp = serde_json::json!([
            { "coin": "", "px": "100.0", "sz": "1.0", "side": "B", "time": now_ms, "fee": "0", "closedPnl": "0" },
            { "coin": "SOL", "px": "150.0", "sz": "2.0", "side": "A", "time": now_ms, "fee": "0", "closedPnl": "10.0" }
        ]);
        let fills = parse_fills(&resp).unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].coin, "SOL");
    }

    #[test]
    fn parse_account_state_missing_margin_summary_errors() {
        let resp = serde_json::json!({"assetPositions": []});
        let err = parse_account_state(&resp).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("marginSummary"),
            "should mention missing field: {msg}"
        );
    }

    #[test]
    fn parse_account_state_unparseable_equity_errors() {
        let resp = serde_json::json!({
            "marginSummary": {
                "accountValue": "not_a_number",
                "totalRawUsd": "100.0"
            },
            "assetPositions": []
        });
        assert!(parse_account_state(&resp).is_err());
    }

    #[test]
    fn parse_account_state_unparseable_entry_px_errors() {
        let resp = serde_json::json!({
            "marginSummary": {
                "accountValue": "1000.0",
                "totalRawUsd": "500.0"
            },
            "assetPositions": [{
                "position": {
                    "coin": "BTC",
                    "szi": "1.0",
                    "entryPx": "garbage",
                    "unrealizedPnl": "0.0",
                    "leverage": {"type": "cross", "value": 1}
                }
            }]
        });
        assert!(parse_account_state(&resp).is_err());
    }

    #[test]
    fn parse_fills_unparseable_price_errors() {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let resp = serde_json::json!([{
            "coin": "BTC",
            "px": "not_valid",
            "sz": "1.0",
            "side": "B",
            "time": now_ms,
            "fee": "0",
            "closedPnl": "0"
        }]);
        assert!(parse_fills(&resp).is_err());
    }

    #[test]
    fn parse_fills_missing_time_errors() {
        let resp = serde_json::json!([{
            "coin": "BTC",
            "px": "100.0",
            "sz": "1.0",
            "side": "B",
            "fee": "0",
            "closedPnl": "0"
            // "time" missing
        }]);
        assert!(parse_fills(&resp).is_err());
    }

    #[test]
    fn parse_spot_state_valid() {
        let resp = serde_json::json!({
            "balances": [
                { "coin": "PURR", "token": 1, "hold": "0", "total": "1000.0" },
                { "coin": "USDC", "token": 2, "hold": "50.0", "total": "500.0" }
            ]
        });
        let balances = parse_spot_state(&resp).unwrap();
        assert_eq!(balances.len(), 2);
        assert_eq!(balances[0].coin, "PURR");
        assert_eq!(balances[0].token, 1);
        assert_eq!(balances[0].hold, Decimal::ZERO);
        assert_eq!(balances[0].total, Decimal::from_str("1000.0").unwrap());
        assert_eq!(balances[1].coin, "USDC");
        assert_eq!(balances[1].token, 2);
        assert_eq!(balances[1].hold, Decimal::from_str("50.0").unwrap());
        assert_eq!(balances[1].total, Decimal::from_str("500.0").unwrap());
    }

    #[test]
    fn parse_spot_state_empty_balances() {
        let resp = serde_json::json!({ "balances": [] });
        let balances = parse_spot_state(&resp).unwrap();
        assert!(balances.is_empty());
    }

    #[test]
    fn parse_spot_state_missing_balances_errors() {
        let resp = serde_json::json!({});
        assert!(parse_spot_state(&resp).is_err());
    }

    #[test]
    fn parse_spot_state_skips_empty_coin() {
        let resp = serde_json::json!({
            "balances": [
                { "coin": "", "token": 0, "hold": "0", "total": "0" },
                { "coin": "PURR", "token": 1, "hold": "0", "total": "100.0" }
            ]
        });
        let balances = parse_spot_state(&resp).unwrap();
        assert_eq!(balances.len(), 1);
        assert_eq!(balances[0].coin, "PURR");
    }

    #[test]
    fn parse_spot_state_missing_token_errors() {
        let resp = serde_json::json!({
            "balances": [
                { "coin": "PURR", "hold": "0", "total": "100.0" }
            ]
        });
        assert!(parse_spot_state(&resp).is_err());
    }

    #[test]
    fn parse_staking_delegations_basic() {
        let resp = serde_json::json!([
            { "validator": "0xval1", "amount": "1000.0", "rewards": "5.0" },
            { "validator": "0xval2", "amount": "2000.0", "rewards": "10.5" }
        ]);
        let delegations = parse_staking_delegations(&resp).unwrap();
        assert_eq!(delegations.len(), 2);
        assert_eq!(delegations[0].validator, "0xval1");
        assert_eq!(delegations[0].amount, Decimal::from_str("1000.0").unwrap());
        assert_eq!(delegations[0].rewards, Decimal::from_str("5.0").unwrap());
        assert_eq!(delegations[1].validator, "0xval2");
        assert_eq!(delegations[1].amount, Decimal::from_str("2000.0").unwrap());
        assert_eq!(delegations[1].rewards, Decimal::from_str("10.5").unwrap());
    }

    #[test]
    fn parse_staking_delegations_empty() {
        let resp = serde_json::json!([]);
        let delegations = parse_staking_delegations(&resp).unwrap();
        assert!(delegations.is_empty());
    }

    #[test]
    fn parse_staking_delegations_expects_array() {
        let resp = serde_json::json!({"not": "an array"});
        assert!(parse_staking_delegations(&resp).is_err());
    }

    #[test]
    fn parse_staking_delegations_skips_empty_validator() {
        let resp = serde_json::json!([
            { "validator": "", "amount": "100.0", "rewards": "1.0" },
            { "validator": "0xval1", "amount": "200.0", "rewards": "2.0" }
        ]);
        let delegations = parse_staking_delegations(&resp).unwrap();
        assert_eq!(delegations.len(), 1);
        assert_eq!(delegations[0].validator, "0xval1");
    }

    #[test]
    fn parse_staking_delegations_missing_amount_errors() {
        let resp = serde_json::json!([
            { "validator": "0xval1", "rewards": "1.0" }
        ]);
        assert!(parse_staking_delegations(&resp).is_err());
    }

    #[test]
    fn parse_staking_delegations_missing_rewards_errors() {
        let resp = serde_json::json!([
            { "validator": "0xval1", "amount": "100.0" }
        ]);
        assert!(parse_staking_delegations(&resp).is_err());
    }

    #[test]
    fn parse_borrow_lend_state_basic() {
        let resp = serde_json::json!({
            "balances": [
                {
                    "coin": "USDC",
                    "supply": "10000.0",
                    "borrow": "0.0",
                    "apy": "0.05"
                },
                {
                    "coin": "ETH",
                    "supply": "0.0",
                    "borrow": "5.0",
                    "apy": "0.08"
                }
            ]
        });
        let states = parse_borrow_lend_state(&resp).unwrap();
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].coin, "USDC");
        assert_eq!(states[0].supply, Decimal::from_str("10000.0").unwrap());
        assert_eq!(states[0].borrow, Decimal::ZERO);
        assert_eq!(states[0].apy, Decimal::from_str("0.05").unwrap());
        assert_eq!(states[1].coin, "ETH");
        assert_eq!(states[1].supply, Decimal::ZERO);
        assert_eq!(states[1].borrow, Decimal::from_str("5.0").unwrap());
        assert_eq!(states[1].apy, Decimal::from_str("0.08").unwrap());
    }

    #[test]
    fn parse_borrow_lend_state_skips_plain_balances() {
        let resp = serde_json::json!({
            "balances": [
                { "coin": "PURR", "token": 1, "hold": "0", "total": "1000.0" },
                {
                    "coin": "USDC",
                    "supply": "500.0",
                    "borrow": "0.0",
                    "apy": "0.03"
                }
            ]
        });
        let states = parse_borrow_lend_state(&resp).unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].coin, "USDC");
    }

    #[test]
    fn parse_borrow_lend_state_empty_balances() {
        let resp = serde_json::json!({ "balances": [] });
        let states = parse_borrow_lend_state(&resp).unwrap();
        assert!(states.is_empty());
    }

    #[test]
    fn parse_borrow_lend_state_missing_balances_errors() {
        let resp = serde_json::json!({});
        assert!(parse_borrow_lend_state(&resp).is_err());
    }

    #[test]
    fn parse_borrow_lend_state_skips_empty_coin() {
        let resp = serde_json::json!({
            "balances": [
                { "coin": "", "supply": "100.0", "borrow": "0.0", "apy": "0.01" },
                { "coin": "BTC", "supply": "1.0", "borrow": "0.0", "apy": "0.02" }
            ]
        });
        let states = parse_borrow_lend_state(&resp).unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].coin, "BTC");
    }

    #[test]
    fn parse_fills_missing_closed_pnl_defaults_to_zero() {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let resp = serde_json::json!([{
            "coin": "BTC",
            "px": "100.0",
            "sz": "1.0",
            "side": "B",
            "time": now_ms,
            "fee": "0.5"
            // "closedPnl" missing — should default to 0.0
        }]);
        let fills = parse_fills(&resp).unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].closed_pnl, Decimal::ZERO);
    }

    #[test]
    fn parse_user_fees_basic() {
        let resp = serde_json::json!({
            "userCrossRate": "0.0002",
            "userAddRate": "0.0005",
            "feeTier": "VIP1"
        });
        let fees = parse_user_fees(&resp).unwrap();
        assert_eq!(fees.fee_tier, "VIP1");
        assert_eq!(fees.maker_rate, Decimal::from_str("0.0002").unwrap());
        assert_eq!(fees.taker_rate, Decimal::from_str("0.0005").unwrap());
    }

    #[test]
    fn parse_user_fees_numeric_tier() {
        let resp = serde_json::json!({
            "userCrossRate": "0.0001",
            "userAddRate": "0.00035",
            "feeTier": 3
        });
        let fees = parse_user_fees(&resp).unwrap();
        assert_eq!(fees.fee_tier, "3");
        assert_eq!(fees.maker_rate, Decimal::from_str("0.0001").unwrap());
        assert_eq!(fees.taker_rate, Decimal::from_str("0.00035").unwrap());
    }

    #[test]
    fn parse_user_fees_missing_tier_defaults_empty() {
        let resp = serde_json::json!({
            "userCrossRate": "0.0002",
            "userAddRate": "0.0005"
        });
        let fees = parse_user_fees(&resp).unwrap();
        assert_eq!(fees.fee_tier, "");
    }

    #[test]
    fn parse_user_fees_missing_cross_rate_errors() {
        let resp = serde_json::json!({
            "userAddRate": "0.0005",
            "feeTier": "VIP1"
        });
        assert!(parse_user_fees(&resp).is_err());
    }

    #[test]
    fn parse_user_fees_missing_add_rate_errors() {
        let resp = serde_json::json!({
            "userCrossRate": "0.0002",
            "feeTier": "VIP1"
        });
        assert!(parse_user_fees(&resp).is_err());
    }

    #[test]
    fn parse_rate_limit_status_basic() {
        let resp = serde_json::json!({
            "cumVlm": "500000.0",
            "nRequestsUsed": 42,
            "nRequestsCap": 1200,
            "windowMs": 60000
        });
        let status = parse_rate_limit_status(&resp).unwrap();
        assert_eq!(status.used, 42);
        assert_eq!(status.limit, 1200);
        assert_eq!(status.window_ms, 60000);
    }

    #[test]
    fn parse_rate_limit_status_default_window() {
        let resp = serde_json::json!({
            "cumVlm": "100.0",
            "nRequestsUsed": 10,
            "nRequestsCap": 500
        });
        let status = parse_rate_limit_status(&resp).unwrap();
        assert_eq!(status.used, 10);
        assert_eq!(status.limit, 500);
        assert_eq!(status.window_ms, 60_000);
    }

    #[test]
    fn parse_rate_limit_status_missing_used_errors() {
        let resp = serde_json::json!({
            "nRequestsCap": 1200,
            "windowMs": 60000
        });
        assert!(parse_rate_limit_status(&resp).is_err());
    }

    #[test]
    fn parse_rate_limit_status_missing_cap_errors() {
        let resp = serde_json::json!({
            "nRequestsUsed": 42,
            "windowMs": 60000
        });
        assert!(parse_rate_limit_status(&resp).is_err());
    }

    // --- open_orders parser tests ---

    #[test]
    fn parse_open_orders_basic() {
        let resp = serde_json::json!([
            {
                "oid": 12345,
                "coin": "BTC",
                "side": "B",
                "limitPx": "60000.0",
                "sz": "0.5",
                "timestamp": 1700000000000_u64,
                "orderType": "Limit",
                "cloid": "my-order-1"
            },
            {
                "oid": 12346,
                "coin": "ETH",
                "side": "A",
                "limitPx": "3000.0",
                "sz": "2.0",
                "timestamp": 1700000000001_u64,
                "orderType": "Limit"
            }
        ]);
        let orders = parse_open_orders(&resp).unwrap();
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0].oid, 12345);
        assert_eq!(orders[0].coin, "BTC");
        assert_eq!(orders[0].side, "B");
        assert_eq!(orders[0].limit_px, Decimal::from_str("60000.0").unwrap());
        assert_eq!(orders[0].sz, Decimal::from_str("0.5").unwrap());
        assert_eq!(orders[0].timestamp, 1700000000000);
        assert_eq!(orders[0].order_type, "Limit");
        assert_eq!(orders[0].cloid.as_deref(), Some("my-order-1"));
        assert_eq!(orders[1].oid, 12346);
        assert_eq!(orders[1].coin, "ETH");
        assert!(orders[1].cloid.is_none());
    }

    #[test]
    fn parse_open_orders_empty() {
        let resp = serde_json::json!([]);
        let orders = parse_open_orders(&resp).unwrap();
        assert!(orders.is_empty());
    }

    #[test]
    fn parse_open_orders_expects_array() {
        let resp = serde_json::json!({"not": "an array"});
        assert!(parse_open_orders(&resp).is_err());
    }

    #[test]
    fn parse_open_orders_missing_oid_errors() {
        let resp = serde_json::json!([{
            "coin": "BTC", "side": "B", "limitPx": "100.0",
            "sz": "1.0", "timestamp": 0
        }]);
        assert!(parse_open_orders(&resp).is_err());
    }

    // --- order_status parser tests ---

    #[test]
    fn parse_order_status_wrapped() {
        let resp = serde_json::json!({
            "order": {
                "oid": 555,
                "coin": "SOL",
                "side": "B",
                "limitPx": "150.0",
                "sz": "10.0",
                "timestamp": 1700000000000_u64,
                "orderType": "Limit"
            },
            "status": "filled"
        });
        let detail = parse_order_status(&resp).unwrap();
        assert_eq!(detail.oid, 555);
        assert_eq!(detail.coin, "SOL");
        assert_eq!(detail.status, "filled");
        assert_eq!(detail.limit_px, Decimal::from_str("150.0").unwrap());
    }

    #[test]
    fn parse_order_status_flat() {
        let resp = serde_json::json!({
            "oid": 100,
            "coin": "BTC",
            "side": "A",
            "limitPx": "60000.0",
            "sz": "0.1",
            "timestamp": 1700000000000_u64,
            "orderType": "Limit",
            "status": "open"
        });
        let detail = parse_order_status(&resp).unwrap();
        assert_eq!(detail.oid, 100);
        assert_eq!(detail.status, "open");
    }

    #[test]
    fn parse_order_status_missing_fields_errors() {
        let resp = serde_json::json!({"status": "error"});
        assert!(parse_order_status(&resp).is_err());
    }

    // --- funding_history parser tests ---

    #[test]
    fn parse_funding_history_basic() {
        let resp = serde_json::json!([
            {
                "coin": "BTC",
                "fundingRate": "0.0001",
                "premium": "0.00005",
                "time": 1700000000000_u64
            },
            {
                "coin": "ETH",
                "fundingRate": "-0.0002",
                "premium": "0.0001",
                "time": 1700000000001_u64
            }
        ]);
        let entries = parse_funding_history(&resp).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].coin, "BTC");
        assert_eq!(
            entries[0].funding_rate,
            Decimal::from_str("0.0001").unwrap()
        );
        assert_eq!(entries[0].premium, Decimal::from_str("0.00005").unwrap());
        assert_eq!(entries[0].time, 1700000000000);
        assert_eq!(entries[1].coin, "ETH");
        assert_eq!(
            entries[1].funding_rate,
            Decimal::from_str("-0.0002").unwrap()
        );
    }

    #[test]
    fn parse_funding_history_empty() {
        let resp = serde_json::json!([]);
        let entries = parse_funding_history(&resp).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_funding_history_expects_array() {
        let resp = serde_json::json!({"not": "an array"});
        assert!(parse_funding_history(&resp).is_err());
    }

    #[test]
    fn parse_funding_history_missing_funding_rate_errors() {
        let resp = serde_json::json!([{
            "coin": "BTC", "premium": "0.0", "time": 0
        }]);
        assert!(parse_funding_history(&resp).is_err());
    }

    // --- user_funding parser tests ---

    #[test]
    fn parse_user_funding_basic() {
        let resp = serde_json::json!([
            {
                "coin": "BTC",
                "usdc": "-1.5",
                "szi": "0.5",
                "fundingRate": "0.0001",
                "time": 1700000000000_u64
            }
        ]);
        let entries = parse_user_funding(&resp).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].coin, "BTC");
        assert_eq!(entries[0].usdc, Decimal::from_str("-1.5").unwrap());
        assert_eq!(entries[0].szi, Decimal::from_str("0.5").unwrap());
        assert_eq!(
            entries[0].funding_rate,
            Decimal::from_str("0.0001").unwrap()
        );
        assert_eq!(entries[0].time, 1700000000000);
    }

    #[test]
    fn parse_user_funding_empty() {
        let resp = serde_json::json!([]);
        let entries = parse_user_funding(&resp).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_user_funding_expects_array() {
        let resp = serde_json::json!({"not": "an array"});
        assert!(parse_user_funding(&resp).is_err());
    }

    #[test]
    fn parse_user_funding_missing_usdc_errors() {
        let resp = serde_json::json!([{
            "coin": "BTC", "szi": "0.5", "fundingRate": "0.0001", "time": 0
        }]);
        assert!(parse_user_funding(&resp).is_err());
    }

    // --- historical_orders parser tests ---

    #[test]
    fn parse_historical_orders_basic() {
        let resp = serde_json::json!([
            {
                "oid": 777,
                "coin": "BTC",
                "side": "A",
                "limitPx": "65000.0",
                "sz": "0.1",
                "timestamp": 1700000000000_u64,
                "orderType": "Limit",
                "cloid": "hist-1",
                "status": "filled"
            },
            {
                "oid": 778,
                "coin": "ETH",
                "side": "B",
                "limitPx": "3000.0",
                "sz": "1.0",
                "timestamp": 1700000000001_u64,
                "orderType": "Limit",
                "status": "canceled"
            }
        ]);
        let orders = parse_historical_orders(&resp).unwrap();
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0].oid, 777);
        assert_eq!(orders[0].coin, "BTC");
        assert_eq!(orders[0].status, "filled");
        assert_eq!(orders[0].cloid.as_deref(), Some("hist-1"));
        assert_eq!(orders[1].oid, 778);
        assert_eq!(orders[1].status, "canceled");
        assert!(orders[1].cloid.is_none());
    }

    #[test]
    fn parse_historical_orders_empty() {
        let resp = serde_json::json!([]);
        let orders = parse_historical_orders(&resp).unwrap();
        assert!(orders.is_empty());
    }

    #[test]
    fn parse_historical_orders_expects_array() {
        let resp = serde_json::json!({"not": "an array"});
        assert!(parse_historical_orders(&resp).is_err());
    }

    #[test]
    fn parse_historical_orders_default_order_type() {
        let resp = serde_json::json!([{
            "oid": 1,
            "coin": "BTC",
            "side": "B",
            "limitPx": "100.0",
            "sz": "1.0",
            "timestamp": 0,
            "status": "filled"
        }]);
        let orders = parse_historical_orders(&resp).unwrap();
        assert_eq!(orders[0].order_type, "Limit");
    }
}
