# Release Process

Rust workspace — all 6 crates share one version and are released together.

## Tag Convention

| Tag format | Example | Registry | Workflow |
|------------|---------|----------|----------|
| `rust-vX.Y.Z` | `rust-v0.1.0` | crates.io | `publish-rust.yml` |

## Release Checklist

### 1. Version Bump

File: `Cargo.toml` (workspace root) → `version = "X.Y.Z"`

All crates inherit from `workspace.package.version`, so one change bumps everything.

### 2. Update CHANGELOG

`CHANGELOG.md` — Keep a Changelog format:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- ...

### Changed
- ...

### Fixed
- ...
```

### 3. Update Version References

- `README.md` — install section version numbers
- `llms.txt` — header version line, Install section
- `skills/motosan-hyperliquid/SKILL.md` — header version, Install section

### 4. Commit

```bash
git add Cargo.toml CHANGELOG.md README.md llms.txt skills/motosan-hyperliquid/SKILL.md
git commit -m "chore: release rust-vX.Y.Z"
```

### 5. Tag + Push

```bash
git tag -a rust-vX.Y.Z -m "rust-vX.Y.Z — summary of changes"
git push origin main rust-vX.Y.Z
```

Tag push triggers `publish-rust.yml` → crates.io.

## CI Publish Pipeline

### publish-rust.yml

Trigger: `push tags: ["rust-v*"]` OR `workflow_dispatch`

```
Steps:
1. Checkout
2. Setup stable Rust (with rustfmt, clippy)
3. cargo fmt --all -- --check
4. cargo clippy --all-features --all-targets -- -D warnings
5. cargo test --all-features
6. Publish crates in dependency order (see below)
```

### Publish Order

Crates must be published in dependency order (crates.io requires deps to exist first):

1. `hl-types`
2. `hl-signing`
3. `hl-client`
4. `hl-market`
5. `hl-account`
6. `hl-executor`

## Pre-Push Local Validation

```bash
cargo fmt --all -- --check
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
```

## Emergency Manual Publish

```bash
cargo publish -p hl-types && \
cargo publish -p hl-signing && \
cargo publish -p hl-client && \
cargo publish -p hl-market && \
cargo publish -p hl-account && \
cargo publish -p hl-executor
```

## GitHub Secrets

| Secret | Used by | Purpose |
|--------|---------|---------|
| `CARGO_REGISTRY_TOKEN` | publish-rust.yml | Authenticate to crates.io |

## CI Workflows (non-release)

| Workflow | Trigger | Steps |
|----------|---------|-------|
| `ci-rust.yml` | Push/PR to `crates/**` | `fmt` → `clippy` → `test --all-features` |
