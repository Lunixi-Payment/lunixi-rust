//! Verified physical POS webhooks (`physical_pos.*`).

use super::{money::validate_response_money, values::physical_pos_events};
use crate::{Error, Result, WebhookEvent, WebhookVerifier, parse_json_strict};
use reqwest::header::HeaderMap;
use serde_json::Value;
use std::fmt;

const PREFIX: &str = "physical_pos.";

/// The original payment of a reversal event (`data.original`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PhysicalPosOriginal {
    /// The original payment.
    pub payment_id: String,
    /// Its source: `API` or `DEVICE_DIRECT` (an open set).
    pub origin: String,
}
impl PhysicalPosOriginal {
    /// `{paymentId, origin}` when `data.original` carries both as non-empty
    /// strings (other keys are dropped); otherwise `None` (a root payment).
    pub fn from_data(data: &Value) -> Option<Self> {
        let original = data.get("original")?.as_object()?;
        Some(Self {
            payment_id: text(original.get("paymentId"))?,
            origin: text(original.get("origin"))?,
        })
    }
}

/// The device cashier of a payment event (`data.cashier`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PhysicalPosCashier {
    /// The cashier's id.
    pub cashier_id: String,
    /// The cashier's code on the device.
    pub cashier_code: String,
}
impl PhysicalPosCashier {
    /// `{cashierId, cashierCode}` when `data.cashier` carries both as
    /// non-empty strings (other keys are dropped); otherwise `None` (no
    /// cashier session — the payload then has no `cashier` key).
    pub fn from_data(data: &Value) -> Option<Self> {
        let cashier = data.get("cashier")?.as_object()?;
        Some(Self {
            cashier_id: text(cashier.get("cashierId"))?,
            cashier_code: text(cashier.get("cashierCode"))?,
        })
    }
}

/// Event categories (`PhysicalPosWebhookEvent::category`).
pub mod physical_pos_event_category {
    /// `physical_pos.payment.*`.
    pub const PAYMENT: &str = "payment";
    /// `physical_pos.fiscal_document.*`.
    pub const FISCAL_DOCUMENT: &str = "fiscal_document";
    /// `physical_pos.settlement.*` (end-of-day; no `historySeq`).
    pub const SETTLEMENT: &str = "settlement";
    /// Any other `physical_pos.*` event.
    pub const OTHER: &str = "other";
}

/// The category of a `physical_pos.*` event type.
pub fn physical_pos_category_of(event_type: &str) -> &'static str {
    use physical_pos_event_category::*;
    if event_type.starts_with("physical_pos.payment.") {
        PAYMENT
    } else if event_type.starts_with("physical_pos.fiscal_document.") {
        FISCAL_DOCUMENT
    } else if event_type.starts_with("physical_pos.settlement.") {
        SETTLEMENT
    } else {
        OTHER
    }
}

