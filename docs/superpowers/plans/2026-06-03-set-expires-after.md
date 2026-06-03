# `set_expires_after` (action-expiry) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **Part A lands in the `motosan-wallet-core` repo; Part B lands in `motosan-hyperliquid` and depends on Part A being published.**

**Goal:** Let callers attach an `expiresAfter` timestamp so signed L1 actions are rejected by the exchange after that time (replay-window / stale-order protection), with byte-exact parity to the official Hyperliquid Python SDK.

**Architecture:** The expiry must be folded into the **signed L1 action hash** (`msgpack(action) ‖ nonce ‖ vault-flag ‖ 0x00 ‖ expiresAfter`) AND sent as a top-level field in the `/exchange` POST body. The hashing already exists in `motosan-wallet-core` 0.5.2 (`compute_action_hash_with_expiry`); the gap is that `sign_l1_action` doesn't thread it through and the EIP-712 signing primitives (`Agent`, the Exchange domain, `sign_hash`) are **private to wallet-core**. So a new `sign_l1_action_with_expiry` must be added upstream, then wired through `hl-signing → hl-executor → hl-client`.

**Tech Stack:** Rust, `alloy` (EIP-712), `rmp-serde` (msgpack), `serde_json`. External dep: `motosan-wallet-core` (separate repo, published to crates.io).

---

## Why this is blocked on wallet-core (read first)

Verified against the actual `motosan-wallet-core` 0.5.2 source
(`~/.cargo/registry/src/*/motosan-wallet-core-0.5.2/src/operations/hyperliquid.rs`):

- ✅ `compute_action_hash_with_expiry(action, nonce, vault_address, expires_after)` **exists and is public** (re-exported in `lib.rs`). Its body (`compute_action_hash_inner`) appends, when `expires_after = Some(ts)`: one `0x00` byte then `ts.to_be_bytes()` (8 bytes BE) after the vault flag — **byte-identical to the Python `action_hash`**.
- ❌ `sign_l1_action(signer, action, nonce, is_mainnet, vault_address)` takes **no** `expires_after`; it calls the no-expiry `compute_action_hash` (line ~192).
- ❌ The signing internals it uses — the `sol! { struct Agent }`, the `Eip712Domain { name:"Exchange", version:"1", chainId:1337, verifyingContract:0x0 }`, `SolStruct::eip712_signing_hash`, and `fn sign_hash` — are **all private** to that module. Only the top-level `pub fn`s are exported.

Therefore the expiry-aware hash cannot be signed from `motosan-hyperliquid` without re-implementing the Agent/Exchange EIP-712 + secp256k1 signing here — which violates the project rule "fix signing bugs upstream; `hl-signing` only wraps wallet-core" (see memory `walletcore-signing-delegation`). The correct fix is a ~20-line additive function in wallet-core.

### Python reference (ground truth for the golden vector)

```python
# hyperliquid/utils/signing.py
def action_hash(action, vault_address, nonce, expires_after):
    data = msgpack.packb(action)
    data += nonce.to_bytes(8, "big")
    if vault_address is None: data += b"\x00"
    else:                     data += b"\x01" + bytes.fromhex(vault_address[2:])
    if expires_after is not None:
        data += b"\x00"
        data += expires_after.to_bytes(8, "big")
    return keccak(data)

def sign_l1_action(wallet, action, active_pool, nonce, expires_after, is_mainnet):
    hash = action_hash(action, active_pool, nonce, expires_after)
    phantom_agent = {"source": "a" if is_mainnet else "b", "connectionId": hash}
    data = {... "Agent": [{"name":"source","type":"string"},{"name":"connectionId","type":"bytes32"}],
            "domain": {"name":"Exchange","version":"1","chainId":1337,"verifyingContract":"0x0..0"}, ...}
    return sign_inner(wallet, data)

# hyperliquid/exchange.py  _post_action(...)
payload = {"action": action, "nonce": nonce, "signature": signature,
           "vaultAddress": vault if action["type"] not in ("usdClassTransfer","sendAsset") else None,
           "expiresAfter": self.expires_after}   # ALWAYS present (null when unset)
```

`expiresAfter` is **unix epoch milliseconds**, `u64`. It is NOT inside the `action` object — it is top-level in the POST body and (when set) 8 BE bytes in the hash.

---

