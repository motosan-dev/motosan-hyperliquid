# motosan-hyperliquid — Milestone Roadmap

**Source:** the 43-finding audit (2026-06-02), deduped to 31 work items across 6 tiers.

**How to read this:** This is the sequencing layer over the audit. **M0 already has a detailed,
TDD, bite-sized implementation plan** (`2026-06-02-m0-signing-wire-correctness.md`). Per the
writing-plans scope rule (one plan per subsystem), **M1–M5 each get their own detailed
implementation plan when reached** — this document defines their scope, dependencies, and exit
criteria so they can be planned and scheduled.

**Severity legend:** 🔴 critical · 🟠 high · 🟡 medium · ⚪ low.

---

## Milestone Overview

| # | Milestone | Theme | Findings | Top severity | Rough effort | Depends on |
|---|-----------|-------|----------|--------------|--------------|------------|
| **M0** | Signing & Wire Correctness | Make signed actions valid | R1, R2, R3, R4, R5, T1 | 🔴 | ~1 sprint | wallet-core 0.5.1 |
| **M1** | Funds Egress & Core Trading Parity | Withdraw, builder codes, brackets | F1, F2, F3, F4, F5 | 🟠 | ~1 sprint | M0 |
| **M2** | Reads & Subscriptions & Accounts | Info parity, WS, sub-accounts, TWAP | F6, F7, F8, F9 | 🟡 | ~1 sprint | M0 |
| **M3** | Robustness | WS liveness, rate limiting, retries | B1, B2, B3, B4, B5, R4⁺ | 🟡 | ~1 sprint | M0 |
| **M4** | Ergonomics & Safety | Vault-once, symbol API, Cloid type | E1, E2, E3, E4, E5, E6 | 🟡 | ~1 sprint | M0, M1 |
| **M5** | Perf & Test/Doc Hardening | Alloc cleanup, coverage, niche actions | P1–P7, T3, T4, T5, F10–F13 | ⚪ | ~0.5 sprint | M0 |

**Critical path:** `wallet-core 0.5.1 → M0 → (M1 ∥ M2 ∥ M3) → M4 → M5`.
M1/M2/M3 are independent of each other once M0 lands and can run in parallel.

---

## M0 — Signing & Wire Correctness 🔴

> **Detailed plan:** `2026-06-02-m0-signing-wire-correctness.md` (ready to execute).

**Why first:** Until M0 lands, the SDK cannot place a single valid order or move funds on mainnet.
Everything downstream that signs depends on it.

**Scope:**
- 🔴 **R1** Canonical msgpack key order (fix in wallet-core 0.5.1, bump here).
- 🔴 **R2** User-signed EIP-712 domain (chainId 421614 / `0x66eee` / `verifyingContract`).
- 🔴 **R3** Wire normalization + 5-sig-fig price + szDecimals size rounding.
- 🟠 **R4** Auto-cloid idempotency for retried writes + fix `place_trigger_order` cloid format.
- 🟡 **R5** `market_close(size=Some)` → unsigned magnitude, derive side from position (fold into the market_close task).
- 🟠 **T1** Self-contained golden-vector + digest-recovery tests.

**Exit criteria:**
- `cargo test --all-features` green incl. `golden_vectors`.
- A `market_open` testnet order is **accepted** (manual smoke with `HYPERLIQUID_TESTNET_KEY`).
- `cargo clippy --all-features --all-targets -- -D warnings` clean.

---

## M1 — Funds Egress & Core Trading Parity 🟠

**Why now:** Once signing works, the highest-value gaps are the ability to get capital **out**, to
integrate as a builder, and to place atomic risk-managed brackets — the things a serious bot needs
that the SDK simply cannot do today.

**Scope:**
- 🟠 **F1** `withdraw(destination, amount)` → user-signed `withdraw3` (`HyperliquidTransaction:Withdraw`). Completes deposit→trade→**withdraw**.
- 🟡 **F2** `transfer_to_vault` gains `is_deposit` (or add `withdraw_from_vault`) — pull funds back out of a vault.
- 🟠 **F3** `approve_builder_fee(builder, max_fee_rate)` + thread optional `builder:{b,f}` through `place_order`/`bulk_order` into the order action.
- 🟡 **F4** `grouping` enum (`Na`/`NormalTpsl`/`PositionTpsl`) on `bulk_order` — atomic entry+TP+SL OCO bracket.
- 🟡 **F5** `spot_transfer(destination, token, amount)` → user-signed `spotSend`.

**Implementation notes:** F1/F5 reuse the now-correct `sign_user_signed_action` path (mirror
`usdc_transfer`). F2/F3-action/F4 are L1 actions via `send_signed_action`. Each new action needs a
golden-vector test (extend `golden_vectors.rs`).

**Exit criteria:** Each new action has a digest-recovery / action-hash test; a testnet `withdraw3`
and a grouped TP/SL bracket are accepted.

---

## M2 — Reads, Subscriptions & Accounts 🟡

**Why now:** Broadens what strategies the SDK can support — spot awareness, real-time per-asset
context, multi-account, TWAP — mostly low-risk additive reads plus a few actions.

**Scope:**
- 🟡 **F8** Account reads: `spot_user_state`, `user_fees`, `frontend_open_orders`, `user_fills_by_time`, `query_order_by_cloid`, `query_referral_state`, `user_rate_limit` (pure `post_info`).
- 🟡 **F9** WS `Subscription::ActiveAssetCtx { coin }` and `ActiveAssetData { coin, user }` (no lowercasing of coin/user on the wire).
- 🟡 **F7** Sub-accounts: `create_sub_account`, `sub_account_transfer`, `sub_account_spot_transfer`, `Account::query_sub_accounts`.
- 🟡 **F6** TWAP: `twap_order`, `twap_cancel`, `Account::twap_slice_fills`.

