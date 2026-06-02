# M0 — Subagent Prompts (unblocked tasks)

Ready-to-dispatch prompts for the **0.5.2-unblocked** M0 tasks. Each subagent prompt =
**Shared Preamble** + the task-specific **Body** below it. Dispatch with the `Task` tool, `general-purpose`
(or `Senior Developer`) subagent type.

## How to run

- **Branch:** `m0-signing-wire-correctness`. **Working dir:** `/Users/daiwanwei/Projects/wade/motosan-hyperliquid`.
- **Current dependency state:** `Cargo.lock` is at `motosan-wallet-core 0.5.2` (verified byte-exact —
  R1 AND R2 fixed). So the signing oracle tests (Prompts 1 and 8) go **GREEN immediately** — they are
  regression guards, not red→green steps. **All M0 tasks are unblocked.**
- **Order (sequential — they touch the same files, never run two implementers in parallel):**
  `1 → 3 → 8(Task 4) → 6 → 7 → 9(Task 8) → 10(Task 9) → 11(Task 8b)`, with **Prompt 9 (Task 5)**
  runnable any time (it touches transfer.rs/admin.rs, not the others). Prompts 1/2/8 all touch
  `golden_vectors.rs`, so keep them ordered.
- **Per task, run the two-stage review** (Spec Reviewer, then Quality Reviewer at the bottom of this doc);
  loop fixes until both pass; then next task.
- **Commits:** follow the repo's `CLAUDE.md` conventions; if your global config requires a `Co-Authored-By`
  trailer, append it.
- Run lint after the batch: `cargo fmt --all && cargo clippy --all-features --all-targets -- -D warnings`.

---

## Shared Preamble (prepend to every implementer prompt)

```
You are implementing one task in the motosan-hyperliquid Rust SDK (a Cargo workspace of 6 crates:
hl-types < hl-signing < hl-client < hl-market/hl-account < hl-executor). All crates use
hl_types::HlError. Work from: /Users/daiwanwei/Projects/wade/motosan-hyperliquid on branch
m0-signing-wire-correctness. The dependency motosan-wallet-core is already at 0.5.2 in Cargo.lock.

Follow TDD: write the failing test first, run it to see it fail (unless noted otherwise), implement the
minimal code, run it to see it pass, then commit. Use the exact code given in the task — it is a
fully-specified plan, not a sketch. Do not over-build (YAGNI); follow existing patterns in the files
you touch.

## Before You Begin
If anything is unclear about requirements, approach, or assumptions, ASK before starting. Otherwise proceed.

## When you're in over your head
It is OK to stop. Report BLOCKED or NEEDS_CONTEXT with specifically what you're stuck on and what you tried.
Do not guess on signing/financial correctness.

## Before reporting: self-review
- Completeness: did you implement every step? edge cases?
- Quality: clear names, clean, matches surrounding style?
- Testing: do tests verify real behavior? did you run them and see the expected result?
Fix any issues before reporting.

## Report format
- Status: DONE | DONE_WITH_CONCERNS | BLOCKED | NEEDS_CONTEXT
- What you implemented; what you tested and the exact test output; files changed; the commit SHA;
  any concerns.
```

---

## Prompt 1 — L1 action-hash golden oracle (Task 1 + the 0.5.2 bump)

