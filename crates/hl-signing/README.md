# hl-signing

> EIP-712 signing for Hyperliquid — `Signer` trait, `PrivateKeySigner`, L1 action signing, user-signed actions, and optional L1 action expiry.

## Overview

Hyperliquid uses EIP-712 typed-data signatures for exchange actions. This crate provides:

1. `Signer` — abstraction over key-management backends.
2. `PrivateKeySigner` — built-in `k256` private-key signer (default `k256-signer` feature).
3. `sign_l1_action` — sign orders/cancels/leverage/vault transfers.
4. `sign_l1_action_with_expiry` — sign L1 actions with optional `expiresAfter` replay protection.
5. `sign_user_signed_action` — sign user actions such as `usdSend`, `withdraw3`, `spotSend`, `sendAsset`, agent/builder approvals, and sub-account actions.
6. `compute_action_hash` — msgpack + nonce + vault action hash helper.

The crate uses `motosan-wallet-core` 0.5.3 for Hyperliquid-compatible signing.

## Sign an L1 Action

```rust
use hl_signing::{sign_l1_action, PrivateKeySigner, Signer};

let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
let address = signer.address().to_string();

let action = serde_json::json!({
    "type": "order",
    "orders": [{"a": 0, "b": true, "p": "90000", "s": "0.001", "r": false, "t": {"limit": {"tif": "Gtc"}}}],
    "grouping": "na"
});

let signature = sign_l1_action(
    &signer,
    &address,
    &action,
    1234567890, // nonce
    true,       // is_mainnet
    None,       // vault_address
)?;

println!("r={}, s={}, v={}", signature.r, signature.s, signature.v);
```

## Sign an L1 Action with Expiry

```rust
use hl_signing::sign_l1_action_with_expiry;

let expires_after = Some(1_717_000_000_000_u64); // unix epoch milliseconds
let signature = sign_l1_action_with_expiry(
    &signer,
    &address,
    &action,
    nonce,
    true,
    None,
    expires_after,
)?;
```

`expiresAfter` is included in the L1 action hash when present. `hl-executor` exposes this through `OrderExecutor::set_expires_after` and sends the same timestamp in the `/exchange` body.

## Sign a User-Signed Action

```rust
use hl_signing::{sign_user_signed_action, EIP712Field, PrivateKeySigner};

let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
let address = signer.address().to_string();

let action = serde_json::json!({
    "type": "approveAgent",
    "hyperliquidChain": "Mainnet",
    "signatureChainId": "0x66eee",
    "agentAddress": "0x1111111111111111111111111111111111111111",
    "agentName": "my-bot",
    "nonce": 1000
});

let types = vec![
    EIP712Field::new("hyperliquidChain", "string"),
    EIP712Field::new("agentAddress", "address"),
    EIP712Field::new("agentName", "string"),
    EIP712Field::new("nonce", "uint64"),
];

let sig = sign_user_signed_action(
    &signer,
    &address,
    &action,
    &types,
    "HyperliquidTransaction:ApproveAgent",
    true,
)?;
```

User-signed actions use the canonical Hyperliquid EIP-712 domain with chain ID `421614` (`0x66eee`) for both mainnet and testnet, and the posted `signatureChainId` should be `0x66eee`.

## Implement a Custom Signer

```rust
use hl_signing::Signer;
use hl_types::HlError;

struct MyHardwareWalletSigner;

impl Signer for MyHardwareWalletSigner {
    fn sign_hash(&self, address: &str, hash: &[u8; 32]) -> Result<[u8; 65], HlError> {
        // Return r (32) + s (32) + recovery_id (1, value 0 or 1).
        todo!("delegate to hardware wallet")
    }
}
```

## EIP-712 Details

L1 actions (orders, cancels, vault transfers):

- Domain: `{ name: "Exchange", version: "1", chainId: 1337 }`
- Primary type: `Agent`
- Source: `0xa` (mainnet) or `0xb` (testnet)
- Optional expiry: `expiresAfter` is folded into the action hash when provided.

User-signed actions (`usdSend`, `withdraw3`, `spotSend`, `sendAsset`, approvals, sub-account actions):

- Domain: `{ name: "HyperliquidSignTransaction", version: "1", chainId: 421614 }`
- Posted `signatureChainId`: `0x66eee`
- Custom primary type and fields per action.

## License

MIT
