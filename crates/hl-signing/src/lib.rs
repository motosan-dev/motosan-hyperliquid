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

// TODO: upgrade to #![warn(missing_docs)] once public API is fully documented
#![allow(missing_docs)]

pub mod adapter;
pub mod eip712;
#[cfg(feature = "k256-signer")]
pub mod private_key;
pub mod signer;

pub use adapter::SingleAddressSigner;
pub use eip712::{compute_action_hash, sign_l1_action, sign_user_signed_action, EIP712Field};
#[cfg(feature = "k256-signer")]
pub use private_key::PrivateKeySigner;
pub use signer::Signer;
