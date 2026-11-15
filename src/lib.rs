//! Server-side Lunixi merchant APIs and verified webhooks.
//!
//! Share one [`Client`] per merchant, use integer minor units, and persist
//! payment attempt idempotency keys before calling financial mutations.
//! A token, browser return or HTTP success never proves financial capture.
#![forbid(unsafe_code)]

mod bank_connect;
mod bank_transfer;
mod canonical;
mod cards;
mod client;
mod error;
mod fraud;
mod identity;
#[cfg(feature = "sqlite")]
mod inbox;
mod insurance;
mod kyc;
mod marketplace;
mod marketplace_intent;
mod method_payment;
mod payment_link;
pub mod payment_method;
mod payments;
mod physical_pos;
pub mod product_category;
mod promissory_note;
mod reference;
mod route_request;
mod route_table;
mod sanction;
mod split_payment;
mod subscriptions;
mod wallet;
mod webhook;

pub use bank_connect::{
    BANK_CONNECT_BOOKED_RANGE_MAX_DAYS, BANK_CONNECT_EVENT_PATTERN, BANK_CONNECT_EVENT_TYPES,
    BANK_CONNECT_FILTERS, BANK_CONNECT_MAX_ACCOUNT_FILTER, BANK_CONNECT_MAX_LIMIT,
    BANK_TRANSFER_DESCRIPTION_MAX, BANK_TRANSFER_EVENT_PATTERN, BANK_TRANSFER_EVENT_TYPES,
    BANK_TRANSFER_IBAN_MAX, BANK_TRANSFER_IDEMPOTENCY_KEY_PATTERN,
    BANK_TRANSFER_PAYMENT_PURPOSE_PATTERN, BANK_TRANSFER_RECEIVER_NAME_MAX,
    BANK_TRANSFER_RECEIVER_NAME_MIN, BANK_TRANSFER_ROUTES, BankAccountTransactionReceived,
    BankConnect, BankConnectFx, BankConnectTransfers, BankTransferWebhookData, bank_account_status,
    bank_availability, bank_capability, bank_connect_events, bank_connect_filters,
    bank_connection_status, bank_fx_order_status, bank_fx_requested_side, bank_sync_status,
    bank_transaction_direction, bank_transaction_type, bank_transfer_events,
    bank_transfer_initiated_via, bank_transfer_rail, bank_transfer_status,
};
pub use canonical::{canonical_json, canonical_request, digest, parse_json_strict};
pub use cards::{InstallmentOptionsRequest, StoreCardRequest};
pub use client::{Client, Config, Method, Request, Response};
pub use error::{Error, Result};
pub use fraud::{FRAUD_EVENT_BATCH_LIMIT, Fraud};
pub use identity::{
    IDENTITY_ANALYZE_FIELDS, IDENTITY_ROUTES, Identity, identity_link_reasons,
    is_identity_device_shared,
};
#[cfg(feature = "sqlite")]
pub use inbox::SqliteInbox;
pub use insurance::{
    INSURANCE_CONSENT_FIELDS, INSURANCE_EVENT_PATTERN, INSURANCE_EVENT_TYPES,
    INSURANCE_EXPORT_ORDER_CAP, INSURANCE_EXTERNAL_PAYMENT_FIELDS,
    INSURANCE_EXTERNAL_REFUND_FIELDS, INSURANCE_FILTERS, INSURANCE_IDEMPOTENCY_KEY_PATTERN,
    INSURANCE_LEAD_FIELDS, INSURANCE_LEAD_NOTE_MAX_LENGTH, INSURANCE_MAX_CONTEXT_ITEMS,
    INSURANCE_MAX_LIMIT, INSURANCE_ORDER_FIELDS, INSURANCE_PRODUCT_SEARCH_FIELDS,
    INSURANCE_QUOTE_FIELDS, INSURANCE_TYPE_OBJECT_FIELDS, Insurance, InsuranceDocument,
    InsuranceLeadWebhookData, InsuranceReconciliationExport, InsuranceWebhookData,
    InsuranceWebhookLead, InsuranceWebhookPolicy, insurance_collection_status,
    insurance_document_status, insurance_events, insurance_filters, insurance_lead_status,
    insurance_order_status, insurance_order_surface, insurance_payment_mode,
    insurance_policy_status, insurance_premium_collected_by, insurance_quote_status,
    insurance_reference_kind, insurance_refund_status,
};
pub use kyc::{
    KYC_EVENT_TYPES, KYC_ROUTES, KYC_SESSION_TERMINAL_STATES, KYC_SESSION_TTL_MAX_MINUTES, Kyc,
    is_kyc_session_terminal, kyc_assurance_domain, kyc_assurance_level, kyc_assurance_meets,
    kyc_assurance_rank, kyc_events, kyc_evidence_purpose, kyc_session_status, kyc_session_type,
};
pub use marketplace::{
    MARKETPLACE_CHARGE_REQUIRED_FIELDS, MARKETPLACE_DEALER_ONBOARDING_HOSTED_BASE,
    MARKETPLACE_ROUTES, MARKETPLACE_SELLER_FIELDS, Marketplace, marketplace_dealer_status,
    marketplace_dealer_type, marketplace_multi_seller_policy, marketplace_onboarding_url,
};
pub use marketplace_intent::{MarketplaceIntentRequest, MarketplaceSeller};
pub use payment_link::{
    CreatePaymentLinkRequest, PAYMENT_LINK_ACTIONS, PAYMENT_LINK_ATTEMPT_LIST_FILTERS,
    PAYMENT_LINK_BUYER_FIELDS, PAYMENT_LINK_IMMUTABLE_FIELDS, PAYMENT_LINK_LIST_FILTERS,
    PAYMENT_LINK_PAYMENT_LIST_FILTERS, PAYMENT_LINK_QR_MAX_SIZE, PAYMENT_LINK_QR_MIN_SIZE,
    PAYMENT_LINK_SPEC_FIELDS, PAYMENT_LINK_TIME_FIELDS, PaymentLinkActionOptions, PaymentLinkBody,
    PaymentLinkCustomField, PaymentLinkImageUpload, PaymentLinkItem, PaymentLinkSendRequest,
    PaymentLinks, buyer_field_requirement, custom_field_type, payment_link_amount_mode,
    payment_link_attempt_state, payment_link_button_label, payment_link_delivery_channel,
    payment_link_environment, payment_link_image_content_type, payment_link_locale,
    payment_link_qr_format, payment_link_state, payment_link_usage,
};
pub use payments::{
    Address, BasketItem, Buyer, IntentRequest, MAX_ENABLED_INSTALLMENT, MAX_SAFE_INTEGER,
    MIN_ENABLED_INSTALLMENT, Payments,
};
pub use physical_pos::{
    PHYSICAL_POS_CARD_DATA_KEY_RULE, PHYSICAL_POS_EVENT_PATTERN, PHYSICAL_POS_EVENT_TYPES,
    PHYSICAL_POS_FILTERS, PHYSICAL_POS_FREE_TEXT_FILTERS, PHYSICAL_POS_IDEMPOTENCY_KEY_MAX,
    PHYSICAL_POS_IDEMPOTENCY_KEY_MIN, PHYSICAL_POS_MAX_LIMIT, PHYSICAL_POS_NOT_STARTED_CODES,
    PHYSICAL_POS_PAYMENT_TARGET_FIELDS, PHYSICAL_POS_REFUND_ITEM_FIELDS,
    PHYSICAL_POS_REQUEST_FIELDS, PHYSICAL_POS_ROW_VERSION_MAX, PHYSICAL_POS_SALE_ITEM_FIELDS,
    PhysicalPos, PhysicalPosCardDataKeyRule, PhysicalPosCashier, PhysicalPosFieldKinds,
    PhysicalPosManagement, PhysicalPosOriginal, PhysicalPosPage, PhysicalPosWebhookEvent,
    PhysicalPosWriteResult, physical_pos_category_of, physical_pos_contains_pan_like_value,
    physical_pos_event_category, physical_pos_events, physical_pos_filters,
    physical_pos_find_card_data_field, physical_pos_is_card_data_key,
    physical_pos_is_valid_idempotency_key, physical_pos_payment_operation,
    physical_pos_payment_status, physical_pos_request_fields, physical_pos_reversal_status,
};
pub use route_table::{CursorPage, NestedFields, Route};
pub use sanction::{
    SANCTION_NESTED_FIELDS, SANCTION_ROUTES, SANCTION_VALUE_SETS, Sanction, is_clear_screening,
    sanction_coverage_status, sanction_events, sanction_hit_type, sanction_risk_domain,
    sanction_screen_mode, sanction_subject_type, sanction_wallet_screen_mode,
};
pub use subscriptions::{
    SUBSCRIPTION_ACTIVE_STATES, SUBSCRIPTION_BILLING_RULE_FIELDS, SUBSCRIPTION_EVENT_TYPES,
    SUBSCRIPTION_PORTAL_HOSTED_BASE, SUBSCRIPTION_ROUTES, SUBSCRIPTION_TERMINAL_STATES,
    SubscriptionAddons, SubscriptionCards, SubscriptionCustomers, SubscriptionFeatures,
    SubscriptionInvoices, SubscriptionPlans, SubscriptionPortal, SubscriptionProducts,
    SubscriptionUsage, Subscriptions, is_subscription_active, is_subscription_terminal,
    subscription_billing_frequency, subscription_card_verification_mode,
    subscription_collection_method, subscription_events, subscription_invoice_status,
    subscription_plan_change_behavior, subscription_portal_sdk_mode, subscription_portal_url,
    subscription_status,
};
pub use wallet::{
    WALLET_AMOUNT_FIELDS, WALLET_AMOUNT_MAX_DIGITS, WALLET_AMOUNT_PATTERN,
    WALLET_IDEMPOTENCY_KEY_MAX, WALLET_NESTED_FIELDS, WALLET_ROUTES, Wallet, WalletAccounts,
    WalletBulkPayouts, WalletCorporate, WalletDeposits, WalletEndUsers, WalletFees,
    WalletOperations, WalletPayments, WalletPrograms, WalletQr, WalletQrImage, WalletTopups,
    WalletTransfers, WalletWithdrawals, wallet_account_type, wallet_bulk_target_type,
    wallet_corporate_account_status, wallet_end_user_status, wallet_kyc_level, wallet_otp_channel,
    wallet_qr_image_format, wallet_qr_type, wallet_route,
};
pub use webhook::{WebhookEvent, WebhookVerifier};

