use std::str::FromStr;

use rust_decimal::Decimal;
use hl_types::HlError;

/// Parsed result from a single status entry.
pub(crate) struct ParsedStatus {
    pub oid: String,
    /// Average fill price (only present for filled orders).
    pub avg_px: Option<Decimal>,
    /// Total filled size (only present for filled orders).
    pub total_sz: Option<Decimal>,
    /// Whether this was a resting (unfilled) order.
    pub is_resting: bool,
}

/// Parse a single status entry from the statuses array.
fn parse_single_status(entry: &serde_json::Value) -> Result<ParsedStatus, HlError> {
    if let Some(filled) = entry.get("filled") {
        let oid = filled
            .get("oid")
            .and_then(|o| o.as_u64())
            .map(|o| o.to_string())
            .unwrap_or_else(|| {
                let fallback = uuid::Uuid::new_v4().to_string();
                tracing::warn!(
                    fallback_oid = %fallback,
                    "filled status missing oid, using generated UUID"
                );
                fallback
            });
        let avg_px = filled
            .get("avgPx")
            .and_then(|p| p.as_str())
            .and_then(|s| Decimal::from_str(s).ok());
        let total_sz = filled
            .get("totalSz")
            .and_then(|s| s.as_str())
            .and_then(|s| Decimal::from_str(s).ok());
        Ok(ParsedStatus {
            oid,
            avg_px,
            total_sz,
            is_resting: false,
        })
    } else if let Some(resting) = entry.get("resting") {
        let oid = resting
            .get("oid")
            .and_then(|o| o.as_u64())
            .map(|o| o.to_string())
            .unwrap_or_else(|| {
                let fallback = uuid::Uuid::new_v4().to_string();
                tracing::warn!(
                    fallback_oid = %fallback,
                    "resting status missing oid, using generated UUID"
                );
                fallback
            });
        Ok(ParsedStatus {
            oid,
            avg_px: None,
            total_sz: None,
            is_resting: true,
        })
    } else {
        Err(HlError::Parse(format!(
            "unrecognized order status format: {}",
            entry
        )))
    }
}

/// Resolve a [`ParsedStatus`] into a `(order_id, fill_price, fill_size)` tuple
/// using the provided fallback values.
fn resolve_status(
    status: ParsedStatus,
    fallback_price: Decimal,
    fallback_size: Decimal,
) -> (String, Decimal, Decimal) {
    if status.is_resting {
        (status.oid, fallback_price, Decimal::ZERO)
    } else {
        (
            status.oid,
            status.avg_px.unwrap_or(fallback_price),
            status.total_sz.unwrap_or(fallback_size),
        )
    }
}

/// Parse the order/fill information from a Hyperliquid exchange response.
///
/// Both regular orders and trigger orders return the same response structure
/// under `response.data.statuses[0]`. This helper extracts the order ID,
/// average fill price, and total fill size from either "filled" or "resting"
/// status entries.
pub(crate) fn parse_order_response(
    result: &serde_json::Value,
    fallback_price: Decimal,
    fallback_size: Decimal,
) -> Result<(String, Decimal, Decimal), HlError> {
    let status_entry = result
        .get("response")
        .and_then(|r| r.get("data"))
        .and_then(|d| d.get("statuses"))
        .and_then(|s| s.as_array())
        .and_then(|a| a.first());

    if let Some(entry) = status_entry {
        let parsed = parse_single_status(entry)?;
        Ok(resolve_status(parsed, fallback_price, fallback_size))
    } else {
        Err(HlError::Parse(
            "exchange returned ok but statuses array is empty".into(),
        ))
    }
}

/// Parse a bulk order response that may contain multiple statuses.
///
/// Returns a `Vec` of `(order_id, fill_price, fill_size)` tuples, one per
/// status entry. Uses the same fallback logic as [`parse_order_response`].
#[allow(dead_code)]
pub(crate) fn parse_bulk_order_response(
    result: &serde_json::Value,
    fallback_price: Decimal,
    fallback_size: Decimal,
) -> Result<Vec<(String, Decimal, Decimal)>, HlError> {
    let statuses = result
        .get("response")
        .and_then(|r| r.get("data"))
        .and_then(|d| d.get("statuses"))
        .and_then(|s| s.as_array())
        .ok_or_else(|| {
            HlError::Parse("exchange returned ok but statuses array is missing".into())
        })?;

    let mut out = Vec::with_capacity(statuses.len());
    for entry in statuses {
        let parsed = parse_single_status(entry)?;
        out.push(resolve_status(parsed, fallback_price, fallback_size));
    }
    Ok(out)
}

/// Parse a bulk order response using per-order fallback values.
///
/// Each entry in `fallbacks` is `(fallback_price, fallback_size)` corresponding
/// to the order at the same index. If there are more statuses than fallbacks,
/// extra statuses use `Decimal::ZERO` as fallback.
pub(crate) fn parse_bulk_order_response_with_fallbacks(
    result: &serde_json::Value,
    fallbacks: &[(Decimal, Decimal)],
) -> Result<Vec<(String, Decimal, Decimal)>, HlError> {
    let statuses = result
        .get("response")
        .and_then(|r| r.get("data"))
        .and_then(|d| d.get("statuses"))
        .and_then(|s| s.as_array())
        .ok_or_else(|| {
            HlError::Parse("exchange returned ok but statuses array is missing".into())
        })?;

    let mut out = Vec::with_capacity(statuses.len());
    for (i, entry) in statuses.iter().enumerate() {
        let parsed = parse_single_status(entry)?;
        let (fp, fs) = fallbacks
            .get(i)
            .copied()
            .unwrap_or((Decimal::ZERO, Decimal::ZERO));
        out.push(resolve_status(parsed, fp, fs));
    }
    Ok(out)
}
