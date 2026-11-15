//! Sanctions screening (`/api/v1/sanction`): [`Sanction`]
//! (`client.sanction()`), checked against [`SANCTION_ROUTES`], and the
//! clear-read rule [`is_clear_screening`].

mod client;
mod values;

pub use client::Sanction;
pub use values::*;
