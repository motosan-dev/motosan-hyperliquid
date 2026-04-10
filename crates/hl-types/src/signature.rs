use serde::{Deserialize, Serialize};

/// ECDSA signature split into r, s, v components (hex-encoded).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Signature {
    /// r component as 0x-prefixed hex string.
    pub r: String,
    /// s component as 0x-prefixed hex string.
    pub s: String,
    /// Recovery id (27 or 28).
    pub v: u8,
}

impl Signature {
    /// Creates a new `Signature` from its components.
    pub fn new(r: String, s: String, v: u8) -> Self {
        Self { r, s, v }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_serde_roundtrip() {
        let sig = Signature {
            r: "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".into(),
            s: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".into(),
            v: 27,
        };
        let json = serde_json::to_string(&sig).unwrap();
        let parsed: Signature = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.r, sig.r);
        assert_eq!(parsed.s, sig.s);
        assert_eq!(parsed.v, 27);
    }

    #[test]
    fn signature_v28_roundtrip() {
        let sig = Signature {
            r: "0x00".into(),
            s: "0x01".into(),
            v: 28,
        };
        let json = serde_json::to_string(&sig).unwrap();
        let parsed: Signature = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.v, 28);
    }
}