# Part A — `motosan-wallet-core` (do first, then publish)

**File:** `src/operations/hyperliquid.rs` (+ `src/lib.rs` re-export, + `tests/vectors/hyperliquid.json`).

### Task A1: Add `sign_l1_action_with_expiry` and make `sign_l1_action` delegate

The existing `sign_l1_action` body is duplicated into the new fn with the only change being `compute_action_hash` → `compute_action_hash_with_expiry`. To avoid drift, make the existing fn a thin delegate (mirrors how `compute_action_hash` delegates to `compute_action_hash_inner`).

- [ ] **Step 1: Write the failing tests** (append to the `#[cfg(test)] mod tests` block)

```rust
#[test]
fn test_sign_l1_action_with_expiry_none_matches_base() {
    let signer = test_signer();
    let action = serde_json::json!({"type": "order", "limit_px": "100.5"});
    let base = sign_l1_action(&signer, &action, 1000, true, None).unwrap();
    let with_none = sign_l1_action_with_expiry(&signer, &action, 1000, true, None, None).unwrap();
    assert_eq!(base, with_none, "None expiry must be byte-identical to legacy sign_l1_action");
}

#[test]
fn test_sign_l1_action_with_expiry_some_differs() {
    let signer = test_signer();
    let action = serde_json::json!({"type": "order"});
    let without = sign_l1_action_with_expiry(&signer, &action, 1, true, None, None).unwrap();
    let with_exp = sign_l1_action_with_expiry(&signer, &action, 1, true, None, Some(1_700_000_000_000)).unwrap();
    assert_ne!(without, with_exp, "a set expiry must change the signature");
}

#[test]
fn test_sign_l1_action_with_expiry_deterministic() {
    let signer = test_signer();
    let action = serde_json::json!({"type": "order"});
    let s1 = sign_l1_action_with_expiry(&signer, &action, 1, true, None, Some(9_999)).unwrap();
    let s2 = sign_l1_action_with_expiry(&signer, &action, 1, true, None, Some(9_999)).unwrap();
    assert_eq!(s1, s2);
}
```

- [ ] **Step 2: Run them, expect compile failure** — `cargo test -p motosan-wallet-core --features hyperliquid sign_l1_action_with_expiry` → FAIL: `cannot find function sign_l1_action_with_expiry`.

- [ ] **Step 3: Implement** — replace the existing `pub fn sign_l1_action` with the delegate + the new fn:

```rust
/// Sign a Hyperliquid L1 action using EIP-712 structured data.
///
/// Constructs an `Agent` typed struct with the action hash embedded as
/// `connectionId`, then signs it under the Exchange domain.
pub fn sign_l1_action(
    signer: &dyn HlSigner,
    action: &Value,
    nonce: u64,
    is_mainnet: bool,
    vault_address: Option<&str>,
) -> Result<HlSignature, WalletError> {
    sign_l1_action_with_expiry(signer, action, nonce, is_mainnet, vault_address, None)
}

/// Like [`sign_l1_action`] but folds an optional `expiresAfter` timestamp
/// (unix epoch **milliseconds**) into the signed action hash, so the exchange
/// rejects the action after that time.
///
/// When `expires_after` is `None` the signature is byte-identical to
/// [`sign_l1_action`].
pub fn sign_l1_action_with_expiry(
    signer: &dyn HlSigner,
    action: &Value,
    nonce: u64,
    is_mainnet: bool,
    vault_address: Option<&str>,
    expires_after: Option<u64>,
) -> Result<HlSignature, WalletError> {
    let action_hash = compute_action_hash_with_expiry(action, nonce, vault_address, expires_after)?;

    // source: "a" for mainnet, "b" for testnet (string type per Python SDK)
    let source = if is_mainnet { "a" } else { "b" };

    let agent = Agent {
        source: source.to_string(),
        connectionId: FixedBytes::from(action_hash),
    };

    // EIP-712 domain: Exchange v1, chainId 1337, verifyingContract 0x0...0
    let domain = alloy::sol_types::Eip712Domain {
        name: Some("Exchange".into()),
        version: Some("1".into()),
        chain_id: Some(alloy::primitives::U256::from(1337)),
        verifying_contract: Some(Address::ZERO),
        salt: None,
    };

    let signing_hash = alloy::sol_types::SolStruct::eip712_signing_hash(&agent, &domain);

    sign_hash(signer, signing_hash)
}
```

