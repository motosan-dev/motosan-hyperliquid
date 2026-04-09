//! # hl-signing
//!
//! EIP-712 signing for Hyperliquid L1 actions.
//!
//! Provides the [`Signer`] trait for abstracting key management, a built-in
//! [`PrivateKeySigner`] for direct private key signing, and functions to sign
//! L1 actions ([`sign_l1_action`]) and user-signed actions
//! ([`sign_user_signed_action`]).

pub mod adapter;
pub mod eip712;
pub mod private_key;
pub mod signer;

pub use adapter::SingleAddressSigner;
pub use eip712::{compute_action_hash, sign_l1_action, sign_user_signed_action, EIP712Field};
pub use private_key::PrivateKeySigner;
pub use signer::Signer;
