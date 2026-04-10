use std::collections::HashMap;

use hl_client::HttpTransport;
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
    pub async fn load(client: &dyn HttpTransport) -> Result<Self, HlError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cache() -> AssetMetaCache {
        AssetMetaCache::from_maps(
            [
                ("BTC".to_string(), 0),
                ("ETH".to_string(), 1),
                ("SOL".to_string(), 2),
            ]
            .into(),
            [
                ("BTC".to_string(), 5),
                ("ETH".to_string(), 8),
                ("SOL".to_string(), 4),
            ]
            .into(),
        )
    }

    #[test]
    fn asset_index_exact_match() {
        let cache = test_cache();
        assert_eq!(cache.asset_index("BTC"), Some(0));
        assert_eq!(cache.asset_index("ETH"), Some(1));
        assert_eq!(cache.asset_index("SOL"), Some(2));
    }

    #[test]
    fn asset_index_case_insensitive() {
        let cache = test_cache();
        assert_eq!(cache.asset_index("btc"), Some(0));
        assert_eq!(cache.asset_index("Btc"), Some(0));
        assert_eq!(cache.asset_index("eth"), Some(1));
    }

    #[test]
    fn asset_index_not_found() {
        let cache = test_cache();
        assert_eq!(cache.asset_index("DOGE"), None);
        assert_eq!(cache.asset_index(""), None);
    }

    #[test]
    fn sz_decimals_lookup() {
        let cache = test_cache();
        assert_eq!(cache.sz_decimals("BTC"), Some(5));
        assert_eq!(cache.sz_decimals("ETH"), Some(8));
        assert_eq!(cache.sz_decimals("SOL"), Some(4));
    }

    #[test]
    fn sz_decimals_case_insensitive() {
        let cache = test_cache();
        assert_eq!(cache.sz_decimals("btc"), Some(5));
        assert_eq!(cache.sz_decimals("Eth"), Some(8));
    }

    #[test]
    fn sz_decimals_not_found() {
        let cache = test_cache();
        assert_eq!(cache.sz_decimals("UNKNOWN"), None);
    }

    #[test]
    fn empty_cache() {
        let cache = AssetMetaCache::from_maps(HashMap::new(), HashMap::new());
        assert_eq!(cache.asset_index("BTC"), None);
        assert_eq!(cache.sz_decimals("BTC"), None);
    }
}
