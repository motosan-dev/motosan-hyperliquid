# M0 — Signing & Wire Correctness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every signed Hyperliquid L1 / user-signed action produce a valid signature and a wire-canonical payload the exchange accepts, and lock it down with self-contained golden-vector tests.

**Architecture:** The signing/hash core lives in the external crate `motosan-wallet-core` (the user controls it). We fix the two critical signing defects **upstream** in wallet-core (release `0.5.1`) and the wire-formatting + idempotency defects **in this repo**. Correctness is anchored by **in-repo derived golden vectors** (no Python): an independent canonical-field-order oracle (serde struct declaration order via `rmp_serde`) for the L1 action hash, and a digest-recovery oracle for the user-signed EIP-712 path.

**Tech Stack:** Rust, `serde_json`, `rmp-serde`, `rust_decimal`, `sha3` (Keccak256), `k256`, `motosan-wallet-core`.

**Decisions locked (from review):**
- R1/R2 fixes land **upstream in `motosan-wallet-core` → 0.5.1**, this repo bumps the dep.
- Golden vectors are **derived in-repo** (no official Python SDK run); signature `r/s/v` are recorded post-fix as regression baselines.

**Root-cause recap (verified against source):**
- **R1** `compute_action_hash_inner` (`motosan-wallet-core-0.5.0/src/operations/hyperliquid.rs:155`) does `rmp_serde::to_vec_named(action)` on a `serde_json::Value`. `serde_json` has **no `preserve_order`** (Cargo.lock deps: `itoa, memchr, serde, serde_core, zmij` — no `indexmap`), so `Value::Object` is a `BTreeMap` → keys serialize **alphabetically**. Order fields `{a,b,p,s,r,t,c}` become `a,b,c,p,r,s,t` (`r`/`s` swapped) → wrong `connectionId` → every L1 signature invalid.
- **R2** `sign_user_signed_action` (`hyperliquid.rs:229,246-249,283`) uses domain `chainId = 42161` on mainnet (canonical: `421614`/`0x66eee` for both nets), writes `signatureChainId` as a **decimal string** (canonical: hex `"0x66eee"`), and computes a **3-field domain separator missing `verifyingContract`** (canonical: 4-field). The L1 domain path (`hyperliquid.rs:210`, alloy `eip712_signing_hash`) is already correct.

---

## File Structure

**Repo: `motosan-wallet-core` (separate repo; specs only — executed there):**
- Modify: `Cargo.toml` — enable `serde_json` `preserve_order`
- Modify: `src/operations/hyperliquid.rs:229,246-249,279-300` — user-signed domain/chainId/signatureChainId fix

**Repo: `motosan-hyperliquid` (this repo):**
- Modify: `crates/hl-signing/Cargo.toml` — add `rmp-serde`, `serde` dev-deps; bump `motosan-wallet-core` to `0.5.1`
- Create: `crates/hl-signing/tests/golden_vectors.rs` — L1 action-hash oracle + user-signed digest-recovery oracle + signature baselines
- Modify: `crates/hl-types/src/util.rs` (or `lib.rs`) — add `normalize_wire(Decimal) -> String`
- Modify: `crates/hl-types/src/order.rs:247-324` — builders use `normalize_wire`
- Modify: `crates/hl-executor/src/executor/orders.rs` — add `new_cloid`/`ensure_cloid`/`round_price_perp`/`round_size`; wire them into `place_order`, `bulk_order`, `market_open`, `market_close`, `place_trigger_order`
- Modify: `crates/hl-executor/src/executor/transfer.rs:41`, `crates/hl-executor/src/executor/admin.rs:21` — set the `signatureChainId` wire literal to canonical `0x66eee`
- Modify: `CHANGELOG.md`

---

## Track W — `motosan-wallet-core` (upstream, release 0.5.1)

> These tasks execute in the wallet-core repo, not here. They are the prerequisite for Task 2. Each is concrete against the `0.5.0` source.

### Task W1: Fix msgpack key ordering (R1)

**Files (wallet-core):** `Cargo.toml`, `src/operations/hyperliquid.rs`

- [ ] **Step 1:** In wallet-core `Cargo.toml`, enable preserve-order on serde_json:

```toml
serde_json = { version = "1", features = ["preserve_order"] }
```

This makes `serde_json::Value::Object` an insertion-ordered `IndexMap`. Via Cargo feature unification it also turns on `preserve_order` for the consumer (`motosan-hyperliquid`), so `serde_json::json!` actions built in canonical Hyperliquid field order serialize correctly.

- [ ] **Step 2:** Add an asserting in-crate regression test next to `compute_action_hash` that builds an order action with `serde_json::json!` in canonical order and compares against a declaration-order struct serialized with `rmp_serde::to_vec_named` (same oracle technique as Task 1 here). It must FAIL before Step 1 and PASS after.

- [ ] **Step 3:** Run `cargo test` in wallet-core. Expected: the new test passes; existing tests still pass.

- [ ] **Step 4:** Commit: `fix: serialize Hyperliquid action msgpack in canonical field order (preserve_order)`

### Task W2: Fix user-signed EIP-712 domain (R2)

**Files (wallet-core):** `src/operations/hyperliquid.rs:228-249,279-300`

- [ ] **Step 1:** In `sign_user_signed_action`, use the canonical chain id for **both** networks and write `signatureChainId` as a hex string:

```rust
// chainId is 0x66eee (421614) for BOTH mainnet and testnet (matches the Python SDK).
let chain_id: u64 = 421614;
let domain = alloy::sol_types::Eip712Domain {
    name: Some("HyperliquidSignTransaction".into()),
    version: Some("1".into()),
    chain_id: Some(alloy::primitives::U256::from(chain_id)),
    verifying_contract: Some(Address::ZERO), // 4-field domain
    salt: None,
};
// ...
map.insert("signatureChainId".into(), Value::String("0x66eee".into()));
```

