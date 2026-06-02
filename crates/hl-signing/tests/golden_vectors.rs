use hl_signing::{compute_action_hash, sign_l1_action, Signer};
use hl_types::HlError;
use serde::Serialize;
use serde_json::json;
use sha3::{Digest, Keccak256};

#[derive(Serialize)]
struct OracleTif {
    tif: &'static str,
}

#[derive(Serialize)]
struct OracleLimit {
    limit: OracleTif,
}

#[derive(Serialize)]
struct OracleOrder {
    a: u32,
    b: bool,
    p: &'static str,
    s: &'static str,
    r: bool,
    t: OracleLimit,
}

#[derive(Serialize)]
struct OracleAction {
    #[serde(rename = "type")]
    type_: &'static str,
    orders: Vec<OracleOrder>,
    grouping: &'static str,
}

fn oracle_action_hash(nonce: u64) -> [u8; 32] {
    let action = OracleAction {
        type_: "order",
        orders: vec![OracleOrder {
            a: 0,
            b: true,
            p: "50000",
            s: "0.01",
            r: false,
            t: OracleLimit {
                limit: OracleTif { tif: "Gtc" },
            },
        }],
        grouping: "na",
    };

    let mut data = rmp_serde::to_vec_named(&action).expect("oracle msgpack encode");
    data.extend_from_slice(&nonce.to_be_bytes());
    data.push(0x00);

    Keccak256::digest(&data).into()
}

#[test]
fn l1_action_hash_matches_canonical_oracle() {
    let nonce = 1_700_000_000_000;
    let action = json!({
        "type": "order",
        "orders": [{
            "a": 0,
            "b": true,
            "p": "50000",
            "s": "0.01",
            "r": false,
            "t": {"limit": {"tif": "Gtc"}},
        }],
        "grouping": "na",
    });

    let actual = compute_action_hash(&action, None, nonce).expect("action hash");
    let expected = oracle_action_hash(nonce);

    assert_eq!(
        hex::encode(actual),
        hex::encode(expected),
        "L1 action hash must preserve Hyperliquid declaration order"
    );
}

const TEST_KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe512961708279f22a82e1e0e3e1d0a2";

struct K256Signer {
    key: k256::ecdsa::SigningKey,
    address: String,
}

impl K256Signer {
    fn new(hex_key: &str) -> Self {
        let stripped = hex_key.strip_prefix("0x").unwrap_or(hex_key);
        let key_bytes = hex::decode(stripped).unwrap();
        let key = k256::ecdsa::SigningKey::from_bytes((&key_bytes[..]).into()).unwrap();
        let vk = key.verifying_key();
        let point = vk.to_encoded_point(false);
        let hash = Keccak256::digest(&point.as_bytes()[1..]);
        let address = format!("0x{}", hex::encode(&hash[12..]));
        Self { key, address }
    }
}

impl Signer for K256Signer {
    fn sign_hash(&self, _addr: &str, hash: &[u8; 32]) -> Result<[u8; 65], HlError> {
        use k256::ecdsa::{signature::hazmat::PrehashSigner, RecoveryId};
        let (sig, rid): (k256::ecdsa::Signature, RecoveryId) = self
            .key
            .sign_prehash(hash)
            .map_err(|e| HlError::signing(e.to_string()))?;
        let mut out = [0u8; 65];
        out[..64].copy_from_slice(&sig.to_bytes());
        out[64] = rid.to_byte();
        Ok(out)
    }
}

fn fixed_order_action() -> serde_json::Value {
    serde_json::json!({
        "type": "order",
        "orders": [{
            "a": 0, "b": true, "p": "30000", "s": "0.1", "r": false,
            "t": { "limit": { "tif": "Gtc" } }
        }],
        "grouping": "na"
    })
}

#[test]
fn l1_signature_baseline_mainnet() {
    let signer = K256Signer::new(TEST_KEY);
    let addr = signer.address.clone();
    let sig = sign_l1_action(
        &signer,
        &addr,
        &fixed_order_action(),
        1_700_000_000_000,
        true,
        None,
    )
    .unwrap();
    assert_eq!(
        sig.r,
        "0x35107cc4dd1903337bf938f35207fb4dc26af76976a19bb861e57aa825e661a3"
    );
    assert_eq!(
        sig.s,
        "0x528d2f578993152a17318e11b5fd6db1bff4c92e051020b1839de19160942f9e"
    );
    assert_eq!(sig.v, 28);
}
