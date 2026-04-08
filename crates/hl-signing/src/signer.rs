use hl_types::HlError;

/// Trait for signing message hashes.
///
/// This abstracts over different key management backends (keyring, HD wallet, etc.)
/// so that the signing crate does not depend on any specific implementation.
pub trait Signer: Send + Sync {
    /// Sign an arbitrary message hash (32 bytes) with the key for the given address.
    ///
    /// Returns a 65-byte signature: r (32 bytes) + s (32 bytes) + v (1 byte, recovery id 0 or 1).
    fn sign_hash(&self, address: &str, hash: &[u8; 32]) -> Result<[u8; 65], HlError>;
}