pub use bank_transfer::{
    BANK_TRANSFER_EXCEPTION_LIMIT_MAX, BANK_TRANSFER_OTHER_BANKS_CODE,
    BankTransferExceptionFilters, BankTransferInstruction, bank_transfer_exception_kind,
    bank_transfer_exception_status, payment_bank_transfer_events,
};
pub use method_payment::{
    CARD_ONLY_FIELDS, DIRECT_METHOD_PAYMENT_FIELDS, DirectMethodPaymentBody,
    DirectMethodPaymentRequest, METHOD_BLOCK_FIELDS, PAYMENT_ACTION_CONFIRM_FIELDS,
    SHOPPING_LOAN_MAX_BASKET_ITEMS, payment_action_type, provider_selection,
    provider_selection_mode,
};
pub use promissory_note::{
    PROMISSORY_NOTE_MAX_SCHEDULE_ITEMS, PromissoryNoteScheduleRow,
    promissory_note_cancel_error_code, promissory_note_cancel_result,
    promissory_note_document_locale, promissory_note_document_type, promissory_note_events,
    promissory_note_status,
};
pub use reference::Reference;
pub use split_payment::{
    LIST_SPLIT_PAYMENTS_FILTERS, SPLIT_LEG_BLOCK_FIELDS, SPLIT_LEG_EVENTS_SUBSCRIPTION,
    SPLIT_PAYMENT_LEG_FIELDS, split_payment_events,
};

/// Semantic SDK version, also sent in the User-Agent header.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

mod money;
pub use money::decimal_to_minor;