```
Implement Task 1: L1 action-hash golden-vector oracle, and bump the wallet-core dependency string.

## Task Description

Files:
- Modify: crates/hl-signing/Cargo.toml
- Create: crates/hl-signing/tests/golden_vectors.rs

Step 1 — In crates/hl-signing/Cargo.toml, bump the wallet-core dep and add dev-deps:
- Change the dependency `motosan-wallet-core = { version = "0.5.0", features = ["hyperliquid"] }`
  to `version = "0.5.2"` (0.5.2 fixes both R1 and R2; Cargo.lock is already at 0.5.2).
- Add/extend [dev-dependencies]:

    [dev-dependencies]
    tokio = { version = "1", features = ["full"] }
    rmp-serde = "1"
    serde = { version = "1", features = ["derive"] }
    sha3 = "0.10"
    hex = "0.4"
    k256 = { version = "0.13", features = ["ecdsa"] }

Step 2 — Create crates/hl-signing/tests/golden_vectors.rs:

    //! In-repo derived golden vectors. The oracle msgpacks canonical-ordered serde
    //! structs (serde serializes struct fields in DECLARATION order, never
    //! alphabetically), reproducing Hyperliquid's action-hash scheme independently.

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
             preserve_order) and Hyperliquid would reject every L1 signature"
        );
    }

Step 3 — Run: `cargo test -p hl-signing --test golden_vectors l1_action_hash_matches_canonical_oracle`
Expected: PASS (wallet-core 0.5.2 enables serde_json preserve_order, so production msgpack is canonical).
Optional sanity check that the test is meaningful: temporarily `cargo update -p motosan-wallet-core
--precise 0.5.0`, re-run → it should FAIL (alphabetical keys); then restore `--precise 0.5.2`.

Step 4 — Commit:
    git add crates/hl-signing/Cargo.toml crates/hl-signing/tests/golden_vectors.rs Cargo.lock
    git commit -m "test(signing): canonical L1 action-hash oracle + bump wallet-core 0.5.2 (R1 guard)"

## Context

This is the regression guard for R1 (alphabetical msgpack key ordering). wallet-core 0.5.2 already
fixed R1 via serde_json `preserve_order`, so this test passes now and locks the behavior. Do NOT add
the usdSend digest oracle here (that is Task 4, blocked on wallet-core 0.5.2). compute_action_hash is
a public fn in hl-signing: `compute_action_hash(action: &serde_json::Value, vault_address:
Option<&str>, nonce: u64) -> Result<[u8;32], HlError>`.
```

---

## Prompt 2 — L1 signature regression baseline (Task 3)

```
Implement Task 3: pin the full L1 signature (r/s/v) as a regression baseline.

## Task Description

File: Modify crates/hl-signing/tests/golden_vectors.rs (append to it).

Step 1 — Add a k256-backed test signer and a signing test that PRINTS r/s/v:

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
        println!("MAINNET r={} s={} v={}", sig.r, sig.s, sig.v);
    }

Step 2 — Run to capture the baseline:
    cargo test -p hl-signing --test golden_vectors l1_signature_baseline_mainnet -- --nocapture
Copy the printed r, s, v exactly.

Step 3 — Replace the println! with hard asserts using the captured values:
    assert_eq!(sig.r, "0x<captured-r>");
    assert_eq!(sig.s, "0x<captured-s>");
    assert_eq!(sig.v, 27); // or 28 — use whatever was printed

Step 4 — Run: `cargo test -p hl-signing --test golden_vectors l1_signature_baseline_mainnet`
Expected: PASS.

Step 5 — Commit:
    git add crates/hl-signing/tests/golden_vectors.rs
    git commit -m "test(signing): pin L1 signature r/s/v regression baseline"

## Context

The Signer trait method is `fn sign_hash(&self, address: &str, hash: &[u8;32]) -> Result<[u8;65],
HlError>`. hl_types::Signature has public fields r (hex String), s (hex String), v (u8). This pins the
full signature against future regressions (k256 uses deterministic RFC6979 nonces, so r/s/v are
stable for a fixed key+message). The expected hex in Step 3 is recorded from a real run, not invented.
```

---

## Prompt 3 — normalize_wire (Task 6)

