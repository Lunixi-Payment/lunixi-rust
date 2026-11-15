//! Identity signals (`/api/v1/identity`): [`Identity`] (`client.identity()`),
//! checked against [`IDENTITY_ROUTES`], and the shared-device helpers
//! [`is_identity_device_shared`] and [`identity_link_reasons`].

mod client;
mod values;

pub use client::Identity;
pub use values::*;
