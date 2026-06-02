use hl_signing::compute_action_hash;
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