- [ ] **Step 4: Run the tests, expect PASS** — `cargo test -p motosan-wallet-core --features hyperliquid` (the 3 new + all existing `sign_l1_action*` tests, since the delegate must keep them green).

- [ ] **Step 5: Commit** — `git commit -m "feat(hyperliquid): add sign_l1_action_with_expiry"`

### Task A2: Re-export from `lib.rs`

- [ ] **Step 1:** In `src/lib.rs`, add `sign_l1_action_with_expiry` to the hyperliquid re-export block:

```rust
pub use operations::hyperliquid::{
    compute_action_hash, compute_action_hash_with_expiry, sign_l1_action,
    sign_l1_action_with_expiry, sign_user_signed_action, HlSignature, HlSigner, HlTypeField,
    LocalSigner,
};
```

- [ ] **Step 2:** `cargo build -p motosan-wallet-core --features hyperliquid` → builds. **Commit.**

### Task A3: Cross-language golden vector for the expiry path

The repo already has `tests/vectors/hyperliquid.json` + `test_vector_l1_signature_mainnet/testnet`. Add an expiry vector generated from the **official Python SDK** so the Rust signature is proven byte-equal.

- [ ] **Step 1: Generate the reference** with the Python SDK (one-off script, commit its output, not the script):

```python
# uses hyperliquid-python-sdk + eth_account
from hyperliquid.utils.signing import action_hash, sign_l1_action
from eth_account import Account
pk = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"  # hardhat #0
wallet = Account.from_key(pk)
action = {"type": "order", "orders": [], "grouping": "na"}
nonce, expires = 1700000000000, 1700000060000
h = action_hash(action, None, nonce, expires).hex()
for is_main in (True, False):
    sig = sign_l1_action(wallet, action, None, nonce, expires, is_main)
    print(is_main, h, sig)  # -> fill l1_action_with_expiry below
```

- [ ] **Step 2:** Extend `tests/vectors/hyperliquid.json` with an `l1_action_with_expiry` object holding `{action, nonce, expires_after, action_hash, mainnet:{is_mainnet,r,s,v}, testnet:{...}}` using the printed values. Extend the `VectorFile`/deserializer struct accordingly.

- [ ] **Step 3: Add the parity test:**

```rust
#[test]
fn test_vector_l1_signature_with_expiry() {
    let v = load_vectors();
    let e = v.l1_action_with_expiry; // new field
    let signer = LocalSigner::from_hex(&v.private_key).unwrap();

    let hash = compute_action_hash_with_expiry(&e.action, e.nonce, None, Some(e.expires_after)).unwrap();
    assert_eq!(format!("0x{}", hex::encode(hash)), e.action_hash);

    let sig = sign_l1_action_with_expiry(&signer, &e.action, e.nonce, e.mainnet.is_mainnet, None, Some(e.expires_after)).unwrap();
    assert_eq!((sig.r, sig.s, sig.v), (e.mainnet.r, e.mainnet.s, e.mainnet.v));
}
```

- [ ] **Step 4:** `cargo test -p motosan-wallet-core --features hyperliquid test_vector_l1_signature_with_expiry` → PASS. **Commit.**

### Task A4: Release wallet-core

- [ ] Bump `motosan-wallet-core` version (additive change → **0.5.3**), update its CHANGELOG, `cargo publish`. Record the published version for Part B.

---

# Part B — `motosan-hyperliquid` (after wallet-core 0.5.3 is published)

Reference symbols, not line numbers (lines shift). Verify each insertion point with codegraph/grep before editing.

### Task B1: Bump the wallet-core dependency

- [ ] `crates/hl-signing/Cargo.toml`: `motosan-wallet-core = { version = "0.5.3", features = ["hyperliquid"] }`. Run `cargo update -p motosan-wallet-core`. `cargo build` → OK. **Commit.**

### Task B2: `hl-signing` wrapper

**File:** `crates/hl-signing/src/eip712.rs` (mirror the existing `sign_l1_action` wrapper, which delegates to `motosan_wallet_core::sign_l1_action` via the `SingleAddressSigner` adapter), and `crates/hl-signing/src/lib.rs` (export).

- [ ] **Step 1: Failing test** (in `eip712.rs` tests, mirror `test_compute_action_hash_deterministic`):

