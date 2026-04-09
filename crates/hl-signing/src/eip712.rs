//! Hyperliquid EIP-712 signing delegated to motosan-wallet-core.
//!
//! Public API signatures are preserved for backward compatibility.
//! Internally, a [`SingleAddressSigner`](crate::SingleAddressSigner)
//! bridges the hl-signing [`Signer`](crate::Signer) trait to motosan-wallet-core's
//! [`HlSigner`] so the actual EIP-712 logic lives in one place.

use crate::Signer;
use crate::SingleAddressSigner;
use hl_types::{HlError, Signature};

use motosan_wallet_core::HlTypeField;

// ============================================================
// EIP-712 Field Descriptor (public API, unchanged)
// ============================================================

/// Describes a single field in an EIP-712 struct type.
#[derive(Debug, Clone)]
pub struct EIP712Field {
    pub name: String,
    pub field_type: String,
}

impl EIP712Field {
    pub fn new(name: &str, field_type: &str) -> Self {
        Self {
            name: name.to_string(),
            field_type: field_type.to_string(),
        }
    }
}

// ============================================================
// Action Hash Computation
// ============================================================

/// Compute action hash for L1 signing.
///
/// Algorithm:
/// 1. msgpack-encode the action (named/map format)
/// 2. Append nonce as 8 bytes big-endian
/// 3. Append vault address flag:
///    - If None: append 0x00
///    - If Some: append 0x01 + 20 address bytes
/// 4. keccak256 the whole thing
pub fn compute_action_hash(
    action: &serde_json::Value,
    vault_address: Option<&str>,
    nonce: u64,
) -> Result<[u8; 32], HlError> {
    motosan_wallet_core::compute_action_hash(action, nonce, vault_address)
        .map_err(|e| HlError::Serialization(e.to_string()))
}

// ============================================================
// L1 Action Signing
// ============================================================

/// Sign an L1 action using EIP-712 typed data.
///
/// Domain: { name: "Exchange", version: "1", chainId: 1337, verifyingContract: 0x0...0 }
/// primaryType: "Agent"
/// message: { source: 0xa (mainnet) or 0xb (testnet), connectionId: action_hash }
pub fn sign_l1_action(
    signer: &dyn Signer,
    address: &str,
    action: &serde_json::Value,
    nonce: u64,
    is_mainnet: bool,
    vault_address: Option<&str>,
) -> Result<Signature, HlError> {
    let adapter = SingleAddressSigner::new(signer, address.to_string());
    let hl_sig =
        motosan_wallet_core::sign_l1_action(&adapter, action, nonce, is_mainnet, vault_address)
            .map_err(|e| HlError::Signing(e.to_string()))?;
    Ok(hl_signature_to_signature(&hl_sig))
}

// ============================================================
// User-Signed Action Signing
// ============================================================

/// Sign a user-signed action (e.g., approveAgent).
///
/// Domain: { name: "HyperliquidSignTransaction", version: "1", chainId: 421614, verifyingContract: 0x0...0 }
///
/// This builds the EIP-712 struct hash manually from the provided type fields
/// and the action values, since the struct shape varies per action.
pub fn sign_user_signed_action(
    signer: &dyn Signer,
    address: &str,
    action: &serde_json::Value,
    types: &[EIP712Field],
    primary_type: &str,
    is_mainnet: bool,
) -> Result<Signature, HlError> {
    // Convert EIP712Field -> HlTypeField for motosan-wallet-core
    let hl_fields: Vec<HlTypeField<'_>> = types
        .iter()
        .map(|f| HlTypeField::new(&f.name, &f.field_type))
        .collect();

    let adapter = SingleAddressSigner::new(signer, address.to_string());
    let hl_sig = motosan_wallet_core::sign_user_signed_action(
        &adapter,
        action,
        &hl_fields,
        primary_type,
        is_mainnet,
    )
    .map_err(|e| HlError::Signing(e.to_string()))?;

    Ok(hl_signature_to_signature(&hl_sig))
}

// ============================================================
// Internal Helpers
// ============================================================

