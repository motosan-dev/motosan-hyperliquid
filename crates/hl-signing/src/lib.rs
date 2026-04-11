//! # hl-signing
//!
//! EIP-712 signing for Hyperliquid L1 actions.
//!
//! Provides the [`Signer`] trait for abstracting key management, and functions
//! to sign L1 actions ([`sign_l1_action`]) and user-signed actions
//! ([`sign_user_signed_action`]).
//!
//! ## Feature flags
//!
//! | Flag | Default | Description |
//! |------|---------|-------------|
//! | `k256-signer` | **yes** | Enables [`PrivateKeySigner`] backed by the `k256` crate. Disable this if you bring your own [`Signer`] (HSM, AWS KMS, etc.) to avoid the extra compile-time cost. |

#![warn(missing_docs)]

/// Adapter bridging [`Signer`] to motosan-wallet-core's `HlSigner`.
pub mod adapter;
/// EIP-712 signing functions and field descriptors.
pub mod eip712;
#[cfg(feature = "k256-signer")]
/// `k256`-backed private key signer implementation.
pub mod private_key;
/// The [`Signer`] trait for abstracting key management.
pub mod signer;

pub use adapter::SingleAddressSigner;
pub use eip712::{compute_action_hash, sign_l1_action, sign_user_signed_action, EIP712Field};
#[cfg(feature = "k256-signer")]
pub use private_key::PrivateKeySigner;
pub use signer::Signer;
