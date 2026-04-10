# EIP-712 Signing

## PrivateKeySigner

```rust
use hl_signing::PrivateKeySigner;

let signer = PrivateKeySigner::from_hex("0xYourPrivateKey")?;
let address = signer.address(); // H160 Ethereum address
```

## Signer Trait

Custom signing backends implement the `Signer` trait:

```rust
use hl_signing::Signer;
use hl_types::Signature;

#[async_trait]
pub trait Signer: Send + Sync {
    fn address(&self) -> ethers_core::types::H160;
    async fn sign_typed_data(&self, data: &TypedData) -> Result<Signature, HlError>;
}
```

## Action Signing

Sign L1 actions (orders, cancels, transfers):

```rust
use hl_signing::sign_l1_action;

let signature = sign_l1_action(&signer, action_hash, is_mainnet).await?;
```

Sign user-signed actions (agent approvals):

```rust
use hl_signing::sign_user_signed_action;

let signature = sign_user_signed_action(&signer, action_data).await?;
```

## Signature Type

```rust
use hl_types::Signature;

// Signature { r: String, s: String, v: u8 }
// r, s are hex-encoded strings
```
