//! Internal test utilities for motosan-hyperliquid.
//!
//! Provides a shared [`MockTransport`] and helper constructors so that every
//! crate in the workspace can write mock-based tests without duplicating the
//! boilerplate.

use async_trait::async_trait;
use hl_client::HttpTransport;
use hl_executor::meta_cache::AssetMetaCache;
use hl_executor::OrderExecutor;
use hl_signing::PrivateKeySigner;
use hl_types::{HlError, Signature};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Mock HTTP transport that returns pre-queued JSON responses in FIFO order.
///
/// By default `is_mainnet` returns `true`.  Use [`MockTransport::testnet`] for
/// a transport that reports testnet.
pub struct MockTransport {
    responses: Mutex<Vec<serde_json::Value>>,
    mainnet: bool,
}

impl MockTransport {
    /// Create a mainnet mock transport with the given response queue.
    pub fn new(responses: Vec<serde_json::Value>) -> Self {
        Self {
            responses: Mutex::new(responses),
            mainnet: true,
        }
    }

    /// Create a mock transport that reports as **mainnet**.
    pub fn mainnet(responses: Vec<serde_json::Value>) -> Self {
        Self::new(responses)
    }

    /// Create a mock transport that reports as **testnet**.
    pub fn testnet(responses: Vec<serde_json::Value>) -> Self {
        Self {
            responses: Mutex::new(responses),
            mainnet: false,
        }
    }
}

#[async_trait]
impl HttpTransport for MockTransport {
    async fn post_info(&self, _request: serde_json::Value) -> Result<serde_json::Value, HlError> {
        let mut queue = self.responses.lock().unwrap();
        if queue.is_empty() {
            return Err(HlError::http("no mock responses"));
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
        let mut queue = self.responses.lock().unwrap();
        if queue.is_empty() {
            return Err(HlError::http("no mock responses"));
        }
        Ok(queue.remove(0))
    }

    fn is_mainnet(&self) -> bool {
        self.mainnet
    }
}

/// Create a test signer from a deterministic private key.
pub fn test_signer() -> Box<dyn hl_signing::Signer> {
    Box::new(
        PrivateKeySigner::from_hex(
            "0x0000000000000000000000000000000000000000000000000000000000000001",
        )
        .unwrap(),
    )
}

/// Create an [`OrderExecutor`] backed by a [`MockTransport`] with a pre-built
/// asset-meta cache containing `BTC=0` and `ETH=1`.
pub fn test_executor(responses: Vec<serde_json::Value>) -> OrderExecutor {
    let mut name_to_idx = HashMap::new();
    name_to_idx.insert("BTC".to_string(), 0u32);
    name_to_idx.insert("ETH".to_string(), 1u32);
    let cache = AssetMetaCache::from_maps(name_to_idx, Default::default());
    OrderExecutor::with_meta_cache(
        Arc::new(MockTransport::new(responses)),
        test_signer(),
        "0x0000000000000000000000000000000000000001".to_string(),
        cache,
    )
}

/// Canned "ok" response suitable for action endpoints that return a generic
/// `{"status": "ok", "response": {"type": "default"}}`.
pub fn ok_response() -> serde_json::Value {
    serde_json::json!({"status": "ok", "response": {"type": "default"}})
}
