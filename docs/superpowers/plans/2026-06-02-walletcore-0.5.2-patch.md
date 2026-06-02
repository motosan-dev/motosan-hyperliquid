# motosan-wallet-core 0.5.2 — Patch (remaining R2 fix)

> **Why this supersedes the 0.5.1 patch doc:** crates.io already has a published `0.5.1` that fixed
> R1 (`serde_json preserve_order`) and the user-signed domain's `verifyingContract` (4-field domain).
> Verified by reading the published `0.5.1` source. **The only remaining defect is the user-signed
> domain `chainId` (R2):** it is still network-dependent (`42161` mainnet / `421614` testnet) and
> `signatureChainId` is still emitted as a decimal string. The canonical Hyperliquid value — confirmed
> from the official Python SDK `utils/signing.py` — is **`chainId = 421614` (`0x66eee`) for BOTH
> networks**; `is_mainnet` only switches `hyperliquidChain`. This patch is the minimal delta
> `0.5.1 → 0.5.2`.

**Evidence (official Python SDK, `hyperliquid/utils/signing.py`):**
```python
action["signatureChainId"] = "0x66eee"                 # hardcoded, both networks
action["hyperliquidChain"] = "Mainnet" if is_mainnet else "Testnet"
chain_id = int(action["signatureChainId"], 16)         # = 421614, both networks
```

**Why R2 cannot be fixed downstream:** the exchange reconstructs the EIP-712 domain `chainId` as
`int(posted_signatureChainId, 16)` and checks the recovered signer. wallet-core signs with its own
domain `chainId`. With 0.5.1's network-dependent `chainId`, **no single posted `signatureChainId`
makes both mainnet and testnet recover correctly** — the fix must set wallet-core's domain `chainId`
to `421614` for both. (Today on 0.5.1: mainnet happens to work, testnet is broken; flipping the
in-repo literal to `0x66eee` would fix testnet and break mainnet.)

All edits are in **`motosan-wallet-core`**, two files: `Cargo.toml` and
`src/operations/hyperliquid.rs`. Line numbers reference the published `0.5.1` source.

> **Audit-confirmed (2026-06-02, 41-agent byte-exact audit of 0.5.1):** this is the **only** remaining
> signing change. 29 other parts of 0.5.1's signing stack were independently verified canonical and
> must NOT be touched — L1 action-hash (preserve_order, nonce BE, vault flag, expires marker), the L1
> Exchange domain, the 4-field user-signed domain-separator construction, and **every `eip712.rs`
> field encoder** (string→keccak(utf8), uint→32B BE, address→left-padded, bool). The single
> load-bearing defect is the user-signed **domain chainId** (`42161` mainnet vs canonical `421614`).
>
> **Impact nuance:** the motosan callers (`usdc_transfer`/`approve_agent`) do **not** list
> `signatureChainId` in their EIP-712 `type_fields`, so it is **not hashed** — only the **domain
> chainId** affects the signature. The exchange reconstructs the domain chainId as
> `int(posted_signatureChainId, 16)` and compares to the signer's domain chainId. So on 0.5.1 *as
> shipped* (posted literal `"0xa4b1"`=42161): mainnet happens to match (42161==42161) and **works**,
> testnet is broken (domain 421614 ≠ 42161). Setting the in-repo literal to `"0x66eee"` (M0 Task 5)
> without this wallet-core fix would flip it (testnet works, mainnet breaks). Only `chainId = 421614`
> here makes `"0x66eee"` correct on **both** networks. The `signatureChainId` line below is changed
> for consistency/defense (it matters for any caller that *does* hash it), but the chainId is the
> load-bearing fix.

---

## Patch 1 — `Cargo.toml`: bump version

```diff
 [package]
 name = "motosan-wallet-core"
-version = "0.5.1"
+version = "0.5.2"
```

(`serde_json` already has `features = ["preserve_order"]` in 0.5.1 — leave it.)

---

## Patch 2 — `hyperliquid.rs`: canonical user-signed `chainId` + hex `signatureChainId` (R2)

`sign_user_signed_action` (0.5.1 lines ~228-264). Two value changes plus a small refactor that
extracts the signing-hash computation into `user_signed_hash` so it can be unit-tested without
signing/recovery.

**Replace** the current 0.5.1 body:

```rust
    // Build EIP-712 domain — chainId differs between mainnet and testnet
    let chain_id: u64 = if is_mainnet { 42161 } else { 421614 };
    let domain = alloy::sol_types::Eip712Domain {
        name: Some("HyperliquidSignTransaction".into()),
        version: Some("1".into()),
        chain_id: Some(alloy::primitives::U256::from(chain_id)),
        verifying_contract: Some(Address::ZERO),
        salt: None,
    };

    // Clone action and inject chain fields
    let mut action_obj = action.clone();
    let map = action_obj
        .as_object_mut()
        .ok_or_else(|| WalletError::SerializationError("action must be a JSON object".into()))?;

    let hl_chain = if is_mainnet { "Mainnet" } else { "Testnet" };
    map.insert("hyperliquidChain".into(), Value::String(hl_chain.into()));
    map.insert(
        "signatureChainId".into(),
        Value::String(chain_id.to_string()),
    );

    // Manually compute EIP-712 struct hash for the dynamic type
    let domain_separator = compute_domain_separator(&domain);
    let struct_hash = compute_struct_hash(primary_type, type_fields, &action_obj)?;

    // EIP-712 final hash: keccak256("\x19\x01" || domainSeparator || structHash)
    let mut hasher = Keccak256::new();
    hasher.update([0x19, 0x01]);
    hasher.update(domain_separator.as_slice());
    hasher.update(struct_hash.as_slice());
    let signing_hash: B256 = hasher.finalize();

    sign_hash(signer, signing_hash)
```

**with** (extract `user_signed_hash`; fix the two values):

```rust
    let signing_hash = user_signed_hash(action, type_fields, primary_type, is_mainnet)?;
    sign_hash(signer, signing_hash)
}

/// Compute the canonical EIP-712 signing hash for a Hyperliquid user-signed action.
///
/// Hyperliquid user-signed actions ALWAYS use Arbitrum Sepolia chainId 421614
/// (`0x66eee`) for BOTH mainnet and testnet — the network is conveyed by
/// `hyperliquidChain`, not the chainId. (Matches the Python SDK: signatureChainId
/// = "0x66eee", domain chainId = int("0x66eee", 16) = 421614.)
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
```

**Net change vs 0.5.1:**
- `let chain_id: u64 = if is_mainnet { 42161 } else { 421614 };` → **`let chain_id: u64 = 421614;`**
- `Value::String(chain_id.to_string())` (decimal) → **`Value::String("0x66eee".into())`** (hex)
- signing-hash assembly extracted into `user_signed_hash` (so Patch 3 can test it directly).

> The L1 path (`sign_l1_action`), `compute_domain_separator` (already 4-field in 0.5.1),
> `compute_struct_hash`, and `compute_action_hash_inner` are **unchanged**.

---

## Patch 3 — Test: user-signed digest uses the canonical domain (add to `#[cfg(test)] mod tests`)

This is **RED on 0.5.1** (mainnet domain chainId 42161 ≠ oracle 421614) and **GREEN on 0.5.2**.
Run with: `cargo test --features hyperliquid user_signed_hash_matches_canonical_domain`.

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

        // is_mainnet = true is the case that is broken on 0.5.1 (domain chainId 42161).
        let produced = user_signed_hash(&action, &types, "HyperliquidTransaction:UsdSend", true).unwrap();
        let oracle = oracle_usd_send_digest(destination, amount, time, true);
        assert_eq!(
            produced.as_slice(), &oracle[..],
            "user-signed digest must use chainId 421614 for BOTH networks (R2)"
        );
    }
```

> `HlTypeField::new(name, solidity_type)`, `user_signed_hash`, `compute_struct_hash`,
> `compute_domain_separator`, and `Keccak256` are all in-module — no extra `use`.

---

## Verification & Release

- [ ] `cargo fmt --all`
- [ ] `cargo clippy --features hyperliquid --all-targets -- -D warnings`
- [ ] `cargo test --features hyperliquid` — the new test passes; existing tests still pass.
- [ ] **CHANGELOG** — add a `0.5.2` entry:

```markdown
## [0.5.2]
### Fixed
- Hyperliquid user-signed actions (usdSend/approveAgent/withdraw/spotSend) now sign with the
  canonical EIP-712 domain chainId 421614 (0x66eee) for BOTH mainnet and testnet, and emit
  signatureChainId as the hex string "0x66eee". 0.5.1 used a network-dependent chainId (42161 on
  mainnet) and a decimal signatureChainId, so the exchange's domain reconstruction did not match
  the recovered signer. (R2)
```

- [ ] `cargo publish` (or push the release tag).
- [ ] Confirm `motosan-wallet-core 0.5.2` resolves from crates.io.

---

## Downstream coupling (motosan-hyperliquid)

After 0.5.2 ships, the M0 plan's **Task 5** (set the `signatureChainId` wire literal to `"0x66eee"`
in `usdc_transfer`/`approve_agent`) becomes correct and necessary: the **posted** wire body and the
**signed** domain then agree at `421614` on both networks. Until 0.5.2, Task 5 must NOT be applied
(on 0.5.1 it would fix testnet but break mainnet). Track the M0 dependency as:

```
wallet-core 0.5.1 (R1 ✅, R2 ❌)  →  unblocks M0 Tasks 1,2,3 + in-repo 6,7,8,8b,9
wallet-core 0.5.2 (R2 ✅)         →  unblocks M0 Tasks 4,5  (usdSend oracle + signatureChainId literal)
```