**Exit criteria:** Each read returns a typed/`Value` result with a unit test against a recorded
sample payload; new WS variants round-trip subscribe/parse against a recorded frame.

---

## M3 — Robustness 🟡

**Why now:** Independent of feature work; makes long-running bots survive flaky links and bursts.
Can run in parallel with M1/M2.

**Scope:**
- 🟡 **B1** WS pong-liveness: track last-pong/last-frame; drop+reconnect after ~2× heartbeat.
- 🟡 **B2** Background WS driver (`connect_and_run` over mpsc) + `unsubscribe` + subscription dedup (large).
- 🟡 **B3** Weight-aware rate limiter: per-request token cost (`bulk_order` charges `orders.len()`); document the bucket as a coarse net.
- ⚪ **B4** HTTP retry jitter (mirror the WS path); make `rand` available on the HTTP path.
- ⚪ **B5** Remove the unreachable `order_to_json` error arm (route inner `t` through `serde_json::to_value`, or drop `#[non_exhaustive]` on the closed enum).
- 🟠 **R4⁺** (follow-up to M0) Split retry policy: don't retry `post_action` on ambiguous Http/Timeout by default; rely on cloid dedup or explicit opt-in.

**Exit criteria:** A simulated half-open WS triggers reconnect within the deadline; a `bulk_order`
burst respects the weighted budget; retry-storm test shows jittered delays.

---

## M4 — Ergonomics & Safety 🟡

**Why now:** Depends on M0 (and M1 for the new actions to wrap). Reduces footguns and makes the
public API pleasant and hard to misuse.

**Scope:**
- 🟡 **E1** `OrderExecutor` stores `vault_address` (`with_vault`/`set_vault`); `send_signed_action` defaults to it (keep per-call override; preserve `transfer_to_vault`/direct-`post_action` semantics).
- 🟡 **E2** Symbol-first `limit_order(symbol, side, px, sz, tif)` resolving the index internally.
- 🟡 **E3** `cancel_order_by_symbol`, `cancel_all`/`cancel_all_for_symbol`, `close_position(symbol)`.
- 🟡 **E4** `Cloid` newtype (validates `0x`+32-hex; (de)serializes to hex; threaded through builders/cancel paths).
- ⚪ **E5** `FromStr` + `From<bool>`/`Into<bool>` for Side/Tif/Tpsl/OrderStatus; `Default = Gtc` on Tif.
- ⚪ **E6** Trigger-safe builder: `.tif()` unavailable on trigger builders at compile time (type-state) or a `debug_assert!`.

**Exit criteria:** Vault set once is honored on every signed write (test); symbol-first order +
cancel + close paths covered by unit tests; `Cloid::try_from` rejects malformed ids.

---

## M5 — Performance & Test/Doc Hardening ⚪

**Why last:** Lowest impact (signing + network dominate latency); best done once the surface is
stable so cleanup doesn't churn.

**Scope:**
- 🟡 **P1** `market_close(size=None)` → `tokio::try_join!` the two reads; resolve asset index once; `eq_ignore_ascii_case` in `extract_position_szi`. (Only user-visible perf win.)
- ⚪ **P2–P7** Allocation cleanup pass: store parsed Decimals on `OrderWire` (+ remove the `unwrap_or(Decimal::ZERO)` fallbacks); fractional-refill `TokenBucket`; move-not-clone in `WsMessage::parse`; `mem::take` WS subscription set; move-not-clone Account array reads; precompute endpoint URLs; expose `_normalized` cache lookups.
- 🟡 **T3** Unit-test `order_to_json` (single-letter-key output) and `determine_status` boundaries.
- 🟡 **T4** Unit-test `next_nonce` strict monotonicity + multi-thread no-duplicates.
- 🟡 **T5** Fix the live-test docs/gating mismatch (`--features live-test` vs `#[ignore]`).
- ⚪ **F13** Expose `expires_after` end-to-end (or document unsupported) + hash/body consistency test.
- 🟡 **F10 / ⚪ F11 / ⚪ F12** Niche actions: multi-sig (large), cross-DEX `sendAsset`, `set_referrer`.

**Exit criteria:** `market_close` latency roughly halved (parallel reads); `order_to_json`/`next_nonce`
covered; live-test command actually runs the live suite.

---

## Sequencing & Parallelization

```
wallet-core 0.5.1
        │
        ▼
      ┌────┐
      │ M0 │   ← blocks everything that signs
      └─┬──┘
        │
   ┌────┼────┬──────────────┐
   ▼    ▼    ▼              │
 ┌──┐ ┌──┐ ┌──┐            │  (M1/M2/M3 independent — run in parallel)
 │M1│ │M2│ │M3│            │
 └─┬┘ └──┘ └──┘            │
   │                        │
   ▼                        │
 ┌──┐  ◄── needs M0 + M1's new actions to wrap
 │M4│
 └─┬┘
   ▼
 ┌──┐
 │M5│  ← stabilize, then optimize/cover
 └──┘
```

**Recommended order for a small team:** finish M0 (with wallet-core 0.5.1) → start M1 while M3 runs
in parallel → M2 → M4 → M5. Each milestone, when picked up, gets its own bite-sized implementation
plan authored the same way as M0.
