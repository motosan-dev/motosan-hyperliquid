# hl-signing

> EIP-712 signing for Hyperliquid L1 actions -- `Signer` trait, `PrivateKeySigner`, action hashing.

## Overview

Hyperliquid uses EIP-712 typed data signatures for all exchange actions. This crate provides:

1. A **`Signer` trait** that abstracts over key management backends (raw private key, HD wallet, hardware wallet, etc.)
2. A built-in **`PrivateKeySigner`** for direct private key signing
3. Functions to **sign L1 actions** (`sign_l1_action`) and **user-signed actions** (`sign_user_signed_action`)
4. An **action hash computation** function (`compute_action_hash`) for the msgpack + nonce + vault scheme

## Usage

### Sign an L1 Action (Order, Cancel, Transfer)

```rust
use hl_signing::{PrivateKeySigner, Signer, sign_l1_action};

let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
let address = signer.address().to_string();

let action = serde_json::json!({
    "type": "order",
    "orders": [{"a": 0, "b": true, "p": "90000", "s": "0.001"}],
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

### Sign a User-Signed Action (Agent Approval)

```rust
use hl_signing::{PrivateKeySigner, sign_user_signed_action, EIP712Field};

let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
let address = signer.address().to_string();

let action = serde_json::json!({
    "hyperliquidChain": "Mainnet",
    "agentAddress": "0xAgentAddress",
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

### Implement a Custom Signer

```rust
use hl_signing::Signer;
use hl_types::HlError;

struct MyHardwareWalletSigner { /* ... */ }

impl Signer for MyHardwareWalletSigner {
    fn sign_hash(&self, address: &str, hash: &[u8; 32]) -> Result<[u8; 65], HlError> {
        // Return 65 bytes: r (32) + s (32) + recovery_id (1, value 0 or 1)
        todo!("delegate to hardware wallet")
    }
}
```

## EIP-712 Details

**L1 actions** (orders, cancels, vault transfers) use:
- Domain: `{ name: "Exchange", version: "1", chainId: 1337 }`
- Primary type: `Agent`
- Source: `0xa` (mainnet) or `0xb` (testnet)

**User-signed actions** (agent approval) use:
- Domain: `{ name: "HyperliquidSignTransaction", version: "1", chainId: 421614 }`
- Custom primary type and fields per action

## License

MIT
