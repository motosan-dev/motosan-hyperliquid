use hl_client::HyperliquidClient;
use hl_types::{HlAccountState, HlError, HlFill, HlPosition};

/// Typed interface for Hyperliquid account state queries.
///
/// Wraps a [`HyperliquidClient`] and provides methods to fetch positions,
/// fills, vault information, and agent approvals for any public address.
pub struct Account {
    client: HyperliquidClient,
}

impl Account {
    /// Create a new `Account` instance wrapping the given client.
    pub fn new(client: HyperliquidClient) -> Self {
        Self { client }
    }

    /// Fetch the full clearinghouse state for an address.
    pub async fn state(&self, address: &str) -> Result<HlAccountState, HlError> {
        let payload = serde_json::json!({
            "type": "clearinghouseState",
            "user": address,
        });
        let resp = self.client.post_info(payload).await?;
        parse_account_state(&resp)
    }

    /// Fetch only the open positions for an address.
    pub async fn positions(&self, address: &str) -> Result<Vec<HlPosition>, HlError> {
        let state = self.state(address).await?;
        Ok(state.positions)
    }

    /// Fetch all fills (trade history) for an address.
    pub async fn fills(&self, address: &str) -> Result<Vec<HlFill>, HlError> {
        let payload = serde_json::json!({ "type": "userFills", "user": address });
        let resp = self.client.post_info(payload).await?;
        parse_fills(&resp)
    }

    /// Fetch vault summaries for an address.
    pub async fn vault_summaries(&self, address: &str) -> Result<Vec<serde_json::Value>, HlError> {
        let payload = serde_json::json!({ "type": "vaultSummaries", "user": address });
        let resp = self.client.post_info(payload).await?;
        resp.as_array()
            .cloned()
            .ok_or_else(|| HlError::Parse("expected array for vaultSummaries".into()))
    }

    /// Fetch details for a specific vault.
    pub async fn vault_details(
        &self,
        address: &str,
        vault: &str,
    ) -> Result<serde_json::Value, HlError> {
        let payload = serde_json::json!({
            "type": "vaultDetails",
            "user": address,
            "vaultAddress": vault,
        });
        self.client.post_info(payload).await
    }

    /// Fetch extra (sub-)agent approvals for an address.
    pub async fn extra_agents(&self, address: &str) -> Result<Vec<serde_json::Value>, HlError> {
        let payload = serde_json::json!({ "type": "extraAgents", "user": address });
        let resp = self.client.post_info(payload).await?;
        resp.as_array()
            .cloned()
            .ok_or_else(|| HlError::Parse("expected array for extraAgents".into()))
    }
}

/// Parse a string-encoded float from a JSON value, returning an error on failure.
fn parse_str_f64(val: &serde_json::Value, field: &str) -> Result<f64, HlError> {
    match val {
        serde_json::Value::String(s) => s
            .parse::<f64>()
            .map_err(|_| HlError::Parse(format!("cannot parse '{field}' value \"{s}\" as f64"))),
        serde_json::Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| HlError::Parse(format!("cannot convert '{field}' number to f64"))),
        serde_json::Value::Null => Err(HlError::Parse(format!("field '{field}' is null"))),
        v => Err(HlError::Parse(format!(
            "unexpected type for '{field}': expected string or number, got {v}"
        ))),
    }
}