```
Implement Task 6: a canonical wire-string formatter for Decimals.

## Task Description

Files: Modify crates/hl-types/src/util.rs and crates/hl-types/src/lib.rs.

Step 1 — Write failing tests in crates/hl-types/src/util.rs:

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
            // round_dp(8) is banker's rounding (MidpointNearestEven); 0.000000005 is
            // an exact midpoint that rounds to even => "0". Use NON-midpoint inputs.
            assert_eq!(normalize_wire(d("0.000000006")), "0.00000001");
            assert_eq!(normalize_wire(d("0.123456789")), "0.12345679");
            assert_eq!(normalize_wire(d("0.000000005")), "0");
        }

        #[test]
        fn integer_is_plain() {
            assert_eq!(normalize_wire(d("42")), "42");
            assert_eq!(normalize_wire(Decimal::ZERO), "0");
        }
    }

Step 2 — Run `cargo test -p hl-types wire_tests` → expect FAIL (cannot find function normalize_wire).

Step 3 — Implement in crates/hl-types/src/util.rs:

    use rust_decimal::Decimal;

    /// Format a [`Decimal`] into Hyperliquid canonical wire form.
    ///
    /// Mirrors the Python SDK's `float_to_wire`: at most 8 decimal places,
    /// trailing zeros stripped, plain decimal (never scientific notation).
    pub fn normalize_wire(value: Decimal) -> String {
        // round_dp uses banker's rounding (MidpointNearestEven), matching the
        // Python SDK's float_to_wire (`f"{x:.8f}"`). normalize() strips trailing
        // zeros and converts -0 to 0; Decimal's Display never uses sci-notation.
        value.round_dp(8).normalize().to_string()
    }

Step 4 — Re-export in crates/hl-types/src/lib.rs next to the other util re-exports (e.g. normalize_coin):
    pub use util::normalize_wire;

Step 5 — Run `cargo test -p hl-types wire_tests` → expect PASS.

Step 6 — Commit:
    git add crates/hl-types/src/util.rs crates/hl-types/src/lib.rs
    git commit -m "feat(types): add normalize_wire for canonical Hyperliquid float strings"

## Context

hl-types already depends on rust_decimal (locked at 1.41 — round_dp default is MidpointNearestEven).
util.rs already holds normalize_coin, re-exported from lib.rs; mirror that pattern. This is the
canonical wire formatter Task 7 will wire into the order builders.
```

---

## Prompt 4 — Builders emit canonical wire strings (Task 7)

```
Implement Task 7: order builders use normalize_wire instead of to_string().

## Task Description

File: Modify crates/hl-types/src/order.rs (limit_buy/limit_sell/trigger_buy/trigger_sell, ~lines 247-324).

Step 1 — Add a failing test in the order.rs test module:

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

Step 2 — Run `cargo test -p hl-types builder_uses_canonical_wire_strings` → expect FAIL
(currently "94500.000").

Step 3 — In limit_buy, replace `limit_px: limit_px.to_string()` with `limit_px: crate::normalize_wire(limit_px)`
and `sz: sz.to_string()` with `sz: crate::normalize_wire(sz)`. Apply the SAME change to limit_sell,
trigger_buy, and trigger_sell. In the trigger constructors also normalize the trigger price:
`let trigger_px_str = crate::normalize_wire(trigger_px);`

Step 4 — Run `cargo test -p hl-types` → expect PASS (the existing doctest expecting "90000" still holds).

Step 5 — Commit:
    git add crates/hl-types/src/order.rs
    git commit -m "fix(types): order builders emit canonical wire strings (R3)"

## Context

Depends on Task 6 (crate::normalize_wire must exist + be re-exported). build() re-parses the stored
string for positivity validation — all normalize_wire outputs parse fine via Decimal::from_str, so
build() is unaffected. Keep everything else in the builders unchanged.
```

---

## Prompt 5 — Price/size rounding for market orders (Task 8)

