use hl_types::HlError;
use crate::Signer;
use k256::ecdsa::{signature::hazmat::PrehashSigner, SigningKey, VerifyingKey};
use sha3::{Digest, Keccak256};

pub struct PrivateKeySigner {
    key: SigningKey,
    address: String,
}

impl PrivateKeySigner {
    pub fn from_hex(key_hex: &str) -> Result<Self, HlError> {
        let stripped = key_hex.strip_prefix("0x").unwrap_or(key_hex);
        let bytes = hex::decode(stripped)
            .map_err(|e| HlError::Signing(format!("invalid hex: {e}")))?;
        let key = SigningKey::from_bytes(bytes.as_slice().into())
            .map_err(|e| HlError::Signing(format!("invalid key: {e}")))?;
        let vk = *key.verifying_key();
        let address = Self::verifying_key_to_address(&vk);
        Ok(Self { key, address })
    }

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
        let (sig, recovery_id) = self.key.sign_prehash(hash)
            .map_err(|e| HlError::Signing(e.to_string()))?;
        let mut result = [0u8; 65];
        result[..64].copy_from_slice(&sig.to_bytes());
        result[64] = recovery_id.to_byte();
        Ok(result)
    }
}
