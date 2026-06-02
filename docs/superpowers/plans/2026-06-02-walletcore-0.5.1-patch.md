# motosan-wallet-core 0.5.1 — Patch (R1 + R2)

> ⚠️ **SUPERSEDED (2026-06-02):** A `0.5.1` was already published on crates.io that contains the R1
> (`preserve_order`) and `verifyingContract` fixes from this doc, but **not** the R2 `chainId` fix.
> Use **`2026-06-02-walletcore-0.5.2-patch.md`** for the remaining (minimal) R2 change. This doc is
> kept for reference only.

> Apply these in the **`motosan-wallet-core`** repo (github.com/motosan-dev/motosan-wallet), then
> publish `0.5.1`. This is Track W of the M0 plan; the `motosan-hyperliquid` repo bumps to `0.5.1`
> afterward (M0 Task 2). Line numbers reference the published `0.5.0` source
> (`src/operations/hyperliquid.rs`); match on the function bodies, not the line numbers, since your
> working tree may differ slightly.

All edits are in two files: `Cargo.toml` and `src/operations/hyperliquid.rs`.

---

## Patch 1 — `Cargo.toml`: enable `preserve_order` + bump version (R1)

**Why:** `compute_action_hash_inner` msgpacks the action via `rmp_serde::to_vec_named(&Value)`. Without
`serde_json`'s `preserve_order`, `Value::Object` is a `BTreeMap` and keys serialize **alphabetically**
(`a,b,p,s,r,t` → `a,b,p,r,s,t`), producing a hash Hyperliquid rejects. Enabling `preserve_order` makes
`Value::Object` insertion-ordered; via Cargo **feature unification** this also fixes every downstream
consumer (e.g. `motosan-hyperliquid`) whose `serde_json::json!` actions are already built in canonical
field order.

```diff
 [package]
 name = "motosan-wallet-core"
-version = "0.5.0"
+version = "0.5.1"

 [dependencies]
-serde_json = "1"
+serde_json = { version = "1", features = ["preserve_order"] }
```

> If your real `Cargo.toml` uses the table form (`[dependencies.serde_json]`), add `features = ["preserve_order"]` there instead.

---

## Patch 2 — `src/operations/hyperliquid.rs`: fix the user-signed EIP-712 domain (R2)

**Why:** `sign_user_signed_action` signs `usdSend`/`approveAgent`/`withdraw3` etc. The 0.5.0 code uses
domain `chainId = 42161` on mainnet (canonical is **421614** = `0x66eee` for *both* networks), writes
`signatureChainId` as a **decimal** string, and builds a **3-field** domain separator missing
`verifyingContract`. All three diverge from Hyperliquid/the Python SDK, so the signature is rejected.

### 2a. Extract `user_signed_hash` and fix the domain/chainId/signatureChainId

Replace the body of `sign_user_signed_action` (0.5.0 lines ~227-262) so it delegates to a new
`user_signed_hash` helper, and fix the three values:

```rust
/// Compute the canonical EIP-712 signing hash for a Hyperliquid user-signed action.
///
/// Domain: { name: "HyperliquidSignTransaction", version: "1", chainId: 421614,
///           verifyingContract: 0x0 }. `signatureChainId` is always "0x66eee"
/// (Arbitrum Sepolia, 421614) for BOTH mainnet and testnet — the network is
/// conveyed by `hyperliquidChain`, not the chainId. Matches the Python SDK.
fn user_signed_hash(
    action: &Value,
    type_fields: &[HlTypeField<'_>],
    primary_type: &str,
    is_mainnet: bool,
) -> Result<B256, WalletError> {
    const SIGNATURE_CHAIN_ID: &str = "0x66eee";
    let chain_id: u64 = 421614;

    let domain = alloy::sol_types::Eip712Domain {
        name: Some("HyperliquidSignTransaction".into()),
        version: Some("1".into()),
        chain_id: Some(alloy::primitives::U256::from(chain_id)),
        verifying_contract: Some(Address::ZERO),
        salt: None,
    };

    // Clone action and inject chain fields (used only for the struct hash).
    let mut action_obj = action.clone();
    let map = action_obj
        .as_object_mut()
        .ok_or_else(|| WalletError::SerializationError("action must be a JSON object".into()))?;

    let hl_chain = if is_mainnet { "Mainnet" } else { "Testnet" };
    map.insert("hyperliquidChain".into(), Value::String(hl_chain.into()));
    map.insert(
        "signatureChainId".into(),
        Value::String(SIGNATURE_CHAIN_ID.into()),
    );

    let domain_separator = compute_domain_separator(&domain);
    let struct_hash = compute_struct_hash(primary_type, type_fields, &action_obj)?;

    // EIP-712 final hash: keccak256("\x19\x01" || domainSeparator || structHash)
    let mut hasher = Keccak256::new();
    hasher.update([0x19, 0x01]);
    hasher.update(domain_separator.as_slice());
    hasher.update(struct_hash.as_slice());
    Ok(hasher.finalize())
}

pub fn sign_user_signed_action(
    signer: &dyn HlSigner,
    action: &Value,
    type_fields: &[HlTypeField<'_>],
    primary_type: &str,
    is_mainnet: bool,
) -> Result<HlSignature, WalletError> {
    let signing_hash = user_signed_hash(action, type_fields, primary_type, is_mainnet)?;
    sign_hash(signer, signing_hash)
}
```

