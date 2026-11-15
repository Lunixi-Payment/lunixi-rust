//! Insurance (`/api/v1/insurance`): [`Insurance`] (`client.insurance()`)
//! with products, quotes, orders, premium payment and refund, policy files,
//! reconciliation and leads; [`InsuranceWebhookData`] and
//! [`InsuranceLeadWebhookData`] read verified webhooks.

mod client;
mod values;
mod webhook;

pub use client::{Insurance, InsuranceDocument, InsuranceReconciliationExport};
pub use values::*;
pub use webhook::{
    InsuranceLeadWebhookData, InsuranceWebhookData, InsuranceWebhookLead, InsuranceWebhookPolicy,
};
