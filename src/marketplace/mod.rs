//! Marketplace sub-dealers (`/api/v1/marketplace`) and split charges
//! (`/api/v1/payments/marketplace`): [`Marketplace`]
//! (`client.marketplace()`), checked against [`MARKETPLACE_ROUTES`].

mod client;
mod values;

pub use client::Marketplace;
pub use values::*;