**Net changes vs 0.5.0:**
- `chain_id`: `if is_mainnet { 42161 } else { 421614 }` → **`421614`** (both networks).
- `verifying_contract`: `None` → **`Some(Address::ZERO)`**.
- `signatureChainId`: `Value::String(chain_id.to_string())` (decimal `"42161"`) → **`Value::String("0x66eee".into())`**.
- The signing-hash assembly is extracted into `user_signed_hash` so it can be unit-tested (Patch 3b).

> `Address` is already imported (`use alloy::primitives::{Address, FixedBytes, Keccak256, B256};`).

### 2b. Make `compute_domain_separator` 4-field (include `verifyingContract`)

This function is used **only** by the user-signed path (verified: the sole caller is
`user_signed_hash`; `eip712.rs::compute_domain_separator_from_json` is a separate function, and
`sign_l1_action` uses alloy's `eip712_signing_hash` directly — neither is touched). Replace it:

```rust
/// Compute the EIP-712 domain separator (4-field, including verifyingContract).
fn compute_domain_separator(domain: &alloy::sol_types::Eip712Domain) -> B256 {
    // EIP712_DOMAIN_TYPEHASH — MUST include verifyingContract to match the
    // Hyperliquid / Python SDK domain, which always sets it.
    let type_hash = keccak256_bytes(
        b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
    );

    let name_hash = keccak256_bytes(domain.name.as_deref().unwrap_or("").as_bytes());
    let version_hash = keccak256_bytes(domain.version.as_deref().unwrap_or("").as_bytes());

    let mut chain_id_bytes = [0u8; 32];
    if let Some(cid) = domain.chain_id {
        chain_id_bytes = cid.to_be_bytes::<32>();
    }

    // verifyingContract: 20-byte address, left-padded to 32 bytes.
    let mut verifying_contract_bytes = [0u8; 32];
    if let Some(addr) = domain.verifying_contract {
        verifying_contract_bytes[12..].copy_from_slice(addr.as_slice());
    }

    let mut hasher = Keccak256::new();
    hasher.update(type_hash.as_slice());
    hasher.update(name_hash.as_slice());
    hasher.update(version_hash.as_slice());
    hasher.update(chain_id_bytes);
    hasher.update(verifying_contract_bytes);
    hasher.finalize()
}
```

**Net change vs 0.5.0:** typehash string gains `,address verifyingContract`; the 32-byte left-padded
verifying-contract is appended to the hash input.

> The L1 action path is unaffected by Patch 2 — `sign_l1_action` already uses alloy's
> `SolStruct::eip712_signing_hash` with `verifying_contract: Some(Address::ZERO)` (correct). R1
> (Patch 1) is what fixes the L1 signatures.

---

## Patch 3 — Tests (add to the `#[cfg(test)] mod tests` in `hyperliquid.rs`)

Run with the feature: `cargo test --features hyperliquid`.

### 3a. R1 — canonical msgpack field order

```rust
    #[test]
    fn action_hash_uses_canonical_field_order() {
        use serde::Serialize;

        // Oracle: declaration-order structs (serde serializes struct fields in
        // declaration order, never alphabetically) -> the canonical Hyperliquid bytes.
        #[derive(Serialize)]
        struct Tif { tif: &'static str }
        #[derive(Serialize)]
        struct Limit { limit: Tif }
        #[derive(Serialize)]
        struct Order { a: u32, b: bool, p: &'static str, s: &'static str, r: bool, t: Limit }
        #[derive(Serialize)]
        struct OrderAction {
            #[serde(rename = "type")]
            type_: &'static str,
            orders: Vec<Order>,
            grouping: &'static str,
        }

        let oracle_action = OrderAction {
            type_: "order",
            orders: vec![Order {
                a: 0, b: true, p: "30000", s: "0.1", r: false,
                t: Limit { limit: Tif { tif: "Gtc" } },
            }],
            grouping: "na",
        };
        let mut oracle_bytes = rmp_serde::to_vec_named(&oracle_action).unwrap();
        oracle_bytes.extend_from_slice(&1_700_000_000_000u64.to_be_bytes());
        oracle_bytes.push(0x00);
        let oracle_hash: [u8; 32] = {
            let mut h = Keccak256::new();
            h.update(&oracle_bytes);
            h.finalize().into()
        };

        // Production: same action built as serde_json::Value in canonical insertion
        // order. Matches the oracle ONLY when preserve_order is enabled (Patch 1).
        let action = serde_json::json!({
            "type": "order",
            "orders": [{
                "a": 0, "b": true, "p": "30000", "s": "0.1", "r": false,
                "t": { "limit": { "tif": "Gtc" } }
            }],
            "grouping": "na"
        });
        let produced = compute_action_hash(&action, 1_700_000_000_000, None).unwrap();
        assert_eq!(
            produced, oracle_hash,
            "action hash must use canonical field order — enable serde_json preserve_order (Patch 1)"
        );
    }
```

> Without Patch 1 this test is **RED** (production emits alphabetical `a,b,p,r,s,t`); with Patch 1 it
> is **GREEN**. It's the regression guard for R1.

### 3b. R2 — user-signed digest uses the canonical domain

```rust
    fn keccak(bytes: &[u8]) -> [u8; 32] {
        let mut h = Keccak256::new();
        h.update(bytes);
        h.finalize().into()
    }

    /// Independent oracle for the usdSend EIP-712 digest (chainId 421614,
    /// verifyingContract 0x0). signatureChainId is NOT a hashed field.
    fn oracle_usd_send_digest(destination: &str, amount: &str, time: u64, is_mainnet: bool) -> [u8; 32] {
        let hl_chain = if is_mainnet { "Mainnet" } else { "Testnet" };

        let domain_type = keccak(
            b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
        );
        let mut chain_id = [0u8; 32];
        chain_id[24..].copy_from_slice(&421614u64.to_be_bytes());
        let verifying_contract = [0u8; 32]; // zero address, left-padded
        let mut dbuf = Vec::new();
        dbuf.extend_from_slice(&domain_type);
        dbuf.extend_from_slice(&keccak(b"HyperliquidSignTransaction"));
        dbuf.extend_from_slice(&keccak(b"1"));
        dbuf.extend_from_slice(&chain_id);
        dbuf.extend_from_slice(&verifying_contract);
        let domain_separator = keccak(&dbuf);

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

        let mut fbuf = Vec::new();
        fbuf.extend_from_slice(&[0x19, 0x01]);
        fbuf.extend_from_slice(&domain_separator);
        fbuf.extend_from_slice(&struct_hash);
        keccak(&fbuf)
    }

    #[test]
    fn user_signed_hash_matches_canonical_domain() {
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
        let types = [
            HlTypeField::new("hyperliquidChain", "string"),
            HlTypeField::new("destination", "string"),
            HlTypeField::new("amount", "string"),
            HlTypeField::new("time", "uint64"),
        ];

        let produced = user_signed_hash(&action, &types, "HyperliquidTransaction:UsdSend", true).unwrap();
        let oracle = oracle_usd_send_digest(destination, amount, time, true);
        assert_eq!(
            produced.as_slice(), &oracle[..],
            "user-signed digest must use the canonical domain (chainId 421614, verifyingContract 0x0)"
        );
    }
```

> Without Patch 2 this is **RED** (3-field domain / chainId 42161); with Patch 2 it is **GREEN**.
> `HlTypeField::new(name, solidity_type)` and the private `user_signed_hash`/`compute_struct_hash`
> are all in-module, so no extra `use`.

---

## Verification & Release

- [ ] **Build + test with the feature:**

```bash
cargo fmt --all
cargo clippy --features hyperliquid --all-targets -- -D warnings
cargo test --features hyperliquid
```

Expected: the two new tests pass; all existing tests (incl. `test_compute_action_hash_*`,
`test_local_signer_*`) still pass. (Enabling `preserve_order` does not affect determinism tests.)

- [ ] **Sanity-check nothing else relies on alphabetical key order** — search the wallet-core crate
  for code that depends on `serde_json::Value` key iteration order (keystore JSON, chain RPC payloads).
  None is expected; confirm tests are green.

- [ ] **CHANGELOG** — add a `0.5.1` entry:

```markdown
## [0.5.1]
### Fixed
- Hyperliquid: serialize L1 action msgpack in canonical (insertion) field order by enabling
  serde_json `preserve_order`; alphabetical ordering produced invalid signatures. (R1)
- Hyperliquid: correct the user-signed EIP-712 domain — chainId 421614 (`0x66eee`) for both
  networks, `signatureChainId` as the hex string `0x66eee`, and a 4-field domain separator that
  includes `verifyingContract`. Fixes `usdSend`/`approveAgent`/etc. signature recovery. (R2)
```

- [ ] **Publish:**

```bash
cargo publish        # or push the release tag that triggers your publish workflow
```

- [ ] **Confirm** `motosan-wallet-core 0.5.1` is resolvable from crates.io, then proceed with
  `motosan-hyperliquid` M0 Task 2 (`cargo update -p motosan-wallet-core --precise 0.5.1`).

---

## Cross-repo consistency note

After this patch, the **wire body** `signatureChainId` (set in `motosan-hyperliquid`'s
`usdc_transfer`/`approve_agent`, M0 Task 5) and the **signed domain** `signatureChainId` (set here in
`user_signed_hash`) must both be `"0x66eee"`. They are independent code paths — wallet-core injects
into a hashing-only clone and never mutates the caller's posted action — so **both** must carry the
canonical value or the exchange's domain reconstruction will mismatch the recovered signer.
