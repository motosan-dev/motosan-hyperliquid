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
    spot_token_to_index: HashMap<String, u32>,
    spot_token_to_sz_decimals: HashMap<String, u32>,
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
        let mut spot_token_to_index = HashMap::new();
        let mut spot_token_to_sz_decimals = HashMap::new();
        if let Ok(spot_meta) = client
            .post_info(serde_json::json!({"type": "spotMeta"}))
            .await
        {
            if let Some(tokens) = spot_meta["tokens"].as_array() {
                for token in tokens {
                    if let (Some(name), Some(index)) =
                        (token["name"].as_str(), token["index"].as_u64())
                    {
                        spot_token_to_index.insert(name.to_uppercase(), index as u32);
                        if let Some(sz_dec) = token["szDecimals"].as_u64() {
                            spot_token_to_sz_decimals.insert(name.to_uppercase(), sz_dec as u32);
                        }
                    }
                }
            }
        }

        Ok(Self {
            coin_to_index,
            coin_to_sz_decimals,
            spot_token_to_index,
            spot_token_to_sz_decimals,
        })
    }

    /// Create a cache from pre-built perp maps (useful for testing).
    ///
    /// Spot maps are initialized as empty for backward compatibility.
    pub fn from_maps(
        coin_to_index: HashMap<String, u32>,
        coin_to_sz_decimals: HashMap<String, u32>,
    ) -> Self {
        Self {
            coin_to_index,
            coin_to_sz_decimals,
            spot_token_to_index: HashMap::new(),
            spot_token_to_sz_decimals: HashMap::new(),
        }
    }

    /// Create a cache from pre-built perp and spot maps.
    pub fn from_maps_with_spot(
        coin_to_index: HashMap<String, u32>,
        coin_to_sz_decimals: HashMap<String, u32>,
        spot_token_to_index: HashMap<String, u32>,
        spot_token_to_sz_decimals: HashMap<String, u32>,
    ) -> Self {
        Self {
            coin_to_index,
            coin_to_sz_decimals,
            spot_token_to_index,
            spot_token_to_sz_decimals,
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

    /// Resolve a coin name to its asset index without uppercasing.
    ///
    /// The caller is responsible for passing a pre-normalized (uppercase) coin
    /// name. This avoids a redundant allocation on the hot path when the input
    /// is already known to be uppercase.
    pub(crate) fn asset_index_normalized(&self, coin: &str) -> Option<u32> {
        self.coin_to_index.get(coin).copied()
    }

    /// Look up the size-decimal precision without uppercasing.
    ///
    /// Same pre-normalization requirement as [`Self::asset_index_normalized`].
    #[allow(dead_code)]
    pub(crate) fn sz_decimals_normalized(&self, coin: &str) -> Option<u32> {
        self.coin_to_sz_decimals.get(coin).copied()
    }

    /// Resolve a spot token name to its token index.
    ///
    /// The token name is uppercased before lookup so `"purr"` and `"PURR"` both
    /// work.
    pub fn spot_asset_index(&self, token: &str) -> Option<u32> {
        self.spot_token_to_index.get(&token.to_uppercase()).copied()
    }

    /// Look up the size-decimal precision for a spot token.
    ///
    /// The token name is uppercased before lookup.
    pub fn spot_sz_decimals(&self, token: &str) -> Option<u32> {
        self.spot_token_to_sz_decimals
            .get(&token.to_uppercase())
            .copied()
    }

    /// Resolve a spot token name to its token index without uppercasing.
    ///
    /// The caller is responsible for passing a pre-normalized (uppercase) token
    /// name.
    pub(crate) fn spot_asset_index_normalized(&self, token: &str) -> Option<u32> {
        self.spot_token_to_index.get(token).copied()
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
    fn asset_index_normalized_exact_match() {
        let cache = test_cache();
        assert_eq!(cache.asset_index_normalized("BTC"), Some(0));
    }

    #[test]
    fn asset_index_normalized_wrong_case_fails() {
        let cache = test_cache();
        assert_eq!(cache.asset_index_normalized("btc"), None);
    }

    #[test]
    fn empty_cache() {
        let cache = AssetMetaCache::from_maps(HashMap::new(), HashMap::new());
        assert_eq!(cache.asset_index("BTC"), None);
        assert_eq!(cache.sz_decimals("BTC"), None);
    }

    fn test_cache_with_spot() -> AssetMetaCache {
        AssetMetaCache::from_maps_with_spot(
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
            [("PURR".to_string(), 10000), ("USDC".to_string(), 10001)].into(),
            [("PURR".to_string(), 0), ("USDC".to_string(), 6)].into(),
        )
    }

    #[test]
    fn spot_asset_index_exact() {
        let cache = test_cache_with_spot();
        assert_eq!(cache.spot_asset_index("PURR"), Some(10000));
        assert_eq!(cache.spot_asset_index("USDC"), Some(10001));
    }

    #[test]
    fn spot_asset_index_case_insensitive() {
        let cache = test_cache_with_spot();
        assert_eq!(cache.spot_asset_index("purr"), Some(10000));
    }

    #[test]
    fn spot_asset_index_not_found() {
        let cache = test_cache_with_spot();
        assert_eq!(cache.spot_asset_index("DOGE"), None);
    }

    #[test]
    fn spot_sz_decimals_lookup() {
        let cache = test_cache_with_spot();
        assert_eq!(cache.spot_sz_decimals("PURR"), Some(0));
        assert_eq!(cache.spot_sz_decimals("USDC"), Some(6));
    }

    #[test]
    fn spot_does_not_overlap_perp() {
        let cache = test_cache_with_spot();
        // BTC exists in perp but not spot
        assert_eq!(cache.asset_index("BTC"), Some(0));
        assert_eq!(cache.spot_asset_index("BTC"), None);
        // PURR exists in spot but not perp
        assert_eq!(cache.spot_asset_index("PURR"), Some(10000));
        assert_eq!(cache.asset_index("PURR"), None);
    }
}
