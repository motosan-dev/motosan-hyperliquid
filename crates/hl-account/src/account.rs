use std::str::FromStr;
use std::sync::Arc;

use rust_decimal::Decimal;

use hl_client::{HttpTransport, HyperliquidClient};
use hl_types::{
    HlAccountState, HlError, HlExtraAgent, HlFill, HlPosition, HlVaultDetails, HlVaultSummary,
};

/// Typed interface for Hyperliquid account state queries.
///
/// Wraps an [`HttpTransport`] and provides methods to fetch positions,
/// fills, vault information, and agent approvals for any public address.
pub struct Account {
    client: Arc<dyn HttpTransport>,
}

impl Account {
    /// Create a new `Account` instance wrapping an [`HttpTransport`].
    pub fn new(client: Arc<dyn HttpTransport>) -> Self {
        Self { client }
    }

    /// Convenience constructor that wraps a [`HyperliquidClient`] in an `Arc`.
    pub fn from_client(client: HyperliquidClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// Fetch the full clearinghouse state for an address.
    #[tracing::instrument(skip(self))]
    pub async fn state(&self, address: &str) -> Result<HlAccountState, HlError> {
        let payload = serde_json::json!({
            "type": "clearinghouseState",
            "user": address,
        });
        let resp = self.client.post_info(payload).await?;
        parse_account_state(&resp)
    }

    /// Fetch only the open positions for an address.
    #[tracing::instrument(skip(self))]
    pub async fn positions(&self, address: &str) -> Result<Vec<HlPosition>, HlError> {
        let state = self.state(address).await?;
        Ok(state.positions)
    }

    /// Fetch all fills (trade history) for an address.
    #[tracing::instrument(skip(self))]
    pub async fn fills(&self, address: &str) -> Result<Vec<HlFill>, HlError> {
        let payload = serde_json::json!({ "type": "userFills", "user": address });
        let resp = self.client.post_info(payload).await?;
        parse_fills(&resp)
    }

    /// Fetch vault summaries for an address.
    #[tracing::instrument(skip(self))]
    pub async fn vault_summaries(&self, address: &str) -> Result<Vec<HlVaultSummary>, HlError> {
        let payload = serde_json::json!({ "type": "vaultSummaries", "user": address });
        let resp = self.client.post_info(payload).await?;
        let arr = resp
            .as_array()
            .ok_or_else(|| HlError::Parse("expected array for vaultSummaries".into()))?;
        arr.iter()
            .map(|v| {
                serde_json::from_value(v.clone())
                    .map_err(|e| HlError::Parse(format!("vaultSummary: {e}")))
            })
            .collect()
    }

    /// Fetch details for a specific vault.
    #[tracing::instrument(skip(self))]
    pub async fn vault_details(
        &self,
        address: &str,
        vault: &str,
    ) -> Result<HlVaultDetails, HlError> {
        let payload = serde_json::json!({
            "type": "vaultDetails",
            "user": address,
            "vaultAddress": vault,
        });
        let resp = self.client.post_info(payload).await?;
        serde_json::from_value(resp).map_err(|e| HlError::Parse(format!("vaultDetails: {e}")))
    }

    /// Fetch extra (sub-)agent approvals for an address.
    #[tracing::instrument(skip(self))]
    pub async fn extra_agents(&self, address: &str) -> Result<Vec<HlExtraAgent>, HlError> {
        let payload = serde_json::json!({ "type": "extraAgents", "user": address });
        let resp = self.client.post_info(payload).await?;
        let arr = resp
            .as_array()
            .ok_or_else(|| HlError::Parse("expected array for extraAgents".into()))?;
        arr.iter()
            .map(|v| {
                serde_json::from_value(v.clone())
                    .map_err(|e| HlError::Parse(format!("extraAgent: {e}")))
            })
            .collect()
    }
}

/// Parse a string-encoded decimal from a JSON value, returning an error on failure.
fn parse_str_decimal(val: &serde_json::Value, field: &str) -> Result<Decimal, HlError> {
    match val {
        serde_json::Value::String(s) => Decimal::from_str(s).map_err(|_| {
            HlError::Parse(format!("cannot parse '{field}' value \"{s}\" as Decimal"))
        }),
        serde_json::Value::Number(n) => {
            let s = n.to_string();
            Decimal::from_str(&s)
                .map_err(|_| HlError::Parse(format!("cannot convert '{field}' number to Decimal")))
        }
        serde_json::Value::Null => Err(HlError::Parse(format!("field '{field}' is null"))),
        v => Err(HlError::Parse(format!(
            "unexpected type for '{field}': expected string or number, got {v}"
        ))),
    }
}

/// A small threshold used to detect zero-size (closed) positions.
const ZERO_SIZE_THRESHOLD: Decimal = Decimal::from_parts(1, 0, 0, false, 12); // 1e-12

/// Parse a `clearinghouseState` JSON response into an [`HlAccountState`].
///
/// Hyperliquid returns numeric fields as quoted strings, e.g. `"szi": "0.001"`.
/// Zero-size positions (|szi| < 1e-12) are skipped.
pub fn parse_account_state(resp: &serde_json::Value) -> Result<HlAccountState, HlError> {
    let margin_summary = resp
        .get("marginSummary")
        .ok_or_else(|| HlError::Parse("missing 'marginSummary' in clearinghouseState".into()))?;

    let equity: Decimal = parse_str_decimal(
        margin_summary
            .get("accountValue")
            .ok_or_else(|| HlError::Parse("missing 'accountValue' in marginSummary".into()))?,
        "accountValue",
    )?;

    let margin_available: Decimal = {
        let raw = margin_summary
            .get("totalRawUsd")
            .or_else(|| margin_summary.get("availableMargin"))
            .ok_or_else(|| {
                HlError::Parse(
                    "missing 'totalRawUsd' and 'availableMargin' in marginSummary".into(),
                )
            })?;
        parse_str_decimal(raw, "totalRawUsd/availableMargin")?
    };

    let mut positions = Vec::new();

    if let Some(asset_positions) = resp["assetPositions"].as_array() {
        for pos in asset_positions {
            let p = &pos["position"];

            // Size: parse with error propagation. A size of 0.0 is valid
            // (means the position is closed), so we skip it rather than error.
            let size: Decimal = parse_str_decimal(
                p.get("szi")
                    .ok_or_else(|| HlError::Parse("missing 'szi' in position".into()))?,
                "szi",
            )?;
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

            let entry_px: Decimal = parse_str_decimal(
                p.get("entryPx")
                    .ok_or_else(|| HlError::Parse("missing 'entryPx' in position".into()))?,
                "entryPx",
            )?;
            let unrealized_pnl: Decimal = parse_str_decimal(
                p.get("unrealizedPnl")
                    .ok_or_else(|| HlError::Parse("missing 'unrealizedPnl' in position".into()))?,
                "unrealizedPnl",
            )?;
            let leverage: Decimal = match p.get("leverage").and_then(|l| l.get("value")) {
                Some(v) => parse_str_decimal(v, "leverage.value")
                    // Leverage defaults to 1.0 if unparseable (cross-margin mode).
                    .unwrap_or(Decimal::ONE),
                None => Decimal::ONE,
            };
            let liquidation_px: Option<Decimal> = match p.get("liquidationPx") {
                Some(serde_json::Value::Null) | None => None,
                Some(v) => Some(parse_str_decimal(v, "liquidationPx")?),
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

        let px: Decimal = parse_str_decimal(
            fill.get("px")
                .ok_or_else(|| HlError::Parse("missing 'px' in fill".into()))?,
            "px",
        )?;
        let sz: Decimal = parse_str_decimal(
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
        let fee: Decimal = parse_str_decimal(
            fill.get("fee")
                .ok_or_else(|| HlError::Parse("missing 'fee' in fill".into()))?,
            "fee",
        )?;
        // closedPnl may be 0.0 legitimately (no realized PnL), default to 0.0 if missing.
        let closed_pnl: Decimal = match fill.get("closedPnl") {
            Some(v) => parse_str_decimal(v, "closedPnl")?,
            None => Decimal::ZERO,
        };

        fills.push(HlFill::new(coin, px, sz, is_buy, timestamp, fee, closed_pnl));
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
}