/// A verified `physical_pos.*` webhook. There is no way in without the
/// signature: build it with [`parse`](Self::parse) from the exact raw body,
/// the headers and your verifier, or [`from_verified`](Self::from_verified)
/// from an event the verifier already accepted.
///
/// The event body is the envelope's `data`; a `payloadJson` envelope (object
/// or JSON string) is read the same way. Money in the payload is minor-unit
/// text (`"4500"`), checked after the signature. Delivery order is not
/// guaranteed and an event can arrive more than once: deduplicate by `id`
/// and ignore an event whose `history_seq` is not greater than the last one
/// you applied for the same `data.paymentId`. End-of-day events carry no
/// `historySeq`; deduplicate them by `id`.
#[derive(Clone, PartialEq)]
#[non_exhaustive]
pub struct PhysicalPosWebhookEvent {
    /// Event id bound to the signature.
    pub id: String,
    /// Event type from the signed body.
    pub event_type: String,
    /// `payment`, `fiscal_document`, `settlement` or `other`
    /// ([`physical_pos_event_category`](crate::physical_pos_event_category) values).
    pub category: &'static str,
    /// The envelope's `environment` (`TEST` / `LIVE`), when present.
    pub environment: Option<String>,
    /// The envelope's `createdAt` (or `occurredAt` / `timestamp`).
    pub created_at: Option<String>,
    /// `data.historySeq` of payment events.
    pub history_seq: Option<i64>,
    /// The reversal's original payment, `None` on a root payment.
    pub original: Option<PhysicalPosOriginal>,
    /// The device cashier, `None` without a cashier session.
    pub cashier: Option<PhysicalPosCashier>,
    /// `true` for `physical_pos.payment.device_sale` (a sale made on the device).
    pub is_device_sale: bool,
    /// The event body. May contain customer data; never log it.
    pub data: Value,
    /// The whole signed envelope.
    pub raw: Value,
}
impl fmt::Debug for PhysicalPosWebhookEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PhysicalPosWebhookEvent")
            .field("id", &self.id)
            .field("type", &self.event_type)
            .field("history_seq", &self.history_seq)
            .finish_non_exhaustive()
    }
}
impl PhysicalPosWebhookEvent {
    /// Verifies (signature, event id, timestamp) and reads a physical POS
    /// webhook from the exact raw body and its headers. A signature failure
    /// is `Error::Webhook`; a verified body that is not a physical POS event,
    /// has no event data or carries a malformed amount is `Error::Invalid`.
    pub fn parse(verifier: &WebhookVerifier, raw: &[u8], headers: &HeaderMap) -> Result<Self> {
        Self::from_verified(&verifier.verify(raw, headers)?)
    }

    /// Reads an event the verifier already accepted.
    pub fn from_verified(event: &WebhookEvent) -> Result<Self> {
        let envelope = parse_json_strict(event.raw()).map_err(|_| Error::Webhook)?;
        if !envelope.is_object() {
            return Err(Error::Invalid("physical POS webhook envelope"));
        }
        let event_type = event.event_type();
        if !event_type.starts_with(PREFIX) {
            return Err(Error::Invalid("not a physical POS webhook event"));
        }
        let data = payload_of(&envelope)?;
        validate_response_money(&data, false)
            .map_err(|_| Error::Invalid("physical POS webhook amount"))?;
        Ok(Self {
            id: text(envelope.get("id"))
                .or_else(|| text(envelope.get("eventId")))
                .unwrap_or_else(|| event.id().to_owned()),
            event_type: event_type.to_owned(),
            category: physical_pos_category_of(event_type),
            environment: text(envelope.get("environment")),
            created_at: text(envelope.get("createdAt"))
                .or_else(|| text(envelope.get("occurredAt")))
                .or_else(|| text(envelope.get("timestamp"))),
            history_seq: data
                .get("historySeq")
                .and_then(Value::as_i64)
                .filter(|n| (-crate::MAX_SAFE_INTEGER..=crate::MAX_SAFE_INTEGER).contains(n)),
            original: PhysicalPosOriginal::from_data(&data),
            cashier: PhysicalPosCashier::from_data(&data),
            is_device_sale: event_type == physical_pos_events::PAYMENT_DEVICE_SALE,
            data,
            raw: envelope,
        })
    }
}

fn non_empty_object(value: &Value) -> bool {
    value.as_object().is_some_and(|map| !map.is_empty())
}

fn payload_of(envelope: &Value) -> Result<Value> {
    if let Some(data) = envelope.get("data").filter(|d| non_empty_object(d)) {
        return Ok(data.clone());
    }
    let payload = match envelope.get("payloadJson") {
        Some(Value::String(json)) => parse_json_strict(json.as_bytes())
            .map_err(|_| Error::Invalid("physical POS webhook payloadJson"))?,
        Some(other) => other.clone(),
        None => Value::Null,
    };
    if non_empty_object(&payload) {
        Ok(payload)
    } else {
        Err(Error::Invalid(
            "physical POS webhook carries no event data (data / payloadJson)",
        ))
    }
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
