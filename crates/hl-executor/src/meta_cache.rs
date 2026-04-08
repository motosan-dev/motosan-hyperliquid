use std::collections::HashMap;

use hl_client::HyperliquidClient;
use hl_types::HlError;

/// Cache of asset name -> index and size-decimal mappings from the Hyperliquid
/// exchange meta endpoint.
///
/// Used by [`crate::OrderExecutor`] to resolve human-readable coin names (e.g.
/// "BTC") into the numeric asset indices required by the L1 order action, and
/// to look up the `szDecimals` for correct size formatting.
#[derive(Clone, Debug)]
pub struct AssetMetaCache {
    coin_to_index: HashMap<String, u32>,
    coin_to_sz_decimals: HashMap<String, u32>,
}

impl AssetMetaCache {
    /// Fetch asset metadata from the exchange info endpoint and build the cache.
    pub async fn load(client: &HyperliquidClient) -> Result<Self, HlError> {
        let meta = client
            .post_info(serde_json::json!({"type": "meta"}))
            .await?;
        let universe = meta["universe"]
            .as_array()
            .ok_or_else(|| HlError::Parse("meta response missing universe".into()))?;

        let mut coin_to_index = HashMap::new();
        let mut coin_to_sz_decimals = HashMap::new();
        for (i, asset) in universe.iter().enumerate() {
            if let Some(name) = asset["name"].as_str() {
                coin_to_index.insert(name.to_uppercase(), i as u32);
                if let Some(sz_dec) = asset["szDecimals"].as_u64() {
                    coin_to_sz_decimals.insert(name.to_uppercase(), sz_dec as u32);
                }
            }
        }
        Ok(Self {
            coin_to_index,
            coin_to_sz_decimals,
        })
    }

    /// Create a cache from pre-built maps (useful for testing).
    pub fn from_maps(
        coin_to_index: HashMap<String, u32>,
        coin_to_sz_decimals: HashMap<String, u32>,
    ) -> Self {
        Self {
            coin_to_index,
            coin_to_sz_decimals,
        }
    }

    /// Resolve a coin name to its asset index.
    ///
    /// The coin is uppercased before lookup so `"btc"` and `"BTC"` both work.
    pub fn asset_index(&self, coin: &str) -> Option<u32> {
        self.coin_to_index.get(&coin.to_uppercase()).copied()
    }

    /// Look up the size-decimal precision for a coin.
    pub fn sz_decimals(&self, coin: &str) -> Option<u32> {
        self.coin_to_sz_decimals.get(&coin.to_uppercase()).copied()
    }
}