```rust
#[test]
fn sign_l1_action_with_expiry_none_matches_base() {
    let signer = test_signer(); // existing helper
    let addr = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266";
    let action = serde_json::json!({"type": "order"});
    let base = sign_l1_action(signer.as_ref(), addr, &action, 1, true, None).unwrap();
    let exp = sign_l1_action_with_expiry(signer.as_ref(), addr, &action, 1, true, None, None).unwrap();
    assert_eq!(base.r, exp.r); assert_eq!(base.s, exp.s); assert_eq!(base.v, exp.v);
}
```

- [ ] **Step 2:** Run → FAIL (fn missing).
- [ ] **Step 3: Implement** in `eip712.rs` (and make the existing `sign_l1_action` delegate with `None`):

```rust
/// Like [`sign_l1_action`] but folds an optional `expiresAfter` (epoch ms) into
/// the signed action hash. `None` ⇒ identical to `sign_l1_action`.
#[allow(clippy::too_many_arguments)]
pub fn sign_l1_action_with_expiry(
    signer: &dyn Signer,
    address: &str,
    action: &serde_json::Value,
    nonce: u64,
    is_mainnet: bool,
    vault_address: Option<&str>,
    expires_after: Option<u64>,
) -> Result<Signature, HlError> {
    let adapter = SingleAddressSigner::new(signer, address.to_string());
    let hl_sig = motosan_wallet_core::sign_l1_action_with_expiry(
        &adapter, action, nonce, is_mainnet, vault_address, expires_after,
    )
    .map_err(|e| HlError::signing(e.to_string()))?;
    Ok(hl_signature_to_signature(&hl_sig))
}
```

- [ ] **Step 4:** Add `sign_l1_action_with_expiry` to the `pub use eip712::{...}` list in `crates/hl-signing/src/lib.rs`. `cargo test -p hl-signing` → PASS. **Commit.**

### Task B3: `HttpTransport::post_action` gains `expires_after`

> **Decision (document in code):** `expiresAfter` is **omitted from the POST body when `None`** (keeping existing action bodies byte-identical → zero regression, consistent with how `vaultAddress` is already handled) and sent as a JSON **integer when `Some`**. `expiresAfter` is not part of the signature, so this is wire-equivalent to Python's always-`null` form.

First: `grep -rn "fn post_action" crates/` to enumerate every impl (at least: the trait in `hl-client/src/transport.rs`, the inherent `HyperliquidClient::post_action` and the `HttpTransport` impl in `hl-client/src/client.rs`, and `MockTransport` in `hl-test-utils/src/lib.rs`). Update all.

- [ ] **Step 1:** Add the param to the trait (`crates/hl-client/src/transport.rs`):

```rust
async fn post_action(
    &self,
    action: serde_json::Value,
    signature: &Signature,
    nonce: u64,
    vault_address: Option<&str>,
    expires_after: Option<u64>,
) -> Result<serde_json::Value, HlError>;
```

- [ ] **Step 2:** In `crates/hl-client/src/client.rs` `post_action`, add the param and, after the `vaultAddress` insertion, insert the expiry when present:

```rust
if let Some(expires) = expires_after {
    let obj = payload
        .as_object_mut()
        .ok_or_else(|| HlError::serialization("payload is not a JSON object"))?;
    obj.insert("expiresAfter".to_string(), serde_json::Value::from(expires));
}
```

Update the inherent method and the trait `impl` (both `post_action` in `client.rs`) and every internal caller of `post_action` (e.g. `usdc_transfer`/`withdraw`/`spot_send`/`send_asset` in `hl-executor/src/executor/transfer.rs` and `approve_agent` in `admin.rs` call `self.client.post_action(...)` directly — pass `None` for `expires_after` unless they should honor it; see Task B4 note).

- [ ] **Step 3:** Update `MockTransport::post_action` in `crates/hl-test-utils/src/lib.rs` to accept `expires_after: Option<u64>` and capture it (extend the request-capture added for the parity wire tests — e.g. record `(action, expires_after)` or fold expiry into the captured value).
- [ ] **Step 4:** `cargo build --workspace --all-features` → green (every call site updated). **Commit.**

### Task B4: `OrderExecutor::set_expires_after` + thread through `send_signed_action`

**File:** `crates/hl-executor/src/executor/mod.rs`.

