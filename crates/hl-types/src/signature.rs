use serde::{Deserialize, Serialize};

/// ECDSA signature split into r, s, v components (hex-encoded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    /// r component as 0x-prefixed hex string.
    pub r: String,
    /// s component as 0x-prefixed hex string.
    pub s: String,
    /// Recovery id (27 or 28).
    pub v: u8,
}