/// Parse a `clearinghouseState` JSON response into an [`HlAccountState`].
///
/// Hyperliquid returns numeric fields as quoted strings, e.g. `"szi": "0.001"`.
/// Zero-size positions (|szi| < 1e-12) are skipped.
pub fn parse_account_state(resp: &serde_json::Value) -> Result<HlAccountState, HlError> {
    let margin_summary = resp
        .get("marginSummary")
        .ok_or_else(|| HlError::Parse("missing 'marginSummary' in clearinghouseState".into()))?;

    let equity: f64 = parse_str_f64(
        margin_summary
            .get("accountValue")
            .ok_or_else(|| HlError::Parse("missing 'accountValue' in marginSummary".into()))?,
        "accountValue",
    )?;

    let margin_available: f64 = {
        let raw = margin_summary
            .get("totalRawUsd")
            .or_else(|| margin_summary.get("availableMargin"))
            .ok_or_else(|| {
                HlError::Parse(
                    "missing 'totalRawUsd' and 'availableMargin' in marginSummary".into(),
                )
            })?;
        parse_str_f64(raw, "totalRawUsd/availableMargin")?
    };

    let mut positions = Vec::new();

    if let Some(asset_positions) = resp["assetPositions"].as_array() {
        for pos in asset_positions {
            let p = &pos["position"];

            // Size: parse with error propagation. A size of 0.0 is valid
            // (means the position is closed), so we skip it rather than error.
            let size: f64 = parse_str_f64(
                p.get("szi")
                    .ok_or_else(|| HlError::Parse("missing 'szi' in position".into()))?,
                "szi",
            )?;
            if size.abs() < 1e-12 {
                continue;
            }

            let coin = match p.get("coin").and_then(|v| v.as_str()) {
                Some(c) if !c.is_empty() => c.to_string(),
                _ => {
                    tracing::warn!("Skipping position with missing or empty coin field");
                    continue;
                }
            };

            let entry_px: f64 = parse_str_f64(
                p.get("entryPx")
                    .ok_or_else(|| HlError::Parse("missing 'entryPx' in position".into()))?,
                "entryPx",
            )?;
            let unrealized_pnl: f64 = parse_str_f64(
                p.get("unrealizedPnl")
                    .ok_or_else(|| HlError::Parse("missing 'unrealizedPnl' in position".into()))?,
                "unrealizedPnl",
            )?;
            let leverage: f64 = match p.get("leverage").and_then(|l| l.get("value")) {
                Some(v) => parse_str_f64(v, "leverage.value")
                    // Leverage defaults to 1.0 if unparseable (cross-margin mode).
                    .unwrap_or(1.0),
                None => 1.0,
            };
            let liquidation_px: Option<f64> = match p.get("liquidationPx") {
                Some(serde_json::Value::Null) | None => None,
                Some(v) => Some(parse_str_f64(v, "liquidationPx")?),
            };

            positions.push(HlPosition {
                coin,
                size,
                entry_px,
                unrealized_pnl,
                leverage,
                liquidation_px,
            });
        }
    }

    Ok(HlAccountState {
        equity,
        margin_available,
        positions,
    })
}

/// Parse a `userFills` JSON response into a [`Vec<HlFill>`].
///
/// Hyperliquid returns numeric fields as quoted strings.
/// The `side` field is `"B"` (buy) or `"A"` (ask/sell).
pub fn parse_fills(resp: &serde_json::Value) -> Result<Vec<HlFill>, HlError> {
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

        let px: f64 = parse_str_f64(
            fill.get("px")
                .ok_or_else(|| HlError::Parse("missing 'px' in fill".into()))?,
            "px",
        )?;
        let sz: f64 = parse_str_f64(
            fill.get("sz")
                .ok_or_else(|| HlError::Parse("missing 'sz' in fill".into()))?,
            "sz",
        )?;
        let is_buy = fill
            .get("side")
            .and_then(|v| v.as_str())
            .ok_or_else(|| HlError::Parse("missing 'side' in fill".into()))?
            == "B";
        let timestamp: u64 = fill
            .get("time")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| HlError::Parse("missing or invalid 'time' in fill".into()))?;
        let fee: f64 = parse_str_f64(
            fill.get("fee")
                .ok_or_else(|| HlError::Parse("missing 'fee' in fill".into()))?,
            "fee",
        )?;
        // closedPnl may be 0.0 legitimately (no realized PnL), default to 0.0 if missing.
        let closed_pnl: f64 = match fill.get("closedPnl") {
            Some(v) => parse_str_f64(v, "closedPnl")?,
            None => 0.0,
        };

        fills.push(HlFill {
            coin,
            px,
            sz,
            is_buy,
            timestamp,
            fee,
            closed_pnl,
        });
    }

    Ok(fills)
}

#[cfg(test)]
mod tests {
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
        assert_eq!(state.equity, 50000.0);
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
        assert!((btc.size - 0.5).abs() < 1e-9);
        assert!((btc.entry_px - 60000.0).abs() < 1e-9);
        assert!((btc.unrealized_pnl - (-200.0)).abs() < 1e-9);
        assert!((btc.leverage - 5.0).abs() < 1e-9);
        assert_eq!(btc.liquidation_px, Some(55000.0));
    }

    #[test]
    fn parse_account_state_eth_position_no_liquidation() {
        let resp = make_clearinghouse_resp();
        let state = parse_account_state(&resp).unwrap();
        let eth = state.positions.iter().find(|p| p.coin == "ETH").unwrap();
        assert!((eth.size - (-2.0)).abs() < 1e-9);
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
        assert!((btc.px - 60100.5).abs() < 1e-9);
        assert!((btc.sz - 0.1).abs() < 1e-9);
        assert!(btc.is_buy);
        assert_eq!(btc.timestamp, now_ms);
        assert!((btc.fee - 1.50).abs() < 1e-9);
        assert!((btc.closed_pnl - 0.0).abs() < 1e-9);

        let eth = &fills[1];
        assert_eq!(eth.coin, "ETH");
        assert!(!eth.is_buy);
        assert!((eth.closed_pnl - (-50.0)).abs() < 1e-9);
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
        assert!((fills[0].closed_pnl - 0.0).abs() < 1e-9);
    }
}
