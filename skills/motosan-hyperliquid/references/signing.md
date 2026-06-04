# EIP-712 Signing

## PrivateKeySigner

```rust
use hl_signing::{PrivateKeySigner, Signer};

let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
let address = signer.address().to_string();
```

## Signer Trait

Custom key-management backends implement `Signer`:

```rust
use hl_signing::Signer;
use hl_types::HlError;

struct MySigner;

impl Signer for MySigner {
    fn sign_hash(&self, address: &str, hash: &[u8; 32]) -> Result<[u8; 65], HlError> {
        // Return r (32) + s (32) + recovery_id (1).
        todo!("delegate to HSM / KMS / hardware wallet")
    }
}
```

## L1 Action Signing

```rust
use hl_signing::sign_l1_action;

let signature = sign_l1_action(
    &signer,
    &address,
    &action,
    nonce,
    true, // is_mainnet
    None, // vault_address
)?;
```

## L1 Action Signing with Expiry

Optional `expiresAfter` (unix epoch milliseconds) is supported through `motosan-wallet-core` 0.5.3:

```rust
use hl_signing::sign_l1_action_with_expiry;

let signature = sign_l1_action_with_expiry(
    &signer,
    &address,
    &action,
    nonce,
    true,
    None,
    Some(expires_after_ms),
)?;
```

The same `expiresAfter` value must be sent in the `/exchange` body. `hl-executor` handles this automatically with `OrderExecutor::set_expires_after`.

## User-Signed Actions

```rust
use hl_signing::{sign_user_signed_action, EIP712Field};

let action = serde_json::json!({
    "type": "approveAgent",
    "hyperliquidChain": "Mainnet",
    "signatureChainId": "0x66eee",
    "agentAddress": "0x1111111111111111111111111111111111111111",
    "agentName": "my-bot",
    "nonce": nonce,
});

let types = vec![
    EIP712Field::new("hyperliquidChain", "string"),
    EIP712Field::new("agentAddress", "address"),
    EIP712Field::new("agentName", "string"),
    EIP712Field::new("nonce", "uint64"),
];

let signature = sign_user_signed_action(
    &signer,
    &address,
    &action,
    &types,
    "HyperliquidTransaction:ApproveAgent",
    true,
)?;
```

User-signed actions (`usdSend`, `withdraw3`, `spotSend`, `sendAsset`, agent/builder/sub-account) use Hyperliquid's canonical EIP-712 domain with chain ID `421614` (`0x66eee`) for both mainnet and testnet.

## Signature Type

```rust
use hl_types::Signature;

// Signature { r: String, s: String, v: u8 }
```