```
Implement Task 8: Hyperliquid price/size rounding helpers, wired into market orders.

## Task Description

File: Modify crates/hl-executor/src/executor/orders.rs.

Step 1 — Add failing tests in the orders.rs test module:

    #[test]
    fn round_price_perp_caps_5_sig_figs() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let p = round_price_perp(Decimal::from_str("66604.125").unwrap(), 5);
        assert_eq!(p, Decimal::from_str("66604").unwrap());
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

Step 2 — Run `cargo test -p hl-executor round_price_perp_caps_5_sig_figs` → expect FAIL
(cannot find function).

Step 3 — Add helpers near the top of orders.rs (after the imports):

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

Step 4 — Run the two tests → expect PASS.

Step 5 — In market_open (~orders.rs:280-299), after computing limit_price and before building the order,
resolve sz_decimals and round both price and size:

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

Step 6 — In market_close (~orders.rs:354-370) apply the same rounding to limit_price and close_size by
sz_decimals before building. (NOTE: the larger market_close rewrite for R5 is Task 8b — here only add
the rounding; leave the existing size/side logic as-is for now, OR coordinate with whoever runs 8b.)

Step 7 — Run `cargo test -p hl-executor` → expect PASS (existing slippage tests use round numbers).

Step 8 — Commit:
    git add crates/hl-executor/src/executor/orders.rs
    git commit -m "fix(executor): round market order price (5sf) and size (szDecimals) per Hyperliquid rules (R3)"

## Context

rust_decimal 1.41 has round_sf(u32)->Option<Decimal>, round_dp(u32), round_dp_with_strategy(dp, ToZero).
In market_open the local `coin = super::normalize_symbol(symbol)` is already in scope.
AssetMetaCache::sz_decimals(&str)->Option<u32> exists. Decimal == compares by value regardless of scale.
If Task 8b runs after this, it will rewrite market_close more fully; keep your Step 6 change minimal and
compatible.
```

---

## Prompt 6 — cloid idempotency (Task 9)

```
Implement Task 9: auto-attach a client order id so retried POSTs are deduplicated.

## Task Description

File: Modify crates/hl-executor/src/executor/orders.rs.

Step 1 — Add failing tests in the orders.rs test module:

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

Step 2 — Run `cargo test -p hl-executor new_cloid_is_hyperliquid_format` → expect FAIL.

Step 3 — Add helpers near the top of orders.rs:

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

Step 4 — In place_order, change the signature to take `mut order` and call ensure_cloid first:
    pub async fn place_order(&self, mut order: OrderWire, vault: Option<&str>) -> Result<OrderResponse, HlError> {
        ensure_cloid(&mut order);
        ... rest unchanged ...

Step 5 — In bulk_order, change `orders: Vec<OrderWire>` to `mut orders` and loop before building:
    for order in &mut orders {
        ensure_cloid(order);
    }

Step 6 — In place_trigger_order, replace `let cloid = uuid::Uuid::new_v4().to_string();` with
`let cloid = new_cloid();` (canonical 0x+32hex format).

Step 7 — Run `cargo test -p hl-executor` → expect PASS.

Step 8 — Commit:
    git add crates/hl-executor/src/executor/orders.rs
    git commit -m "fix(executor): auto-inject canonical cloid for retry idempotency (R4)"

## Context

OrderWire is #[non_exhaustive] but has a pub cloid field — assigning order.cloid on an OWNED instance
is allowed across crates (#[non_exhaustive] only blocks struct-literal construction + exhaustive
matching). uuid is already a dependency of hl-executor. `mut` in a parameter binding does not change
the public function signature. Depends on nothing else; can run independently.
```

---

## Prompt 7 — market_close magnitude (R5) + trigger rounding (Task 8b)