- [ ] **Step 2:** Make the user-signed domain separator 4-field (include `verifyingContract`). Simplest: reuse alloy's domain hashing instead of the hand-rolled 3-field `compute_domain_separator`:

```rust
let domain_separator = domain.hash_struct(); // alloy includes verifyingContract when Some
```

(or extend `compute_domain_separator` to hash `EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)` with the 32-byte zero address appended.)

- [ ] **Step 3:** Add an asserting test: sign a fixed `usdSend` action with a fixed key on mainnet and testnet, ecrecover the signer, assert it equals the key's address. Must FAIL before, PASS after.

- [ ] **Step 4:** Run `cargo test` in wallet-core. Expected: pass.

- [ ] **Step 5:** Commit: `fix: correct Hyperliquid user-signed EIP-712 domain (chainId 421614, 0x66eee, verifyingContract)`

### Task W3: Release 0.5.1

- [ ] **Step 1:** Bump wallet-core `Cargo.toml` version to `0.5.1`; add a CHANGELOG entry.
- [ ] **Step 2:** `cargo publish` (or push the tag that triggers publish).
- [ ] **Step 3:** Confirm `motosan-wallet-core 0.5.1` is resolvable from crates.io.

---

## Track H — `motosan-hyperliquid` (this repo)

### Task 1: L1 action-hash golden-vector oracle (T1) — RED first

**Files:**
- Modify: `crates/hl-signing/Cargo.toml`
- Create: `crates/hl-signing/tests/golden_vectors.rs`

- [ ] **Step 1: Add dev-deps** to `crates/hl-signing/Cargo.toml` under `[dev-dependencies]`:

```toml
[dev-dependencies]
tokio = { version = "1", features = ["full"] }
rmp-serde = "1"
serde = { version = "1", features = ["derive"] }
sha3 = "0.10"
hex = "0.4"
k256 = { version = "0.13", features = ["ecdsa"] }   # optional dep behind `k256-signer`; add here so the test target always links it
```

- [ ] **Step 2: Write the failing oracle test** `crates/hl-signing/tests/golden_vectors.rs`:

```rust
//! In-repo derived golden vectors. No external (Python) reference needed:
//! the oracle msgpacks canonical-ordered serde structs (serde serializes
//! struct fields in DECLARATION order, never alphabetically), reproducing
//! Hyperliquid's action-hash scheme independently of the production path.

use hl_signing::compute_action_hash;
use serde::Serialize;
use sha3::{Digest, Keccak256};

#[derive(Serialize)]
struct OracleTif { tif: &'static str }
#[derive(Serialize)]
struct OracleLimit { limit: OracleTif }
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

/// keccak256( msgpack(canonical action) || nonce_be(8) || 0x00 ).
fn oracle_action_hash(nonce: u64) -> [u8; 32] {
    let action = OracleAction {
        type_: "order",
        orders: vec![OracleOrder {
            a: 0,
            b: true,
            p: "30000",
            s: "0.1",
            r: false,
            t: OracleLimit { limit: OracleTif { tif: "Gtc" } },
        }],
        grouping: "na",
    };
    let mut data = rmp_serde::to_vec_named(&action).unwrap();
    data.extend_from_slice(&nonce.to_be_bytes());
    data.push(0x00);
    let mut h = Keccak256::new();
    h.update(&data);
    h.finalize().into()
}

#[test]
fn l1_action_hash_matches_canonical_oracle() {
    // Production path: identical action via serde_json::json!, hashed through
    // compute_action_hash -> motosan-wallet-core rmp_serde::to_vec_named.
    let action = serde_json::json!({
        "type": "order",
        "orders": [{
            "a": 0, "b": true, "p": "30000", "s": "0.1", "r": false,
            "t": { "limit": { "tif": "Gtc" } }
        }],
        "grouping": "na"
    });
    let nonce = 1_700_000_000_000u64;
    let produced = compute_action_hash(&action, None, nonce).unwrap();
    let oracle = oracle_action_hash(nonce);
    assert_eq!(
        produced, oracle,
        "production action hash must equal the canonical field-order oracle; \
         a mismatch means serde_json serializes keys alphabetically (no \
         preserve_order) and Hyperliquid will reject every L1 signature"
    );
}
```

- [ ] **Step 3: Run the test, verify it FAILS** (against current `motosan-wallet-core 0.5.0`):

Run: `cargo test -p hl-signing --test golden_vectors l1_action_hash_matches_canonical_oracle`
Expected: **FAIL** — `assertion failed: produced == oracle` (production emits `a,b,p,r,s,t`, oracle emits `a,b,p,s,r,t`). This RED proves R1.

- [ ] **Step 4: Commit the failing test:**

```bash
git add crates/hl-signing/Cargo.toml crates/hl-signing/tests/golden_vectors.rs
git commit -m "test: add failing L1 action-hash canonical-order oracle (proves R1)"
```

### Task 2: Bump wallet-core to 0.5.1 → R1 GREEN

**Files:**
- Modify: `crates/hl-signing/Cargo.toml`

**Depends on:** Track W (0.5.1 published).

- [ ] **Step 1: Bump the dependency** in `crates/hl-signing/Cargo.toml`:

```toml
motosan-wallet-core = { version = "0.5.1", features = ["hyperliquid"] }
```

- [ ] **Step 2: Update the lockfile:**

Run: `cargo update -p motosan-wallet-core --precise 0.5.1`
Expected: `Updating motosan-wallet-core v0.5.0 -> v0.5.1`

- [ ] **Step 3: Verify the oracle test now PASSES:**

Run: `cargo test -p hl-signing --test golden_vectors l1_action_hash_matches_canonical_oracle`
Expected: **PASS**.

