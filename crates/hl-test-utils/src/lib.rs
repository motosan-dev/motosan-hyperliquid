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
    /// Every request body sent through this transport, in order — the `info`
    /// query JSON for `post_info` and the `action` JSON for `post_action`. Used
    /// by tests to assert the exact outbound wire format.
    requests: Mutex<Vec<serde_json::Value>>,
    /// The `expires_after` argument captured for each `post_action` call, in
    /// lockstep with `requests` (the action object alone doesn't carry it — it's
    /// a top-level body field). Lets tests assert the action-expiry threading.
    expires_after: Mutex<Vec<Option<u64>>>,
    mainnet: bool,
}

impl MockTransport {
    /// Create a mainnet mock transport with the given response queue.
    pub fn new(responses: Vec<serde_json::Value>) -> Self {
        Self {
            responses: Mutex::new(responses),
            requests: Mutex::new(Vec::new()),
            expires_after: Mutex::new(Vec::new()),
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
            requests: Mutex::new(Vec::new()),
            expires_after: Mutex::new(Vec::new()),
            mainnet: false,
        }
    }

    /// All request bodies captured so far, in send order.
    pub fn requests(&self) -> Vec<serde_json::Value> {
        self.requests.lock().unwrap().clone()
    }

    /// The most recent request body, if any.
    pub fn last_request(&self) -> Option<serde_json::Value> {
        self.requests.lock().unwrap().last().cloned()
    }

    /// The `expires_after` of the most recent `post_action` call, if any.
    pub fn last_expires_after(&self) -> Option<u64> {
        self.expires_after.lock().unwrap().last().copied().flatten()
    }
}

#[async_trait]
impl HttpTransport for MockTransport {
    async fn post_info(&self, request: serde_json::Value) -> Result<serde_json::Value, HlError> {
        self.requests.lock().unwrap().push(request);
        let mut queue = self.responses.lock().unwrap();
        if queue.is_empty() {
            return Err(HlError::http("no mock responses"));
        }
        Ok(queue.remove(0))
    }

    async fn post_action(
        &self,
        action: serde_json::Value,
        _signature: &Signature,
        _nonce: u64,
        _vault_address: Option<&str>,
        expires_after: Option<u64>,
    ) -> Result<serde_json::Value, HlError> {
        self.requests.lock().unwrap().push(action);
        self.expires_after.lock().unwrap().push(expires_after);
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
    test_executor_capturing(responses).0
}

/// Like [`test_executor`] but also returns the [`MockTransport`] handle so tests
/// can assert the exact outbound wire format via [`MockTransport::requests`].
pub fn test_executor_capturing(
    responses: Vec<serde_json::Value>,
) -> (OrderExecutor, Arc<MockTransport>) {
    let mut name_to_idx = HashMap::new();
    name_to_idx.insert("BTC".to_string(), 0u32);
    name_to_idx.insert("ETH".to_string(), 1u32);
    let mut name_to_sz = HashMap::new();
    name_to_sz.insert("BTC".to_string(), 5u32);
    name_to_sz.insert("ETH".to_string(), 4u32);
    let cache = AssetMetaCache::from_maps(name_to_idx, name_to_sz);
    let transport = Arc::new(MockTransport::new(responses));
    let executor = OrderExecutor::with_meta_cache(
        transport.clone(),
        test_signer(),
        "0x0000000000000000000000000000000000000001".to_string(),
        cache,
    );
    (executor, transport)
}

/// Canned "ok" response suitable for action endpoints that return a generic
/// `{"status": "ok", "response": {"type": "default"}}`.
pub fn ok_response() -> serde_json::Value {
    serde_json::json!({"status": "ok", "response": {"type": "default"}})
}