```
Implement Task 8b: market_close unsigned-magnitude semantics (R5) and trigger-order rounding (R3).

## Task Description

File: Modify crates/hl-executor/src/executor/orders.rs.

Step 1 — Add a failing test in the orders.rs test module:

    #[test]
    fn resolve_close_derives_side_from_position_not_size_sign() {
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let mag = Decimal::from_str("1.5").unwrap();
        let pos = Decimal::from_str("4").unwrap();
        assert_eq!(resolve_close(Some(mag), Side::Buy, pos).unwrap(), (Side::Sell, mag));
        assert_eq!(resolve_close(Some(mag), Side::Sell, pos).unwrap(), (Side::Buy, mag));
        assert_eq!(resolve_close(None, Side::Buy, pos).unwrap(), (Side::Sell, pos));
        assert!(resolve_close(Some(Decimal::from_str("-1").unwrap()), Side::Buy, pos).is_err());
        assert!(resolve_close(Some(Decimal::ZERO), Side::Buy, pos).is_err());
    }

Step 2 — Run `cargo test -p hl-executor resolve_close_derives_side_from_position_not_size_sign`
→ expect FAIL.

Step 3 — Add resolve_close near the top of orders.rs:

    /// Derive the close side and size for `market_close`. The side is ALWAYS taken
    /// from the live position sign (long -> Sell, short -> Buy). `size` is an
    /// unsigned magnitude: Some(m) closes m (must be > 0), None closes the full
    /// position. Direction is never encoded in the sign of `size`.
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

(If the resolve_close test needs Side to derive PartialEq/Debug and it doesn't already, add those
derives to the Side enum in hl-types/src/order.rs — check first; most likely they're already derived.)

Step 4 — Run the test → expect PASS.

Step 5 — Rewrite market_close (~orders.rs:314-373): replace its body from `let coin = ...` through the
`self.place_order(...)` call with — always query the position, use resolve_close, then round:

    let coin = super::normalize_symbol(symbol);

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

Also update the market_close doc comment to state `size` is an unsigned magnitude and the side is
derived from the live position. (Behavior change: size=Some now also reads clearinghouseState and
errors if there is no open position.)

Step 6 — Round + normalize place_trigger_order (it builds its action inline with raw to_string()).
After resolve_asset, resolve sz_decimals, round, and emit canonical wire strings (Decimal is Copy so
trigger_price may be passed to normalize_wire twice):

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

Step 7 — Run `cargo test -p hl-executor` → expect PASS.

Step 8 — Commit:
    git add crates/hl-executor/src/executor/orders.rs
    git commit -m "fix(executor): market_close unsigned magnitude + position-derived side (R5); round trigger orders (R3)"

## Context

Depends on Task 8 (round_price_perp/round_size), Task 6 (hl_types::normalize_wire), and Task 9
(new_cloid). extract_position_szi(&Value, &str) -> Result<(Side, Decimal)> and
extract_mid_price(&client, &coin) already exist in orders.rs and are used by the current code. Direct
LIMIT orders via place_order intentionally get only normalize_wire (not grid-rounding) — that is the
caller's responsibility; note it in the place_order rustdoc.
```

---

## Prompt 8 — usdSend digest-recovery oracle (Task 4) — run after Prompt 2 (Task 3)

```
Implement Task 4: a user-signed (usdSend) EIP-712 digest-recovery oracle that anchors R2.

## Task Description

File: Modify crates/hl-signing/tests/golden_vectors.rs (append; reuses K256Signer/TEST_KEY/Keccak256
from Task 3 — run Prompt 2 first).

Step 1 — Append this oracle + recovery test. It signs a usdSend action via the production
sign_user_signed_action, independently recomputes the canonical EIP-712 digest (domain chainId 421614,
verifyingContract 0x0), ecrecovers, and asserts the recovered address == the signer. On wallet-core
0.5.2 it PASSES (R2 fixed); it would FAIL on 0.5.1.

    use hl_signing::{sign_user_signed_action, EIP712Field};
    use k256::ecdsa::{RecoveryId, Signature as K256Sig, VerifyingKey};

    fn keccak(bytes: &[u8]) -> [u8; 32] {
        let mut h = Keccak256::new();
        h.update(bytes);
        h.finalize().into()
    }

    fn oracle_usd_send_digest(destination: &str, amount: &str, time: u64, is_mainnet: bool) -> [u8; 32] {
        let hl_chain = if is_mainnet { "Mainnet" } else { "Testnet" };
        let domain_type = keccak(
            b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
        );
        let mut chain_id = [0u8; 32];
        chain_id[24..].copy_from_slice(&421614u64.to_be_bytes());
        let verifying_contract = [0u8; 32];
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
            recovered.to_lowercase(), addr.to_lowercase(),
            "usdSend signature must recover the signer under the canonical domain (R2)"
        );
    }

Step 2 — Run: cargo test -p hl-signing --test golden_vectors usd_send_recovers_signer_under_canonical_domain
Expected: PASS. Optional: cargo update -p motosan-wallet-core --precise 0.5.1 → it FAILS (proves the R2
guard) → restore --precise 0.5.2.

Step 3 — Commit:
    git add crates/hl-signing/tests/golden_vectors.rs
    git commit -m "test(signing): usdSend digest-recovery oracle anchors R2 (wallet-core 0.5.2)"

## Context

Reuses K256Signer/TEST_KEY/Keccak256 from Task 3 — run Prompt 2 first. API:
sign_user_signed_action(signer, address, action, types: &[EIP712Field], primary_type, is_mainnet)
-> Result<Signature, HlError>; EIP712Field::new(name, solidity_type); Signature has pub r/s/v.
```

