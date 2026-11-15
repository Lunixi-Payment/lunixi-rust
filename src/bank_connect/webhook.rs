//! Readers of verified `bank_account.*` and `bank_transfer.*` webhooks.

use super::values::{BANK_CONNECT_EVENT_TYPES, BANK_TRANSFER_EVENT_TYPES};
use crate::{
    Result, WebhookEvent,
    route_table::{webhook_data, webhook_minor, webhook_text},
};
use std::fmt;

/// The `data` of a verified `bank_account.transaction.received` webhook
/// ([`bank_connect_events`](crate::bank_connect_events)). Only the
/// allow-listed fields are read; fields without a value are `None`. Dedupe
/// on `event_id` (the same on every redelivery).
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BankAccountTransactionReceived {
    /// The event id.
    pub event_id: String,
    pub transaction_id: String,
    pub account_id: String,
    pub connection_id: Option<String>,
    pub bank_eft_code: Option<String>,
    pub bank_name: Option<String>,
    /// `CREDIT` or `DEBIT`.
    pub direction: String,
    /// Positive minor-unit text.
    pub amount_minor: String,
    pub currency: String,
    pub booked_at: Option<String>,
    pub value_at: Option<String>,
    pub received_at: Option<String>,
    /// [`bank_transaction_type`](crate::bank_transaction_type) value.
    pub canonical_type: Option<String>,
    /// The bank's own text, as sent.
    pub description: Option<String>,
    pub counterparty_name_masked: Option<String>,
    pub counterparty_iban_masked: Option<String>,
    pub receipt_no: Option<String>,
}
impl fmt::Debug for BankAccountTransactionReceived {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BankAccountTransactionReceived")
            .field("event_id", &self.event_id)
            .field("transaction_id", &self.transaction_id)
            .field("direction", &self.direction)
            .finish_non_exhaustive()
    }
}
impl BankAccountTransactionReceived {
    /// Reads an event the [`WebhookVerifier`](crate::WebhookVerifier)
    /// accepted. Another event type, missing core fields or a malformed
    /// amount is `Error::Invalid`.
    pub fn from_event(event: &WebhookEvent) -> Result<Self> {
        let data = webhook_data(
            event,
            BANK_CONNECT_EVENT_TYPES,
            "not a bank_account.* webhook event",
        )?;
        let text = |key| webhook_text(data, key);
        let required =
            |key| webhook_text(data, key).ok_or(crate::Error::Invalid("bank account webhook data"));
        Ok(Self {
            event_id: event.id().to_owned(),
            transaction_id: required("transactionId")?,
            account_id: required("accountId")?,
            connection_id: text("connectionId"),
            bank_eft_code: text("bankEftCode"),
            bank_name: text("bankName"),
            direction: required("direction")?,
            amount_minor: webhook_minor(data, "amountMinor", "bank account webhook amount")?,
            currency: required("currency")?,
            booked_at: text("bookedAt"),
            value_at: text("valueAt"),
            received_at: text("receivedAt"),
            canonical_type: text("canonicalType"),
            description: text("description"),
            counterparty_name_masked: text("counterpartyNameMasked"),
            counterparty_iban_masked: text("counterpartyIbanMasked"),
            receipt_no: text("receiptNo"),
        })
    }
}

/// The `data` of a verified `bank_transfer.*` webhook
/// ([`bank_transfer_events`](crate::bank_transfer_events)). Only the
/// allow-listed fields are read; fields without a value are `None`. One
/// event per transfer and type; dedupe on `event_id`.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BankTransferWebhookData {
    /// The event id.
    pub event_id: String,
    /// The event type ([`bank_transfer_events`](crate::bank_transfer_events)).
    pub event_type: String,
    pub transfer_id: String,
    pub connection_id: Option<String>,
    pub account_id: Option<String>,
    pub bank_eft_code: Option<String>,
    pub bank_name: Option<String>,
    /// [`bank_transfer_status`](crate::bank_transfer_status) value.
    pub status: String,
    /// [`bank_transfer_rail`](crate::bank_transfer_rail) value, when known.
    pub rail: Option<String>,
    /// Positive minor-unit text.
    pub amount_minor: String,
    pub currency: String,
    pub receiver_iban_masked: Option<String>,
    pub receiver_name_masked: Option<String>,
    pub payment_purpose: Option<String>,
    pub description: Option<String>,
    /// `bank_connect.transfer.*` on a failed transfer.
    pub failure_code: Option<String>,
    pub idempotency_key: Option<String>,
    pub created_at: Option<String>,
    pub completed_at: Option<String>,
    pub failed_at: Option<String>,
    pub returned_at: Option<String>,
}
impl fmt::Debug for BankTransferWebhookData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BankTransferWebhookData")
            .field("event_id", &self.event_id)
            .field("event_type", &self.event_type)
            .field("transfer_id", &self.transfer_id)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}
impl BankTransferWebhookData {
    /// Reads an event the [`WebhookVerifier`](crate::WebhookVerifier)
    /// accepted. Another event type (a payment `payment.bank_transfer.*`
    /// event included), missing core fields or a malformed amount is
    /// `Error::Invalid`.
    pub fn from_event(event: &WebhookEvent) -> Result<Self> {
        let data = webhook_data(
            event,
            BANK_TRANSFER_EVENT_TYPES,
            "not a bank_transfer.* webhook event",
        )?;
        let text = |key| webhook_text(data, key);
        let required = |key| {
            webhook_text(data, key).ok_or(crate::Error::Invalid("bank transfer webhook data"))
        };
        Ok(Self {
            event_id: event.id().to_owned(),
            event_type: event.event_type().to_owned(),
            transfer_id: required("transferId")?,
            connection_id: text("connectionId"),
            account_id: text("accountId"),
            bank_eft_code: text("bankEftCode"),
            bank_name: text("bankName"),
            status: required("status")?,
            rail: text("rail"),
            amount_minor: webhook_minor(data, "amountMinor", "bank transfer webhook amount")?,
            currency: required("currency")?,
            receiver_iban_masked: text("receiverIbanMasked"),
            receiver_name_masked: text("receiverNameMasked"),
            payment_purpose: text("paymentPurpose"),
            description: text("description"),
            failure_code: text("failureCode"),
            idempotency_key: text("idempotencyKey"),
            created_at: text("createdAt"),
            completed_at: text("completedAt"),
            failed_at: text("failedAt"),
            returned_at: text("returnedAt"),
        })
    }
}
