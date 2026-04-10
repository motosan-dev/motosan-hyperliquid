//! Convenience re-exports of the most commonly used types.
//!
//! ```rust,ignore
//! use motosan_hyperliquid::prelude::*;
//! ```

// -- Types (always available) ------------------------------------------------

pub use hl_types::{
    Decimal, HlAccountState, HlActionResponse, HlAssetInfo, HlCandle, HlError, HlFill,
    HlFundingRate, HlOrderbook, HlPosition, OrderWire, OrderWireBuilder, Side, Signature,
    CancelByCloidRequest, CancelRequest, ModifyRequest,
    OrderStatus, PositionSide, Tif, Tpsl, normalize_coin,
};

// -- Client (always available) -----------------------------------------------

pub use hl_client::{HttpTransport, HyperliquidClient, RetryConfig, TimeoutConfig};

// -- Market ------------------------------------------------------------------

#[cfg(feature = "market")]
pub use hl_market::MarketData;

// -- Account -----------------------------------------------------------------

#[cfg(feature = "account")]
pub use hl_account::Account;

// -- Executor ----------------------------------------------------------------

#[cfg(feature = "executor")]
pub use hl_executor::{AssetMetaCache, OrderExecutor};

// -- Signing -----------------------------------------------------------------

#[cfg(feature = "signing")]
pub use hl_signing::{sign_l1_action, sign_user_signed_action, PrivateKeySigner, Signer};

// -- WebSocket ---------------------------------------------------------------

#[cfg(feature = "ws")]
pub use hl_client::{HyperliquidWs, Subscription, WsConfig, WsMessage};
