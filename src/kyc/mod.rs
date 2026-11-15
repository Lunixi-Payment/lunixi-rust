//! KYC, KYB and evidence sessions (`/api/v1/kyc`): [`Kyc`] (`client.kyc()`),
//! checked against [`KYC_ROUTES`], and the assurance helpers
//! [`kyc_assurance_meets`] and [`is_kyc_session_terminal`].

mod client;
mod values;

pub use client::Kyc;
pub use values::*;