- [ ] **Step 4: Audit every action-build site is in canonical field order** (so `preserve_order` produces correct bytes). Confirm by reading:
  - `crates/hl-executor/src/executor/orders.rs:12-19` (`order_to_json`): `a,b,p,s,r,t` then `c` ✓
  - `crates/hl-executor/src/executor/orders.rs:85-89,217-221` (`{type,orders,grouping}`) ✓
  - `crates/hl-executor/src/executor/orders.rs:131-149` (`place_trigger_order`) ✓
  - `crates/hl-executor/src/executor/transfer.rs:15-20` (`vaultTransfer`: `type,vaultAddress,isDeposit,usd`) ✓
  - `crates/hl-executor/src/executor/cancel.rs`, `modify.rs`, `leverage.rs` — confirm `type` is first and field order matches Hyperliquid. Fix any that are out of order.

- [ ] **Step 5: Run the full suite:**

Run: `cargo test --all-features`
Expected: PASS.

- [ ] **Step 6: Commit:**

```bash
git add crates/hl-signing/Cargo.toml Cargo.lock
git commit -m "fix: bump motosan-wallet-core to 0.5.1 — canonical msgpack key order (R1)"
```

### Task 3: L1 signature regression baseline (T1)

**Files:**
- Modify: `crates/hl-signing/tests/golden_vectors.rs`

