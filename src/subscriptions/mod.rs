//! Subscriptions (`/api/v1/subscriptions`): [`Subscriptions`]
//! (`client.subscriptions()`) with the plan, customer, card, invoice,
//! portal, catalog, feature, add-on and usage sub-facades, checked against
//! [`SUBSCRIPTION_ROUTES`].

mod client;
mod values;

pub use client::{
    SubscriptionAddons, SubscriptionCards, SubscriptionCustomers, SubscriptionFeatures,
    SubscriptionInvoices, SubscriptionPlans, SubscriptionPortal, SubscriptionProducts,
    SubscriptionUsage, Subscriptions,
};
pub(crate) use values::encode_component;
pub use values::{
    SUBSCRIPTION_ACTIVE_STATES, SUBSCRIPTION_BILLING_RULE_FIELDS, SUBSCRIPTION_EVENT_TYPES,
    SUBSCRIPTION_PORTAL_HOSTED_BASE, SUBSCRIPTION_ROUTES, SUBSCRIPTION_TERMINAL_STATES,
    is_subscription_active, is_subscription_terminal, subscription_billing_frequency,
    subscription_card_verification_mode, subscription_collection_method,
    subscription_customer_error_code, subscription_events, subscription_invoice_status,
    subscription_plan_change_behavior, subscription_portal_sdk_mode, subscription_portal_url,
    subscription_status,
};
