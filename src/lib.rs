//! Server-side Lunixi merchant APIs and verified webhooks.
//!
//! Share one [`Client`] per merchant, use integer minor units, and persist
//! payment attempt idempotency keys before calling financial mutations.
//! A token, browser return or HTTP success never proves financial capture.
#![forbid(unsafe_code)]

mod canonical;
mod client;
mod error;
mod fraud;
#[cfg(feature = "sqlite")]
mod inbox;
mod payments;
mod webhook;

pub use canonical::{canonical_json, canonical_request, digest, parse_json_strict};
pub use client::{Client, Config, Method, Request, Response};
pub use error::{Error, Result};
pub use fraud::Fraud;
#[cfg(feature = "sqlite")]
pub use inbox::SqliteInbox;
pub use payments::{Address, BasketItem, Buyer, IntentRequest, MAX_SAFE_INTEGER, Payments};
pub use webhook::{WebhookEvent, WebhookVerifier};

/// Semantic SDK version, also sent in the User-Agent header.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
