use crate::Signer;
use hl_types::HlError;
use k256::ecdsa::{signature::hazmat::PrehashSigner, SigningKey, VerifyingKey};
use sha3::{Digest, Keccak256};

/// A [`Signer`] backed by a raw ECDSA private key (k256/secp256k1).
///
/// Use this for simple setups where you have a hex-encoded private key.
/// For production, consider implementing [`Signer`] with a more secure
/// key management backend (HSM, keyring, HD wallet).
pub struct PrivateKeySigner {
    key: SigningKey,
    address: String,
}

impl PrivateKeySigner {
    /// Create a signer from a hex-encoded private key (with or without `0x` prefix).
    pub fn from_hex(key_hex: &str) -> Result<Self, HlError> {
        let stripped = key_hex.strip_prefix("0x").unwrap_or(key_hex);
        let bytes =
            hex::decode(stripped).map_err(|e| HlError::Signing(format!("invalid hex: {e}")))?;
        let key = SigningKey::from_bytes(bytes.as_slice().into())
            .map_err(|e| HlError::Signing(format!("invalid key: {e}")))?;
        let vk = *key.verifying_key();
        let address = Self::verifying_key_to_address(&vk);
        Ok(Self { key, address })
    }

    /// Returns the Ethereum address derived from this key.
    pub fn address(&self) -> &str {
        &self.address
    }

    fn verifying_key_to_address(vk: &VerifyingKey) -> String {
        let point = vk.to_encoded_point(false);
        let hash = Keccak256::digest(&point.as_bytes()[1..]);
        format!("0x{}", hex::encode(&hash[12..]))
    }
}

impl Signer for PrivateKeySigner {
    fn sign_hash(&self, _address: &str, hash: &[u8; 32]) -> Result<[u8; 65], HlError> {
        let (sig, recovery_id) = self
            .key
            .sign_prehash(hash)
            .map_err(|e| HlError::Signing(e.to_string()))?;
        let mut result = [0u8; 65];
        result[..64].copy_from_slice(&sig.to_bytes());
        result[64] = recovery_id.to_byte();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Signer;

    const TEST_KEY: &str = "4c0883a69102937d6231471b5dbb6204fe512961708279f696ae98d69e7e3e01";

    #[test]
    fn from_hex_valid_key_with_0x_prefix() {
        let signer = PrivateKeySigner::from_hex(&format!("0x{TEST_KEY}")).unwrap();
        assert!(signer.address().starts_with("0x"));
        assert_eq!(signer.address().len(), 42);
    }

    #[test]
    fn from_hex_valid_key_without_prefix() {
        let signer = PrivateKeySigner::from_hex(TEST_KEY).unwrap();
        assert!(signer.address().starts_with("0x"));
        assert_eq!(signer.address().len(), 42);
    }

    #[test]
    fn from_hex_invalid_hex() {
        let result = PrivateKeySigner::from_hex("not_a_valid_hex_string_at_all!!");
        assert!(result.is_err());
    }

    #[test]
    #[should_panic]
    fn from_hex_wrong_length_panics() {
        // SigningKey::from_bytes panics on wrong-length input via GenericArray
        let _ = PrivateKeySigner::from_hex("0xabcd");
    }

    #[test]
    #[should_panic]
    fn from_hex_empty_string_panics() {
        // SigningKey::from_bytes panics on empty input via GenericArray
        let _ = PrivateKeySigner::from_hex("");
    }

    #[test]
    fn sign_hash_produces_65_bytes() {
        let signer = PrivateKeySigner::from_hex(TEST_KEY).unwrap();
        let hash = [0u8; 32];
        let sig = signer.sign_hash(signer.address(), &hash).unwrap();
        assert_eq!(sig.len(), 65);
        assert!(sig[64] <= 1); // recovery id is 0 or 1
    }

    #[test]
    fn sign_hash_deterministic() {
        let signer = PrivateKeySigner::from_hex(TEST_KEY).unwrap();
        let hash = [42u8; 32];
        let sig1 = signer.sign_hash(signer.address(), &hash).unwrap();
        let sig2 = signer.sign_hash(signer.address(), &hash).unwrap();
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn sign_different_hashes_different_sigs() {
        let signer = PrivateKeySigner::from_hex(TEST_KEY).unwrap();
        let hash1 = [0u8; 32];
        let hash2 = [1u8; 32];
        let sig1 = signer.sign_hash(signer.address(), &hash1).unwrap();
        let sig2 = signer.sign_hash(signer.address(), &hash2).unwrap();
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn same_key_same_address() {
        let s1 = PrivateKeySigner::from_hex(TEST_KEY).unwrap();
        let s2 = PrivateKeySigner::from_hex(&format!("0x{TEST_KEY}")).unwrap();
        assert_eq!(s1.address(), s2.address());
    }
}