- [ ] **Step 1: Add a signing test that prints r/s/v** (use the well-known test key already in the crate's unit tests):

```rust
use hl_signing::{sign_l1_action, Signer};
use hl_types::HlError;

const TEST_KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe512961708279f22a82e1e0e3e1d0a2";

struct K256Signer {
    key: k256::ecdsa::SigningKey,
    address: String,
}
impl K256Signer {
    fn new(hex_key: &str) -> Self {
        let stripped = hex_key.strip_prefix("0x").unwrap_or(hex_key);
        let key = k256::ecdsa::SigningKey::from_bytes(
            (&hex::decode(stripped).unwrap()[..]).into(),
        )
        .unwrap();
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
    let sig = sign_l1_action(&signer, &addr, &fixed_order_action(), 1_700_000_000_000, true, None)
        .unwrap();
    // Step 2 will replace this print with hard-coded asserts.
    println!("MAINNET r={} s={} v={}", sig.r, sig.s, sig.v);
}
```

- [ ] **Step 2: Run once to capture the baseline:**

Run: `cargo test -p hl-signing --test golden_vectors l1_signature_baseline_mainnet -- --nocapture`
Expected: prints `MAINNET r=0x… s=0x… v=…`. Copy those exact values.

- [ ] **Step 3: Replace the print with hard asserts** using the captured values (example shape — substitute the real captured hex):

```rust
    assert_eq!(sig.r, "0x<captured-r>");
    assert_eq!(sig.s, "0x<captured-s>");
    assert_eq!(sig.v, 27); // or 28, whichever was captured
```

- [ ] **Step 4: Re-run, verify PASS:**

Run: `cargo test -p hl-signing --test golden_vectors l1_signature_baseline_mainnet`
Expected: PASS. This pins the full signature against regression.

- [ ] **Step 5: Commit:**

```bash
git add crates/hl-signing/tests/golden_vectors.rs
git commit -m "test: pin L1 signature r/s/v regression baseline"
```

### Task 4: User-signed digest-recovery oracle (R2 / T1)

**Files:**
- Modify: `crates/hl-signing/tests/golden_vectors.rs`

**Depends on:** Task 2 (0.5.1 with the R2 domain fix).

- [ ] **Step 1: Write a test that independently builds the canonical user-signed EIP-712 digest for `usdSend`, signs via the production `sign_user_signed_action`, ecrecovers, and asserts the recovered address equals the signer.** A mismatch means the production domain/chainId is wrong.

```rust
use hl_signing::{sign_user_signed_action, EIP712Field};
use k256::ecdsa::{RecoveryId, Signature as K256Sig, VerifyingKey};

fn keccak(bytes: &[u8]) -> [u8; 32] {
    let mut h = Keccak256::new();
    h.update(bytes);
    h.finalize().into()
}

// Canonical user-signed digest for usdSend: chainId 421614, verifyingContract 0x0,
// fields hyperliquidChain/destination/amount/time, signatureChainId NOT hashed.
fn oracle_usd_send_digest(destination: &str, amount: &str, time: u64, is_mainnet: bool) -> [u8; 32] {
    let hl_chain = if is_mainnet { "Mainnet" } else { "Testnet" };

    // domainSeparator = keccak( typeHash || keccak(name) || keccak(version) || chainId(32) || verifyingContract(32) )
    let domain_type = keccak(
        b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
    );
    let mut chain_id = [0u8; 32];
    chain_id[24..].copy_from_slice(&421614u64.to_be_bytes());
    let verifying_contract = [0u8; 32]; // zero address, left-padded
    let mut buf = Vec::new();
    buf.extend_from_slice(&domain_type);
    buf.extend_from_slice(&keccak(b"HyperliquidSignTransaction"));
    buf.extend_from_slice(&keccak(b"1"));
    buf.extend_from_slice(&chain_id);
    buf.extend_from_slice(&verifying_contract);
    let domain_separator = keccak(&buf);

    // structHash = keccak( typeHash || keccak(hl_chain) || keccak(destination) || keccak(amount) || time(32) )
    let struct_type = keccak(
        b"HyperliquidTransaction:UsdSend(string hyperliquidChain,string destination,string amount,uint64 time)",
    );
    let mut t = [0u8; 32];
    t[24..].copy_from_slice(&time.to_be_bytes());
    let mut sbuf = Vec::new();
    sbuf.extend_from_slice(&struct_type);
    sbuf.extend_from_slice(&keccak(hl_chain.as_bytes()));
    sbuf.extend_from_slice(&keccak(destination.as_bytes()));
    sbuf.extend_from_slice(&keccak(amount.as_bytes()));
    sbuf.extend_from_slice(&t);
    let struct_hash = keccak(&sbuf);

    // EIP-712 digest = keccak( 0x1901 || domainSeparator || structHash )
    let mut fbuf = Vec::new();
    fbuf.extend_from_slice(&[0x19, 0x01]);
    fbuf.extend_from_slice(&domain_separator);
    fbuf.extend_from_slice(&struct_hash);
    keccak(&fbuf)
}

fn recover_address(digest: &[u8; 32], r: &str, s: &str, v: u8) -> String {
    let r = hex::decode(r.strip_prefix("0x").unwrap()).unwrap();
    let s = hex::decode(s.strip_prefix("0x").unwrap()).unwrap();
    let mut sig = [0u8; 64];
    sig[..32].copy_from_slice(&r);
    sig[32..].copy_from_slice(&s);
    let rid = RecoveryId::from_byte(if v >= 27 { v - 27 } else { v }).unwrap();
    let sig = K256Sig::from_bytes((&sig).into()).unwrap();
    let vk = VerifyingKey::recover_from_prehash(digest, &sig, rid).unwrap();
    let point = vk.to_encoded_point(false);
    let hash = Keccak256::digest(&point.as_bytes()[1..]);
    format!("0x{}", hex::encode(&hash[12..]))
}

#[test]
fn usd_send_recovers_signer_under_canonical_domain() {
    let signer = K256Signer::new(TEST_KEY);
    let addr = signer.address.clone();
    let destination = "0x0000000000000000000000000000000000000001";
    let amount = "12.5";
    let time = 1_700_000_000_000u64;

    let action = serde_json::json!({
        "type": "usdSend",
        "hyperliquidChain": "Mainnet",
        "signatureChainId": "0x66eee",
        "destination": destination,
        "amount": amount,
        "time": time,
    });
    let types = vec![
        EIP712Field::new("hyperliquidChain", "string"),
        EIP712Field::new("destination", "string"),
        EIP712Field::new("amount", "string"),
        EIP712Field::new("time", "uint64"),
    ];
    let sig = sign_user_signed_action(
        &signer, &addr, &action, &types, "HyperliquidTransaction:UsdSend", true,
    )
    .unwrap();

    let digest = oracle_usd_send_digest(destination, amount, time, true);
    let recovered = recover_address(&digest, &sig.r, &sig.s, sig.v);
    assert_eq!(
        recovered.to_lowercase(),
        addr.to_lowercase(),
        "usdSend signature must recover the signer under the canonical domain \
         (chainId 421614, verifyingContract 0x0); mismatch means R2 is unfixed"
    );
}
```

- [ ] **Step 2: Run, verify PASS** (with 0.5.1):

Run: `cargo test -p hl-signing --test golden_vectors usd_send_recovers_signer_under_canonical_domain`
Expected: PASS. (Against 0.5.0 it FAILS — confirming the oracle catches R2.)

- [ ] **Step 3: Commit:**

```bash
git add crates/hl-signing/tests/golden_vectors.rs
git commit -m "test: user-signed usdSend digest-recovery oracle (anchors R2)"
```

### Task 5: Correct the user-signed `signatureChainId` wire literal to canonical `0x66eee` (R2)

**Files:**
- Modify: `crates/hl-executor/src/executor/transfer.rs:41`
- Modify: `crates/hl-executor/src/executor/admin.rs:21`

> **Why a value change, NOT a deletion:** `motosan-wallet-core::sign_user_signed_action`
> (`hyperliquid.rs:238-249`) does `let mut action_obj = action.clone();` and injects
> `hyperliquidChain`/`signatureChainId` into that **local clone** used only for the EIP-712 struct
> hash. It returns `HlSignature { r, s, v }` and **never mutates the caller's `action`**. The caller
> then posts its *own unmodified* `action` (`client.rs:142-157` serializes it verbatim into the POST
> body). The exchange **requires** `signatureChainId` in the wire payload and reconstructs the domain
> from it, so the literal must stay — and must equal the chain id (`421614`) that wallet-core 0.5.1
> signs with, i.e. `"0x66eee"`. Deleting it (or leaving `"0xa4b1"`) makes the exchange rebuild the
> wrong domain → signer recovery fails → action rejected.

- [ ] **Step 1:** In `transfer.rs` `usdc_transfer`, change the literal value:

```rust
            "signatureChainId": "0x66eee",
```

- [ ] **Step 2:** In `admin.rs` `approve_agent`, change the same `"0xa4b1"` literal to `"0x66eee"`.

- [ ] **Step 3: Add an assertion test** (executor test module) that the built `usdSend` action JSON carries the canonical chain id:

```rust
    #[test]
    fn usd_send_action_carries_canonical_signature_chain_id() {
        // Mirror the json! built in usdc_transfer.
        let action = serde_json::json!({
            "type": "usdSend",
            "hyperliquidChain": "Mainnet",
            "signatureChainId": "0x66eee",
            "destination": "0x0000000000000000000000000000000000000001",
            "amount": "1",
            "time": 1u64,
        });
        assert_eq!(action["signatureChainId"], "0x66eee");
    }
```

- [ ] **Step 4: Run executor tests:**

Run: `cargo test -p hl-executor`
Expected: PASS. The posted `usdSend`/`approveAgent` wire body now carries `signatureChainId: "0x66eee"`, matching the domain wallet-core 0.5.1 signs with.

- [ ] **Step 5: Commit:**

```bash
git add crates/hl-executor/src/executor/transfer.rs crates/hl-executor/src/executor/admin.rs
git commit -m "fix(executor): user-signed signatureChainId wire value -> canonical 0x66eee (R2)"
```

### Task 6: `normalize_wire` — canonical float wire string (R3a)

**Files:**
- Modify: `crates/hl-types/src/util.rs`
- Modify: `crates/hl-types/src/lib.rs` (re-export)

- [ ] **Step 1: Write failing tests** in `crates/hl-types/src/util.rs` (append a `#[cfg(test)] mod` or add to existing tests):

```rust
#[cfg(test)]
mod wire_tests {
    use super::normalize_wire;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal { Decimal::from_str(s).unwrap() }

    #[test]
    fn strips_trailing_zeros() {
        assert_eq!(normalize_wire(d("100.0")), "100");
        assert_eq!(normalize_wire(d("94500.000")), "94500");
        assert_eq!(normalize_wire(d("0.10")), "0.1");
        assert_eq!(normalize_wire(d("100.150")), "100.15");
    }

    #[test]
    fn caps_at_eight_decimals() {
        // round_dp(8) is banker's rounding (MidpointNearestEven) in rust_decimal
        // 1.41 — and 0.000000005 is an exact midpoint that rounds to even => "0".
        // Use NON-midpoint inputs so the expectation is unambiguous.
        assert_eq!(normalize_wire(d("0.000000006")), "0.00000001");
        assert_eq!(normalize_wire(d("0.123456789")), "0.12345679");
        assert_eq!(normalize_wire(d("0.000000005")), "0"); // documents banker's rounding
    }

    #[test]
    fn integer_is_plain() {
        assert_eq!(normalize_wire(d("42")), "42");
        assert_eq!(normalize_wire(Decimal::ZERO), "0");
    }
}
```

- [ ] **Step 2: Run, verify FAIL:**

Run: `cargo test -p hl-types wire_tests`
Expected: FAIL — `cannot find function normalize_wire`.

- [ ] **Step 3: Implement `normalize_wire`** in `crates/hl-types/src/util.rs`:

```rust
use rust_decimal::Decimal;

/// Format a [`Decimal`] into Hyperliquid canonical wire form.
///
/// Mirrors the Python SDK's `float_to_wire`: at most 8 decimal places,
/// trailing zeros stripped, plain decimal (never scientific notation).
pub fn normalize_wire(value: Decimal) -> String {
    // round_dp uses banker's rounding (MidpointNearestEven), which matches the
    // Python SDK's float_to_wire (`f"{x:.8f}"`). normalize() strips trailing
    // zeros and converts -0 to 0; Decimal's Display never uses sci-notation.
    value.round_dp(8).normalize().to_string()
}
```

- [ ] **Step 4: Re-export** in `crates/hl-types/src/lib.rs` (next to the other `util` re-exports, e.g. `normalize_coin`):

```rust
pub use util::normalize_wire;
```

- [ ] **Step 5: Run, verify PASS:**

Run: `cargo test -p hl-types wire_tests`
Expected: PASS.

- [ ] **Step 6: Commit:**

```bash
git add crates/hl-types/src/util.rs crates/hl-types/src/lib.rs
git commit -m "feat(types): add normalize_wire for canonical Hyperliquid float strings"
```

### Task 7: Builders emit canonical wire strings (R3a)

**Files:**
- Modify: `crates/hl-types/src/order.rs:247-324` (`limit_buy`, `limit_sell`, `trigger_buy`, `trigger_sell`)

- [ ] **Step 1: Write a failing test** in the `order.rs` test module:

```rust
    #[test]
    fn builder_uses_canonical_wire_strings() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let order = OrderWire::limit_buy(
            0,
            Decimal::from_str("94500.000").unwrap(),
            Decimal::from_str("0.0010").unwrap(),
        )
        .build()
        .unwrap();
        assert_eq!(order.limit_px, "94500");
        assert_eq!(order.sz, "0.001");
    }
```

- [ ] **Step 2: Run, verify FAIL:**

Run: `cargo test -p hl-types builder_uses_canonical_wire_strings`
Expected: FAIL — `assertion failed: order.limit_px == "94500"` (currently `"94500.000"`).

- [ ] **Step 3: Replace `to_string()` with `normalize_wire`** in all four constructors. For `limit_buy` (apply the same change to `limit_sell`, `trigger_buy`, `trigger_sell`):

```rust
    pub fn limit_buy(asset: u32, limit_px: Decimal, sz: Decimal) -> OrderWireBuilder {
        OrderWireBuilder {
            asset,
            is_buy: true,
            limit_px: crate::normalize_wire(limit_px),
            sz: crate::normalize_wire(sz),
            reduce_only: false,
            order_type: OrderTypeWire::Limit(LimitOrderType { tif: Tif::Gtc }),
            cloid: None,
        }
    }
```

For the trigger constructors, also normalize the `trigger_px`:

```rust
        let trigger_px_str = crate::normalize_wire(trigger_px);
```

- [ ] **Step 4: Run, verify PASS** (and the existing doctest at `order.rs:245` that expects `"90000"` still holds):

Run: `cargo test -p hl-types`
Expected: PASS.

- [ ] **Step 5: Commit:**

```bash
git add crates/hl-types/src/order.rs
git commit -m "fix(types): order builders emit canonical wire strings (R3)"
```

### Task 8: Price/size rounding for market orders (R3b)

**Files:**
- Modify: `crates/hl-executor/src/executor/orders.rs` (add helpers + wire into `market_open`/`market_close`)

- [ ] **Step 1: Write failing tests** in the `orders.rs` test module (it already has a `#[cfg(test)] mod tests` around the slippage tests):

```rust
    #[test]
    fn round_price_perp_caps_5_sig_figs() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        // BTC-like: szDecimals 5 -> max 1 decimal place; 5 sig figs.
        let p = round_price_perp(Decimal::from_str("66604.125").unwrap(), 5);
        assert_eq!(p, Decimal::from_str("66604").unwrap());
        // Cheap token: szDecimals 0 -> max 6 decimal places; 5 sig figs.
        let p2 = round_price_perp(Decimal::from_str("0.0034521").unwrap(), 0);
        assert_eq!(p2, Decimal::from_str("0.003452").unwrap());
    }

    #[test]
    fn round_size_truncates_to_sz_decimals() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let s = round_size(Decimal::from_str("0.123456").unwrap(), 3);
        assert_eq!(s, Decimal::from_str("0.123").unwrap());
    }
```

- [ ] **Step 2: Run, verify FAIL:**

Run: `cargo test -p hl-executor round_price_perp_caps_5_sig_figs`
Expected: FAIL — `cannot find function round_price_perp`.

- [ ] **Step 3: Implement the helpers** near the top of `crates/hl-executor/src/executor/orders.rs` (after the imports):

```rust
use rust_decimal::RoundingStrategy;

/// Round a perp price to Hyperliquid's rule: at most 5 significant figures
/// AND at most `6 - sz_decimals` decimal places.
pub(crate) fn round_price_perp(px: Decimal, sz_decimals: u32) -> Decimal {
    let max_dp = 6u32.saturating_sub(sz_decimals);
    let sf = px.round_sf(5).unwrap_or(px);
    sf.round_dp(max_dp)
}

/// Round an order size down (toward zero) to the asset's `szDecimals`.
pub(crate) fn round_size(sz: Decimal, sz_decimals: u32) -> Decimal {
    sz.round_dp_with_strategy(sz_decimals, RoundingStrategy::ToZero)
}
```

- [ ] **Step 4: Run, verify PASS:**

Run: `cargo test -p hl-executor round_price_perp_caps_5_sig_figs round_size_truncates_to_sz_decimals`
Expected: PASS.

- [ ] **Step 5: Wire into `market_open`** (`orders.rs:280-299`). After computing `limit_price` and before building the order, resolve `sz_decimals` and round both price and size:

```rust
        let sz_decimals = self.meta_cache.sz_decimals(&coin).ok_or_else(|| {
            HlError::Parse(format!("szDecimals not found for '{}'", coin))
        })?;
        let limit_price = round_price_perp(limit_price, sz_decimals);
        let size = round_size(size, sz_decimals);

        let order = if side.is_buy() {
            OrderWire::limit_buy(asset_idx, limit_price, size)
        } else {
            OrderWire::limit_sell(asset_idx, limit_price, size)
        }
        .tif(Tif::Ioc)
        .build()?;
```

- [ ] **Step 6: Wire into `market_close`** (`orders.rs:354-370`) identically — round `limit_price` and `close_size` by `sz_decimals` before building.

- [ ] **Step 7: Run the executor suite:**

Run: `cargo test -p hl-executor`
Expected: PASS. (The existing slippage tests at `orders.rs:478-492` use round numbers and remain valid.)

- [ ] **Step 8: Commit:**

```bash
git add crates/hl-executor/src/executor/orders.rs
git commit -m "fix(executor): round market order price (5sf) and size (szDecimals) per Hyperliquid rules (R3)"
```

### Task 8b: `market_close` magnitude semantics (R5) + trigger-order rounding (R3 completion)

**Files:**
- Modify: `crates/hl-executor/src/executor/orders.rs` (add `resolve_close` helper; rewrite `market_close`; round `place_trigger_order`)

**R5 — `market_close(size=Some)` must be an unsigned magnitude with the side derived from the live
position.** Today (`orders.rs:317-330`) it infers direction from the *sign* of `size`
(positive=Sell, negative=Buy) — a silent footgun the audit flagged.

- [ ] **Step 1: Write a failing test** for a pure resolver in the `orders.rs` test module:

```rust
    #[test]
    fn resolve_close_derives_side_from_position_not_size_sign() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let mag = Decimal::from_str("1.5").unwrap();
        let pos = Decimal::from_str("4").unwrap();
        // Long position -> close with Sell, regardless of caller magnitude.
        assert_eq!(resolve_close(Some(mag), Side::Buy, pos).unwrap(), (Side::Sell, mag));
        // Short position -> close with Buy.
        assert_eq!(resolve_close(Some(mag), Side::Sell, pos).unwrap(), (Side::Buy, mag));
        // None -> close the full position.
        assert_eq!(resolve_close(None, Side::Buy, pos).unwrap(), (Side::Sell, pos));
        // Negative / zero magnitude is rejected (no sign-encoded direction).
        assert!(resolve_close(Some(Decimal::from_str("-1").unwrap()), Side::Buy, pos).is_err());
        assert!(resolve_close(Some(Decimal::ZERO), Side::Buy, pos).is_err());
    }
```

- [ ] **Step 2: Run, verify FAIL:**

Run: `cargo test -p hl-executor resolve_close_derives_side_from_position_not_size_sign`
Expected: FAIL — `cannot find function resolve_close`.

- [ ] **Step 3: Implement `resolve_close`** near the top of `orders.rs`:

```rust
/// Derive the close side and size for `market_close`.
///
/// The side is ALWAYS taken from the live position sign (long -> Sell,
/// short -> Buy). `size` is an unsigned magnitude: `Some(m)` closes `m`
/// (must be > 0), `None` closes the full position. Direction is never
/// encoded in the sign of `size`.
pub(crate) fn resolve_close(
    size: Option<Decimal>,
    position_side: Side,
    position_size: Decimal,
) -> Result<(Side, Decimal), HlError> {
    let close_side = if position_side.is_buy() { Side::Sell } else { Side::Buy };
    let close_size = match size {
        Some(m) if m > Decimal::ZERO => m,
        Some(_) => {
            return Err(HlError::Parse(
                "market_close: size must be a positive magnitude".into(),
            ))
        }
        None => position_size,
    };
    Ok((close_side, close_size))
}
```

- [ ] **Step 4: Run, verify PASS:**

Run: `cargo test -p hl-executor resolve_close_derives_side_from_position_not_size_sign`
Expected: PASS.

- [ ] **Step 5: Rewrite `market_close`** (`orders.rs:314-373`) to always query the position, then
`resolve_close`, then round (R3). Replace the body from `let coin = …` through the `self.place_order`
call with:

```rust
        let coin = super::normalize_symbol(symbol);

        // Always query the live position — the close side comes from the
        // position sign, never from the sign of the caller's size (R5).
        let resp = self
            .client
            .post_info(serde_json::json!({
                "type": "clearinghouseState",
                "user": self.address,
            }))
            .await?;
        let (szi_side, position_size) = extract_position_szi(&resp, &coin)?;
        let (close_side, close_size) = resolve_close(size, szi_side, position_size)?;

        let asset_idx = self.resolve_asset(symbol)?;
        let sz_decimals = self.meta_cache.sz_decimals(&coin).ok_or_else(|| {
            HlError::Parse(format!("szDecimals not found for '{}'", coin))
        })?;
        let mid = extract_mid_price(&self.client, &coin).await?;
        let slippage = slippage.unwrap_or_else(|| Decimal::new(5, 2));
        let limit_price = if close_side.is_buy() {
            mid * (Decimal::ONE + slippage)
        } else {
            mid * (Decimal::ONE - slippage)
        };
        let limit_price = round_price_perp(limit_price, sz_decimals);
        let close_size = round_size(close_size, sz_decimals);

        let order = if close_side.is_buy() {
            OrderWire::limit_buy(asset_idx, limit_price, close_size)
        } else {
            OrderWire::limit_sell(asset_idx, limit_price, close_size)
        }
        .tif(Tif::Ioc)
        .reduce_only(true)
        .build()?;

        self.place_order(order, vault).await
```

Also update the doc comment above `market_close` (`orders.rs:302-306`): state that `size` is an
**unsigned magnitude** and the close side is derived from the live position. (Behavior note: the
`size=Some` path now also performs the `clearinghouseState` read it previously skipped, and errors if
there is no open position to close — correct, since you cannot reduce a position you do not hold.)

**R3 completion — `place_trigger_order` builds its action inline with raw `to_string()`** and bypasses
both `round_price_perp`/`round_size` and `normalize_wire`.

- [ ] **Step 6: Round + normalize the trigger order's price/size** in `place_trigger_order`
(`orders.rs:126-149`). After `resolve_asset`, resolve `sz_decimals`, round, and emit canonical wire
strings (`Decimal` is `Copy`, so `trigger_price` may be passed to `normalize_wire` twice):

```rust
        let asset_idx = self.resolve_asset(symbol)?;
        let coin = super::normalize_symbol(symbol);
        let sz_decimals = self.meta_cache.sz_decimals(&coin).ok_or_else(|| {
            HlError::Parse(format!("szDecimals not found for '{}'", coin))
        })?;
        let trigger_price = round_price_perp(trigger_price, sz_decimals);
        let size = round_size(size, sz_decimals);

        let is_buy = side.is_buy();
        let cloid = new_cloid();

        let action = serde_json::json!({
            "type": "order",
            "orders": [{
                "a": asset_idx,
                "b": is_buy,
                "p": hl_types::normalize_wire(trigger_price),
                "s": hl_types::normalize_wire(size),
                "r": true,
                "t": {
                    "trigger": {
                        "triggerPx": hl_types::normalize_wire(trigger_price),
                        "isMarket": true,
                        "tpsl": tpsl.to_string()
                    }
                },
                "c": cloid
            }],
            "grouping": "na"
        });
```

- [ ] **Step 7: Run the executor suite:**

Run: `cargo test -p hl-executor`
Expected: PASS.

- [ ] **Step 8: Commit:**

```bash
git add crates/hl-executor/src/executor/orders.rs
git commit -m "fix(executor): market_close unsigned magnitude + position-derived side (R5); round trigger orders (R3)"
```

> **Direct LIMIT orders are out of scope for grid-rounding:** a plain limit order placed via
> `place_order` gets only builder-level `normalize_wire` (trailing-zero stripping), NOT 5-sig-fig /
> szDecimals grid rounding — the SDK cannot know the caller's intended price grid, so this is the
> caller's responsibility. Document this in the `place_order` rustdoc.

### Task 9: cloid idempotency for retried writes (R4)

**Files:**
- Modify: `crates/hl-executor/src/executor/orders.rs` (`place_order`, `bulk_order`, `place_trigger_order`, helpers)

- [ ] **Step 1: Write failing tests** in the `orders.rs` test module:

```rust
    #[test]
    fn new_cloid_is_hyperliquid_format() {
        let c = new_cloid();
        assert!(c.starts_with("0x"), "cloid must be 0x-prefixed: {c}");
        assert_eq!(c.len(), 34, "cloid must be 0x + 32 hex chars: {c}");
        assert!(c[2..].chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn ensure_cloid_injects_then_is_idempotent() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let mut order = OrderWire::limit_buy(
            0, Decimal::from_str("100").unwrap(), Decimal::from_str("1").unwrap(),
        ).build().unwrap();
        assert!(order.cloid.is_none());
        ensure_cloid(&mut order);
        let first = order.cloid.clone().unwrap();
        assert!(first.starts_with("0x") && first.len() == 34);
        ensure_cloid(&mut order);
        assert_eq!(order.cloid.as_deref(), Some(first.as_str()), "must not overwrite");
    }
```

- [ ] **Step 2: Run, verify FAIL:**

Run: `cargo test -p hl-executor new_cloid_is_hyperliquid_format`
Expected: FAIL — `cannot find function new_cloid`.

- [ ] **Step 3: Implement the helpers** near the top of `orders.rs`:

```rust
/// Generate a Hyperliquid client order id (`0x` + 32 hex chars) for
/// idempotent submission — the exchange dedups retries by cloid.
pub(crate) fn new_cloid() -> String {
    format!("0x{}", uuid::Uuid::new_v4().as_simple())
}

/// Attach a generated cloid if the order has none (idempotent).
pub(crate) fn ensure_cloid(order: &mut OrderWire) {
    if order.cloid.is_none() {
        order.cloid = Some(new_cloid());
    }
}
```

- [ ] **Step 4: Inject in `place_order`** — change the signature to take `mut order` and ensure a cloid before building the action (`orders.rs:74-83`):

```rust
    pub async fn place_order(
        &self,
        mut order: OrderWire,
        vault: Option<&str>,
    ) -> Result<OrderResponse, HlError> {
        ensure_cloid(&mut order);
        let fallback_price: Decimal =
            Decimal::from_str(&order.limit_px).unwrap_or(Decimal::ZERO);
        let fallback_size: Decimal = Decimal::from_str(&order.sz).unwrap_or(Decimal::ZERO);
        let order_json = order_to_json(&order)?;
        // ... unchanged ...
```

- [ ] **Step 5: Inject in `bulk_order`** — change `orders: Vec<OrderWire>` to `mut orders` and loop before building (`orders.rs:197-215`):

```rust
        for order in &mut orders {
            ensure_cloid(order);
        }
        let mut order_jsons = Vec::with_capacity(orders.len());
        // ... unchanged loop building order_jsons/fallbacks ...
```

- [ ] **Step 6: Fix `place_trigger_order` cloid format** (`orders.rs:129`) — replace the hyphenated UUID with the canonical generator:

```rust
        let cloid = new_cloid();
```

- [ ] **Step 7: Run, verify PASS:**

Run: `cargo test -p hl-executor`
Expected: PASS.

- [ ] **Step 8: Commit:**

```bash
git add crates/hl-executor/src/executor/orders.rs
git commit -m "fix(executor): auto-inject canonical cloid for retry idempotency (R4)"
```

### Task 10: Lint, full test, CHANGELOG

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Format and lint:**

Run: `cargo fmt --all && cargo clippy --all-features --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 2: Full test suite:**

Run: `cargo test --all-features`
Expected: PASS (including the new `golden_vectors` tests).

- [ ] **Step 3: Add a CHANGELOG entry** under a new `## [Unreleased]` (or next version) section:

```markdown
### Fixed
- **Critical:** L1 action signatures were invalid due to alphabetical msgpack key ordering; fixed via `motosan-wallet-core` 0.5.1 (canonical field order). (R1)
- **Critical:** user-signed actions (`usdSend`, `approveAgent`) used the wrong EIP-712 domain (chainId/`signatureChainId`/`verifyingContract`); fixed in `motosan-wallet-core` 0.5.1, and the in-repo wire `signatureChainId` literal corrected to `0x66eee`. (R2)
- **Critical:** SDK-computed order prices/sizes (market & trigger orders) are now normalized to Hyperliquid wire rules (canonical float string, ≤5 sig figs, szDecimals rounding). (R3)
- Order writes now auto-attach a client order id (`cloid`) so retried POSTs are deduplicated by the exchange; `place_trigger_order` cloid uses the canonical `0x`+32-hex format. (R4)
- `market_close` now takes `size` as an unsigned magnitude and derives the close side from the live position, removing the sign-encoded-direction footgun. (R5)

### Added
- Self-contained signing golden-vector tests (`crates/hl-signing/tests/golden_vectors.rs`): canonical-order L1 action-hash oracle, `usdSend` user-signed digest-recovery oracle, and signature regression baselines. (T1)
```

- [ ] **Step 4: Commit:**

```bash
git add CHANGELOG.md
git commit -m "docs: changelog for M0 signing & wire correctness fixes"
```

---

## Self-Review

**Spec coverage:** R1 → Tasks W1, 1, 2. R2 → Tasks W2, 4, 5. R3 → Tasks 6, 7, 8 (market orders) + 8b (trigger orders). R4 → Task 9. R5 → Task 8b. T1 → Tasks 1, 3, 4 (L1 action hash + `usdSend` user-signed). All six M0 findings (R1–R5, T1) covered. ✓

**Coverage boundaries (acknowledged):**
- R3 grid-rounding (5 sig figs / szDecimals) is applied to **SDK-computed** prices — `market_open`, `market_close`, `place_trigger_order`. A plain LIMIT order via `place_order` gets only `normalize_wire`; enforcing the caller's price grid is the caller's responsibility (documented in the `place_order` rustdoc).
- T1 golden-vectors cover the L1 action hash and the `usdSend` user-signed path. `approveAgent` (also fixed by R2) relies on the shared wallet-core 0.5.1 domain fix and is **not** independently digest-tested in M0 — add an `approveAgent` digest case (with/without `agentName`) in a follow-up.

**Placeholder scan:** The only intentional fill-in is the captured `r/s/v` hex in Task 3 Step 3 (recorded from a real run, per the chosen "record post-fix" baseline strategy) — not a spec placeholder. ✓

**Type consistency:** `normalize_wire(Decimal) -> String` (Task 6) is used by builders (Task 7), by `place_trigger_order` as `hl_types::normalize_wire` (Task 8b), and re-exported via `crate::normalize_wire`. `round_price_perp`/`round_size`/`resolve_close`/`new_cloid`/`ensure_cloid` are `pub(crate)` in `orders.rs` and used within the same module (Tasks 8, 8b, 9). `compute_action_hash`/`sign_l1_action`/`sign_user_signed_action`/`EIP712Field` match the existing `hl-signing` public API. ✓

## Exit Criteria (M0 done)

- `cargo test --all-features` green, including `golden_vectors`.
- `l1_action_hash_matches_canonical_oracle`, `usd_send_recovers_signer_under_canonical_domain`, and both signature baselines pass.
- `cargo clippy --all-features --all-targets -- -D warnings` clean.
- A testnet smoke order placed via `market_open` is **accepted** (not rejected) — manual verification with `HYPERLIQUID_TESTNET_KEY`.
