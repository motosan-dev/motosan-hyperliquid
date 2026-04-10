//! Adapter that bridges [`crate::signer::Signer`] to [`motosan_wallet_core::HlSigner`].
//!
//! [`SingleAddressSigner`] wraps an hl-signing `Signer` and a fixed Ethereum
//! address so it can be passed to motosan-wallet-core signing functions that
//! expect the `HlSigner` trait.

use crate::signer::Signer;
use motosan_wallet_core::{HlSigner, WalletError};

/// Bridges an hl-signing [`Signer`] (multi-address, recovery-id v=0|1) to
/// motosan-wallet-core's [`HlSigner`] (single-address, Ethereum v=27|28).
pub struct SingleAddressSigner<'a, S: Signer + ?Sized> {
    signer: &'a S,
    address: String,
}

impl<'a, S: Signer + ?Sized> SingleAddressSigner<'a, S> {
    pub fn new(signer: &'a S, address: String) -> Self {
        Self { signer, address }
    }
}

impl<S: Signer + ?Sized> HlSigner for SingleAddressSigner<'_, S> {
    fn address(&self) -> Result<String, WalletError> {
        Ok(self.address.clone())
    }

    fn sign_prehash(&self, hash: &[u8; 32]) -> Result<[u8; 65], WalletError> {
        let mut sig = self
            .signer
            .sign_hash(&self.address, hash)
            .map_err(|e| WalletError::SigningError(e.to_string()))?;
        // Signer returns recovery id (0 or 1); HlSigner expects Ethereum v (27 or 28).
        sig[64] += 27;
        Ok(sig)
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use hl_types::HlError;

    // ── Mock signer that returns a pre-configured 65-byte signature ──

    /// A mock [`Signer`] that returns a fixed signature with the given
    /// recovery id byte at position 64.  This lets us test the v-byte
    /// conversion in isolation without real cryptographic signing.
    struct MockSigner {
        recovery_id: u8,
    }

    impl MockSigner {
        fn new(recovery_id: u8) -> Self {
            Self { recovery_id }
        }

        /// Build a deterministic 65-byte "signature" where r and s are
        /// filled with `0xAA` and the last byte is the recovery id.
        fn expected_raw_sig(&self) -> [u8; 65] {
            let mut sig = [0xAA_u8; 65];
            sig[64] = self.recovery_id;
            sig
        }
    }

    impl Signer for MockSigner {
        fn sign_hash(&self, _address: &str, _hash: &[u8; 32]) -> Result<[u8; 65], HlError> {
            Ok(self.expected_raw_sig())
        }
    }

    /// A mock signer that always returns an error, for error-propagation tests.
    struct FailingSigner;

    impl Signer for FailingSigner {
        fn sign_hash(&self, _address: &str, _hash: &[u8; 32]) -> Result<[u8; 65], HlError> {
            Err(HlError::Signing("simulated key failure".to_string()))
        }
    }

    // ── address() ────────────────────────────────────────────────

    #[test]
    fn address_returns_the_configured_address() {
        let signer = MockSigner::new(0);
        let adapter = SingleAddressSigner::new(&signer, "0xABCD".to_string());
        assert_eq!(adapter.address().unwrap(), "0xABCD");
    }

    // ── v-byte conversion (the critical path) ───────────────────

    #[test]
    fn sign_prehash_converts_recovery_id_0_to_v_27() {
        let signer = MockSigner::new(0);
        let adapter = SingleAddressSigner::new(&signer, "0x1234".to_string());
        let hash = [0u8; 32];

        let sig = adapter.sign_prehash(&hash).unwrap();

        // r and s bytes should be untouched.
        assert_eq!(&sig[..64], &[0xAA_u8; 64]);
        // v must be 27 (0 + 27).
        assert_eq!(sig[64], 27, "recovery id 0 must map to v = 27");
    }

    #[test]
    fn sign_prehash_converts_recovery_id_1_to_v_28() {
        let signer = MockSigner::new(1);
        let adapter = SingleAddressSigner::new(&signer, "0x1234".to_string());
        let hash = [0u8; 32];

        let sig = adapter.sign_prehash(&hash).unwrap();

        assert_eq!(&sig[..64], &[0xAA_u8; 64]);
        assert_eq!(sig[64], 28, "recovery id 1 must map to v = 28");
    }

    #[test]
    fn sign_prehash_v_byte_is_exactly_27_or_28() {
        // Both valid recovery ids must produce valid Ethereum v values.
        for recovery_id in [0u8, 1u8] {
            let signer = MockSigner::new(recovery_id);
            let adapter = SingleAddressSigner::new(&signer, "0xtest".to_string());
            let hash = [0xFF_u8; 32];

            let sig = adapter.sign_prehash(&hash).unwrap();
            let v = sig[64];

            assert!(
                v == 27 || v == 28,
                "v must be 27 or 28 for EVM compatibility, got {v} (recovery_id={recovery_id})"
            );
        }
    }

    // ── r and s passthrough (no mutation) ────────────────────────

    #[test]
    fn sign_prehash_does_not_modify_r_and_s_bytes() {
        let signer = MockSigner::new(1);
        let adapter = SingleAddressSigner::new(&signer, "0xaddr".to_string());
        let hash = [0x42_u8; 32];

        let sig = adapter.sign_prehash(&hash).unwrap();
        let raw = signer.expected_raw_sig();

        assert_eq!(
            &sig[..64],
            &raw[..64],
            "r and s bytes must not be modified by the adapter"
        );
    }

    // ── Error propagation ────────────────────────────────────────

    #[test]
    fn sign_prehash_propagates_signer_error_as_wallet_error() {
        let signer = FailingSigner;
        let adapter = SingleAddressSigner::new(&signer, "0xfail".to_string());
        let hash = [0u8; 32];

        let result = adapter.sign_prehash(&hash);
        assert!(result.is_err(), "signer failure must propagate");

        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("simulated key failure"),
            "error message must contain the original cause, got: {msg}"
        );
    }

    // ── Integration test with real k256 signing ──────────────────

    /// Uses a real k256 private key to sign, then verifies the adapter
    /// produces a valid EVM-compatible signature that can recover the
    /// correct public key.
    #[cfg(feature = "k256-signer")]
    #[test]
    fn sign_prehash_roundtrip_with_real_k256_key() {
        use k256::ecdsa::{signature::hazmat::PrehashSigner, RecoveryId, VerifyingKey};
        use sha3::{Digest, Keccak256};

        // Deterministic test key (DO NOT use in production).
        let key_bytes =
            hex::decode("4c0883a69102937d6231471b5dbb6204fe512961708279f696ae98d69e7e3e01")
                .unwrap();
        let signing_key = k256::ecdsa::SigningKey::from_bytes((&key_bytes[..]).into()).unwrap();

        // Derive address (take verifying key copy before moving signing_key).
        let verifying_key = *signing_key.verifying_key();
        let point = verifying_key.to_encoded_point(false);
        let pubkey_bytes = &point.as_bytes()[1..];
        let addr_hash = Keccak256::digest(pubkey_bytes);
        let address = format!("0x{}", hex::encode(&addr_hash[12..]));

        // Real signer that mirrors the TestSigner pattern from signing.rs.
        struct RealSigner {
            key: k256::ecdsa::SigningKey,
        }

        impl Signer for RealSigner {
            fn sign_hash(&self, _address: &str, hash: &[u8; 32]) -> Result<[u8; 65], HlError> {
                let (signature, recovery_id): (k256::ecdsa::Signature, RecoveryId) = self
                    .key
                    .sign_prehash(hash)
                    .map_err(|e| HlError::Signing(e.to_string()))?;
                let mut result = [0u8; 65];
                result[..64].copy_from_slice(&signature.to_bytes());
                result[64] = recovery_id.to_byte();
                Ok(result)
            }
        }

        let real_signer = RealSigner { key: signing_key };
        let adapter = SingleAddressSigner::new(&real_signer, address.clone());

        // Sign a test message hash.
        let hash = Keccak256::digest(b"test message for issue 863");
        let hash_arr: [u8; 32] = hash.into();

        let sig = adapter.sign_prehash(&hash_arr).unwrap();

        // v must be 27 or 28 (Ethereum standard).
        let v = sig[64];
        assert!(
            v == 27 || v == 28,
            "v byte must be 27 or 28 for EVM, got {v}"
        );

        // Recover the public key using the Ethereum v convention.
        let recovery_id =
            RecoveryId::from_byte(v - 27).expect("v - 27 must be a valid recovery id (0 or 1)");
        let ecdsa_sig = k256::ecdsa::Signature::from_slice(&sig[..64]).expect("valid r||s");
        let recovered = VerifyingKey::recover_from_prehash(&hash_arr, &ecdsa_sig, recovery_id)
            .expect("public key recovery must succeed");

        // The recovered key must match the original signing key.
        assert_eq!(
            recovered, verifying_key,
            "recovered public key must match the signer's key"
        );
    }
}
