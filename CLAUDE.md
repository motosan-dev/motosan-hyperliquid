# CLAUDE.md

## Commands

```bash
cargo fmt --all                                      # Format
cargo clippy --all-features --all-targets -- -D warnings  # Lint
cargo test --all-features                            # Test (unit only)
cargo test --all-features -- --ignored               # Live integration tests (needs testnet)
```

## Rules That Prevent Mistakes

This is a Cargo workspace with 6 crates. All crates inherit `version`, `edition`, `license` from the workspace root `Cargo.toml`.

Dependency graph is strict and layered — `hl-types` at the bottom, `hl-executor` at the top. Never introduce circular deps.

All crates use `hl_types::HlError` as the unified error type. Don't create crate-local error types.

Coin symbols must be normalized before use (`"BTC-PERP"` → `"BTC"`). Use `normalize_coin()` from `hl-types`.

## Release Checklist

See `@llms.txt` § Release for the full process. Files to update: `Cargo.toml` (workspace version), `CHANGELOG.md`, `README.md`, `llms.txt`, `skills/motosan-hyperliquid/SKILL.md`.