> **State representation:** store `expires_after: std::sync::atomic::AtomicU64` with **`0` = unset** (matches the existing lock-free `nonce: AtomicU64` field; an epoch-0 expiry is meaningless). The public API uses `Option<u64>`; `set_expires_after(Some(0))` maps to unset — document this. (If representing epoch-0 is ever needed, switch to `Mutex<Option<u64>>`.)

- [ ] **Step 1: Failing test** (in executor tests, using the capturing `test_executor_capturing`):

```rust
#[tokio::test]
async fn set_expires_after_threads_into_post_body() {
    let (executor, transport) = test_executor_capturing(vec![ok_resting_response(1)]);
    executor.set_expires_after(Some(1_700_000_060_000));
    let order = OrderWire::limit_buy(0, Decimal::from(90000), Decimal::from(1)).build().unwrap();
    executor.place_order(order, None).await.unwrap();
    // MockTransport now records expires_after alongside the action:
    assert_eq!(transport.last_expires_after(), Some(1_700_000_060_000));
}
```

- [ ] **Step 2:** Run → FAIL (`set_expires_after` / `last_expires_after` missing).
- [ ] **Step 3: Implement.** Add the field + init in all constructors (`new`, `with_meta_cache`):

```rust
expires_after: std::sync::atomic::AtomicU64::new(0),
```

Add the accessor + setter near the other small getters (`address()`, `meta_cache()`):

```rust
/// Set (or clear with `None`) an `expiresAfter` timestamp (unix epoch ms)
/// applied to every subsequent signed action. `Some(0)` is treated as unset.
pub fn set_expires_after(&self, expires_after: Option<u64>) {
    use std::sync::atomic::Ordering;
    self.expires_after.store(expires_after.unwrap_or(0), Ordering::Release);
}

/// The currently configured `expiresAfter`, if any.
pub fn expires_after(&self) -> Option<u64> {
    use std::sync::atomic::Ordering;
    match self.expires_after.load(Ordering::Acquire) {
        0 => None,
        ts => Some(ts),
    }
}
```

Thread it through `send_signed_action` — read the expiry, sign with it, and pass it to `post_action`:

```rust
pub(crate) async fn send_signed_action(
    &self,
    action: serde_json::Value,
    vault: Option<&str>,
) -> Result<serde_json::Value, HlError> {
    let nonce = self.next_nonce();
    let expires_after = self.expires_after();
    let signature = sign_l1_action_with_expiry(
        self.signer.as_ref(),
        &self.address,
        &action,
        nonce,
        self.client.is_mainnet(),
        vault,
        expires_after,
    )?;
    let result = self
        .client
        .post_action(action, &signature, nonce, vault, expires_after)
        .await?;
    // ... unchanged status check ...
}
```

Update the `use hl_signing::...` import to bring in `sign_l1_action_with_expiry`.

> **Note:** the user-signed actions (`usdSend`/`withdraw3`/`spotSend`/`sendAsset`/`approveAgent`) call `self.client.post_action(...)` directly, NOT through `send_signed_action`, and use EIP-712 (not L1) signing — `expiresAfter` does **not** apply to them in the Python SDK. Pass `None` for their `post_action` `expires_after` arg.

- [ ] **Step 4:** Add `last_expires_after()` to `MockTransport` (returns the captured expiry of the last action). `cargo test --workspace --all-features` → PASS. **Commit.**

### Task B5: Docs + changelog

- [ ] Add a `set_expires_after` line to the CHANGELOG `[Unreleased]` (or the version being cut), and mention it in `skills/motosan-hyperliquid/references/execution.md`. **Commit.**

---

## Self-review checklist

- [ ] `expires_after` is **epoch milliseconds**, `u64`, 8 BE bytes in the hash (same scale as `nonce`).
- [ ] `None` expiry produces **byte-identical** signatures and POST bodies to today (delegation tests + omit-when-None body).
- [ ] `expiresAfter` is **top-level** in the POST body, never inside the `action` object.
- [ ] Cross-language golden vector (Task A3) proves the signed bytes match the Python SDK.
- [ ] wallet-core dep bumped and published **before** Part B; `cargo update -p motosan-wallet-core` run.
- [ ] All `post_action` impls + call sites updated (trait, real client ×2, MockTransport, direct callers in transfer.rs/admin.rs).
