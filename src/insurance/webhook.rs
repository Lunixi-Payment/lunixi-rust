//! Readers of verified `insurance.*` webhooks.

use super::values::insurance_events;
use crate::{
    Error, Result, WebhookEvent,
    route_table::{webhook_data, webhook_minor, webhook_text},
};
use serde_json::{Map, Value};

/// A policy in an order webhook (`data.policies[]`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InsuranceWebhookPolicy {
    pub policy_id: String,
    pub basket_item_id: Option<String>,
    pub policy_number: Option<String>,
    /// [`insurance_policy_status`](crate::insurance_policy_status) value.
    pub policy_status: Option<String>,
    /// [`insurance_document_status`](crate::insurance_document_status) value.
    pub document_status: Option<String>,
    /// The `get_policy_document` address while the policy is in force
    /// (downloading it is a signed API call); `None` otherwise.
    pub document_url: Option<String>,
}

/// The `data` of a verified order webhook
/// ([`insurance_events::ORDER_EVENTS`](crate::insurance_events::ORDER_EVENTS)).
/// No personal data is delivered. Decide on the order from the webhook;
/// dedupe on `event_id`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InsuranceWebhookData {
    /// The event id (`data.eventId`, else the envelope's).
    pub event_id: String,
    /// The event type.
    pub event_type: String,
    pub merchant_id: Option<String>,
    pub insurance_order_id: String,
    pub payment_intent_id: Option<String>,
    pub merchant_order_id: Option<String>,
    pub insurance_type: Option<String>,
    /// `MERCHANT` or `PROVIDER`.
    pub premium_collected_by: Option<String>,
    /// `{code, name, logoUrl, connectionId}`.
    pub provider: Option<Value>,
    pub status: Option<String>,
    pub collection_status: Option<String>,
    pub policy_status: Option<String>,
    pub refund_status: Option<String>,
    pub document_status: Option<String>,
    /// Minor-unit text.
    pub premium_amount: Option<String>,
    pub currency: Option<String>,
    pub policies: Vec<InsuranceWebhookPolicy>,
    /// Only on `insurance.policy.failed` and `insurance.refund.required`.
    pub error_code: Option<String>,
    pub created_at: Option<String>,
}
impl InsuranceWebhookData {
    /// Reads an order event the [`WebhookVerifier`](crate::WebhookVerifier)
    /// accepted. A lead event, another product's event, a missing
    /// `insuranceOrderId` or a malformed amount is `Error::Invalid`.
    pub fn from_event(event: &WebhookEvent) -> Result<Self> {
        let data = webhook_data(
            event,
            insurance_events::ORDER_EVENTS,
            "not an insurance order webhook event",
        )?;
        let text = |key| webhook_text(data, key);
        let premium_amount = match data.get("premiumAmount") {
            None | Some(Value::Null) => None,
            Some(_) => Some(webhook_minor(
                data,
                "premiumAmount",
                "insurance webhook premiumAmount",
            )?),
        };
        let policies = data
            .get("policies")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_object)
                    .map(policy)
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            event_id: text("eventId").unwrap_or_else(|| event.id().to_owned()),
            event_type: event.event_type().to_owned(),
            merchant_id: text("merchantId"),
            insurance_order_id: text("insuranceOrderId")
                .ok_or(Error::Invalid("insurance webhook insuranceOrderId"))?,
            payment_intent_id: text("paymentIntentId"),
            merchant_order_id: text("merchantOrderId"),
            insurance_type: text("insuranceType"),
            premium_collected_by: text("premiumCollectedBy"),
            provider: data.get("provider").filter(|p| p.is_object()).cloned(),
            status: text("status"),
            collection_status: text("collectionStatus"),
            policy_status: text("policyStatus"),
            refund_status: text("refundStatus"),
            document_status: text("documentStatus"),
            premium_amount,
            currency: text("currency"),
            policies,
            error_code: text("errorCode"),
            created_at: text("createdAt"),
        })
    }
}

fn policy(row: &Map<String, Value>) -> InsuranceWebhookPolicy {
    let text = |key| webhook_text(row, key);
    InsuranceWebhookPolicy {
        policy_id: text("policyId").unwrap_or_default(),
        basket_item_id: text("basketItemId"),
        policy_number: text("policyNumber"),
        policy_status: text("policyStatus"),
        document_status: text("documentStatus"),
        document_url: text("documentUrl"),
    }
}

/// The lead in an `insurance.lead.*` webhook (no personal data, no note).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InsuranceWebhookLead {
    pub lead_id: String,
    pub insurance_type: Option<String>,
    pub schema_version: Option<i64>,
    /// [`insurance_lead_status`](crate::insurance_lead_status) value.
    pub status: Option<String>,
    pub submitted_at: Option<String>,
    pub created_at: Option<String>,
}

/// The `data` of a verified lead webhook
/// ([`insurance_events::LEAD_EVENTS`](crate::insurance_events::LEAD_EVENTS)).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InsuranceLeadWebhookData {
    /// The event id (`data.eventId`, else the envelope's).
    pub event_id: String,
    /// The event type.
    pub event_type: String,
    pub merchant_id: Option<String>,
    pub lead: InsuranceWebhookLead,
    pub created_at: Option<String>,
}
impl InsuranceLeadWebhookData {
    /// Reads a lead event the [`WebhookVerifier`](crate::WebhookVerifier)
    /// accepted. An order event, another product's event or a missing
    /// `lead.leadId` is `Error::Invalid`.
    pub fn from_event(event: &WebhookEvent) -> Result<Self> {
        let data = webhook_data(
            event,
            insurance_events::LEAD_EVENTS,
            "not an insurance lead webhook event",
        )?;
        let lead = data
            .get("lead")
            .and_then(Value::as_object)
            .ok_or(Error::Invalid("insurance lead webhook lead"))?;
        let lead_text = |key| webhook_text(lead, key);
        Ok(Self {
            event_id: webhook_text(data, "eventId").unwrap_or_else(|| event.id().to_owned()),
            event_type: event.event_type().to_owned(),
            merchant_id: webhook_text(data, "merchantId"),
            lead: InsuranceWebhookLead {
                lead_id: lead_text("leadId")
                    .ok_or(Error::Invalid("insurance lead webhook leadId"))?,
                insurance_type: lead_text("insuranceType"),
                schema_version: lead.get("schemaVersion").and_then(Value::as_i64),
                status: lead_text("status"),
                submitted_at: lead_text("submittedAt"),
                created_at: lead_text("createdAt"),
            },
            created_at: webhook_text(data, "createdAt"),
        })
    }
}