## Prompt 9 — signatureChainId wire literal → 0x66eee (Task 5) — runnable any time

```
Implement Task 5: set the user-signed signatureChainId wire literal to canonical "0x66eee".

## Task Description

Files: crates/hl-executor/src/executor/transfer.rs (usdc_transfer) and
crates/hl-executor/src/executor/admin.rs (approve_agent).

Why a value change, not a deletion: motosan-wallet-core injects signatureChainId only into a local
clone used for the EIP-712 hash; it never mutates the caller's action. The caller's action is what is
POSTed, and the exchange reconstructs the domain from the posted signatureChainId. wallet-core 0.5.2
signs the user-signed domain at chainId 421614, so the posted value must be the hex string "0x66eee"
(= 421614) for the exchange's reconstruction to match the recovered signer on BOTH networks. The
current literal is "0xa4b1" (42161) — change its value.

Step 1 — In transfer.rs usdc_transfer, change the signatureChainId literal to:
    "signatureChainId": "0x66eee",

Step 2 — In admin.rs approve_agent, change the same "0xa4b1" literal to "0x66eee".

Step 3 — Add an assertion test in the executor test module:
    #[test]
    fn usd_send_action_carries_canonical_signature_chain_id() {
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

Step 4 — Run: cargo test -p hl-executor → expect PASS.

Step 5 — Commit:
    git add crates/hl-executor/src/executor/transfer.rs crates/hl-executor/src/executor/admin.rs
    git commit -m "fix(executor): user-signed signatureChainId wire value -> canonical 0x66eee (R2)"

## Context

Pairs with wallet-core 0.5.2 (user-signed domain chainId 421614). Verify the current literal in both
files first (grep "0xa4b1"). Do NOT delete the field — only change its value.
```

---

## Reusable — Spec Compliance Reviewer (run after each implementer reports DONE)

```
You are reviewing whether an implementation matches its specification. Do NOT trust the implementer's
report — read the actual code and verify independently.

## What was requested
[Paste the same Task Description you gave the implementer.]

## What the implementer claims
[Paste the implementer's report.]

## Your job
Read the changed code (git show <commit>, and the files) and verify:
- Missing requirements: did they implement every step? any skipped?
- Extra/unneeded work: anything built that wasn't requested? over-engineering?
- Misunderstandings: right feature, wrong way? wrong values (e.g. test expectations, field order)?
Confirm the exact test command in the task was run and gives the stated result.

Report:
- ✅ Spec compliant (everything matches after code inspection), or
- ❌ Issues: [specific, with file:line]
```

## Reusable — Code Quality Reviewer (run only after spec review is ✅)

```
You are doing a code-quality review of one task's changes.

DESCRIPTION: [task summary from the implementer's report]
REQUIREMENTS: [the Task Description]
DIFF: git diff <base_sha>..<head_sha>   (base = commit before the task; head = the task's commit)

Check: correctness, clear naming, tests verify real behavior (not mocks), no dead code, follows the
surrounding style, one clear responsibility per file, no unrequested scope creep. For this codebase
specifically: coin symbols normalized; HlError used (no crate-local errors); wire field order
unchanged; #[non_exhaustive] respected.

Return: Strengths; Issues (Critical / Important / Minor, each with file:line + fix); Assessment
(Approved / Changes needed).
```