/// Convert motosan-wallet-core's HlSignature to our Signature type.
fn hl_signature_to_signature(hl: &motosan_wallet_core::HlSignature) -> Signature {
    Signature {
        r: hl.r.clone(),
        s: hl.s.clone(),
        v: hl.v,
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal test signer using k256 for testing.
    struct TestSigner {
        key: k256::ecdsa::SigningKey,
        address: String,
    }

    impl TestSigner {
        fn new(hex_key: &str) -> Self {
            let stripped = hex_key.strip_prefix("0x").unwrap_or(hex_key);
            let key_bytes = hex::decode(stripped).unwrap();
            let key = k256::ecdsa::SigningKey::from_bytes((&key_bytes[..]).into()).unwrap();

            // Derive address
            let verifying_key = key.verifying_key();
            let point = verifying_key.to_encoded_point(false);
            let pubkey_bytes = &point.as_bytes()[1..];
            use sha3::{Digest, Keccak256};
            let hash = Keccak256::digest(pubkey_bytes);
            let address = format!("0x{}", hex::encode(&hash[12..]));

            Self { key, address }
        }

        fn address(&self) -> &str {
            &self.address
        }
    }

    impl Signer for TestSigner {
        fn sign_hash(&self, _address: &str, hash: &[u8; 32]) -> Result<[u8; 65], HlError> {
            use k256::ecdsa::{signature::hazmat::PrehashSigner, RecoveryId};
            let (signature, recovery_id): (k256::ecdsa::Signature, RecoveryId) = self
                .key
                .sign_prehash(hash)
                .map_err(|e| HlError::Signing(format!("k256 signing error: {}", e)))?;
            let mut result = [0u8; 65];
            result[..64].copy_from_slice(&signature.to_bytes());
            result[64] = recovery_id.to_byte();
            Ok(result)
        }
    }

    const TEST_KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe512961708279f22a82e1e0e3e1d0a2";

    #[test]
    fn test_compute_action_hash_deterministic() {
        let action = serde_json::json!({
            "type": "order",
            "orders": [{"a": 0, "b": true, "p": "30000", "s": "0.1"}],
            "grouping": "na"
        });
        let hash1 = compute_action_hash(&action, None, 1234567890).unwrap();
        let hash2 = compute_action_hash(&action, None, 1234567890).unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_action_hash_different_nonce() {
        let action = serde_json::json!({"type": "order"});
        let hash1 = compute_action_hash(&action, None, 100).unwrap();
        let hash2 = compute_action_hash(&action, None, 200).unwrap();
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_action_hash_with_vault_address() {
        let action = serde_json::json!({"type": "order"});
        let hash_no_vault = compute_action_hash(&action, None, 100).unwrap();
        let hash_with_vault = compute_action_hash(
            &action,
            Some("0x1234567890abcdef1234567890abcdef12345678"),
            100,
        )
        .unwrap();
        assert_ne!(hash_no_vault, hash_with_vault);
    }

    #[test]
    fn test_sign_l1_action_produces_valid_signature() {
        let signer = TestSigner::new(TEST_KEY);
        let addr = signer.address().to_string();
        let action = serde_json::json!({
            "type": "order",
            "orders": [{"a": 0, "b": true, "p": "30000", "s": "0.1"}],
            "grouping": "na"
        });

        let sig = sign_l1_action(&signer, &addr, &action, 1234567890, true, None).unwrap();

        // Verify signature format
        assert!(sig.r.starts_with("0x"));
        assert!(sig.s.starts_with("0x"));
        assert_eq!(sig.r.len(), 66); // 0x + 64 hex chars
        assert_eq!(sig.s.len(), 66);
        assert!(sig.v == 27 || sig.v == 28);
    }

    #[test]
    fn test_sign_l1_action_mainnet_vs_testnet() {
        let signer = TestSigner::new(TEST_KEY);
        let addr = signer.address().to_string();
        let action = serde_json::json!({"type": "order"});

        let sig_mainnet = sign_l1_action(&signer, &addr, &action, 100, true, None).unwrap();
        let sig_testnet = sign_l1_action(&signer, &addr, &action, 100, false, None).unwrap();

        // Different source address should produce different signatures
        assert_ne!(sig_mainnet.r, sig_testnet.r);
    }

    #[test]
    fn test_sign_l1_action_recoverable() {
        let signer = TestSigner::new(TEST_KEY);
        let addr = signer.address().to_string();
        let action = serde_json::json!({"type": "order"});

        let sig = sign_l1_action(&signer, &addr, &action, 100, true, None).unwrap();

        // Recompute the EIP-712 hash to verify recovery.
        // We use motosan-wallet-core's compute_action_hash (via our wrapper).
        let action_hash = compute_action_hash(&action, None, 100).unwrap();

        // Build the Agent EIP-712 hash the same way motosan-wallet-core does.
        use sha3::{Digest, Keccak256};
        fn keccak256(data: &[u8]) -> [u8; 32] {
            Keccak256::digest(data).into()
        }
        let connection_id_hex = format!("0x{}", hex::encode(action_hash));
        let _agent_action = serde_json::json!({
            "source": "a",
            "connectionId": connection_id_hex,
        });

        // Type hash
        let type_string = "Agent(string source,bytes32 connectionId)";
        let type_hash = keccak256(type_string.as_bytes());

        // Domain separator (Exchange, v1, chainId 1337, verifyingContract zero)
        let domain_type_hash = keccak256(
            b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
        );
        let name_hash = keccak256(b"Exchange");
        let version_hash = keccak256(b"1");
        let mut chain_id_bytes = [0u8; 32];
        chain_id_bytes[24..32].copy_from_slice(&1337u64.to_be_bytes());
        let contract_bytes = [0u8; 32];

        let mut domain_buf = Vec::with_capacity(32 * 5);
        domain_buf.extend_from_slice(domain_type_hash.as_slice());
        domain_buf.extend_from_slice(name_hash.as_slice());
        domain_buf.extend_from_slice(version_hash.as_slice());
        domain_buf.extend_from_slice(&chain_id_bytes);
        domain_buf.extend_from_slice(&contract_bytes);
        let domain_sep = keccak256(&domain_buf);

        // Struct hash
        let source_hash = keccak256(b"a");
        let conn_bytes = hex::decode(connection_id_hex.strip_prefix("0x").unwrap()).unwrap();
        let mut struct_buf = Vec::with_capacity(32 * 3);
        struct_buf.extend_from_slice(&type_hash);
        struct_buf.extend_from_slice(&source_hash);
        struct_buf.extend_from_slice(&conn_bytes);
        let struct_hash = keccak256(&struct_buf);

        let mut eip712_data = Vec::with_capacity(66);
        eip712_data.extend_from_slice(&[0x19, 0x01]);
        eip712_data.extend_from_slice(&domain_sep);
        eip712_data.extend_from_slice(&struct_hash);
        let final_hash = keccak256(&eip712_data);

        // Recover the signer
        let r_bytes = hex::decode(sig.r.strip_prefix("0x").unwrap()).unwrap();
        let s_bytes = hex::decode(sig.s.strip_prefix("0x").unwrap()).unwrap();
        let v = sig.v - 27;

        let mut sig_bytes = [0u8; 64];
        sig_bytes[..32].copy_from_slice(&r_bytes);
        sig_bytes[32..].copy_from_slice(&s_bytes);

        use k256::ecdsa::{RecoveryId, Signature as K256Sig, VerifyingKey};
        let signature = K256Sig::from_slice(&sig_bytes).unwrap();
        let recovery_id = RecoveryId::from_byte(v).unwrap();
        let recovered =
            VerifyingKey::recover_from_prehash(&final_hash, &signature, recovery_id).unwrap();

        // Derive address from recovered key
        let point = recovered.to_encoded_point(false);
        let pubkey_bytes = &point.as_bytes()[1..];
        let hash = Keccak256::digest(pubkey_bytes);
        let recovered_addr = format!("0x{}", hex::encode(&hash[12..]));

        assert_eq!(recovered_addr, addr);
    }

    #[test]
    fn test_sign_user_signed_action() {
        let signer = TestSigner::new(TEST_KEY);
        let addr = signer.address().to_string();

        let action = serde_json::json!({
            "type": "approveAgent",
            "agentAddress": "0x1234567890abcdef1234567890abcdef12345678",
            "agentName": "test-agent",
            "nonce": 1000
        });

        let types = vec![
            EIP712Field::new("hyperliquidChain", "string"),
            EIP712Field::new("agentAddress", "address"),
            EIP712Field::new("agentName", "string"),
            EIP712Field::new("nonce", "uint64"),
        ];

        let sig = sign_user_signed_action(
            &signer,
            &addr,
            &action,
            &types,
            "HyperliquidTransaction:ApproveAgent",
            true,
        )
        .unwrap();

        assert!(sig.r.starts_with("0x"));
        assert!(sig.s.starts_with("0x"));
        assert_eq!(sig.r.len(), 66);
        assert_eq!(sig.s.len(), 66);
        assert!(sig.v == 27 || sig.v == 28);
    }
}
