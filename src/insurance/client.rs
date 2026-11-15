//! `client.insurance()`: offer, sell and service insurance for basket
//! items, submit leads, download policy files and reconcile.

use super::values::{
    INSURANCE_CONSENT_FIELDS, INSURANCE_EXTERNAL_PAYMENT_FIELDS, INSURANCE_EXTERNAL_REFUND_FIELDS,
    INSURANCE_LEAD_FIELDS, INSURANCE_LEAD_NOTE_MAX_LENGTH, INSURANCE_MAX_CONTEXT_ITEMS,
    INSURANCE_MAX_LIMIT, INSURANCE_ORDER_FIELDS, INSURANCE_PRODUCT_SEARCH_FIELDS,
    INSURANCE_QUOTE_FIELDS, insurance_filters, insurance_lead_status,
    insurance_premium_collected_by, insurance_reference_kind,
};
use crate::{
    BasketItem, Client, Error, Method, Request, Response, Result,
    route_table::{
        CursorPage, minor_text, object_of, path_segment, present, require_currency, to_page,
        unwrap, valid_printable_key,
    },
};
use serde_json::{Map, Value, json};
use std::fmt;

const BASE: &str = "/api/v1/insurance";
const PDF_ACCEPT: &str = "application/pdf, application/json";
const CSV_ACCEPT: &str = "text/csv, application/json";
const CURSOR_MAX_LENGTH: usize = 512;
const TEXT_MAX_LENGTH: usize = 128;

/// Insurance — `/api/v1/insurance` (product `INSURANCE_SERVICE`;
/// `client.insurance()`).
///
/// Every call is signed, reads included; the key decides the environment.
/// Bodies and filters are JSON objects checked before sending (an unknown
/// top-level field or filter, a missing required value, a malformed
/// currency, UUID or amount is `Error::Invalid`); the field rules of an
/// insurance type's own object (`insured`, `tickets` …) are not repeated
/// here — a violation comes back as 422 `insurance.validation.invalid`.
/// `create_order`, `confirm_payment`, `issue_policy`, `cancel_order`,
/// `refund_premium` and `create_lead` require an `Idempotency-Key` (8–200
/// printable ASCII characters without spaces); `create_quote` and
/// `bind_payment` take one optionally. Answers are the envelope's `data`;
/// lists are [`CursorPage`]; policy files are [`InsuranceDocument`].
pub struct Insurance<'a> {
    client: &'a Client,
}
impl Client {
    /// Returns the insurance facade; no independent HTTP client is constructed.
    pub fn insurance(&self) -> Insurance<'_> {
        Insurance { client: self }
    }
}

/// A policy file (document, receipt or certificate): PDF bytes.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InsuranceDocument {
    /// `application/pdf`.
    pub content_type: Option<String>,
    /// The file.
    pub bytes: Vec<u8>,
}
impl fmt::Debug for InsuranceDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InsuranceDocument")
            .field("content_type", &self.content_type)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

/// The reconciliation report (`export_reconciliation`): CSV, UTF-8 with
/// BOM, one line per policy, amounts in minor units. A file holds at most
/// 20000 orders; when it was cut, `truncated` is true and `next_cursor`
/// continues it (pass it as `cursor`). `aborted` is true when the file ends
/// with the gateway's abort line: it is incomplete, request it again.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InsuranceReconciliationExport {
    /// `text/csv`.
    pub content_type: Option<String>,
    /// The CSV file.
    pub bytes: Vec<u8>,
    /// The file ends with the continuation line.
    pub truncated: bool,
    /// The cursor of the continuation, when `truncated`.
    pub next_cursor: Option<String>,
    /// The file ends with the abort line.
    pub aborted: bool,
}
impl fmt::Debug for InsuranceReconciliationExport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InsuranceReconciliationExport")
            .field("bytes", &self.bytes.len())
            .field("truncated", &self.truncated)
            .field("aborted", &self.aborted)
            .finish_non_exhaustive()
    }
}

impl Insurance<'_> {
    /// Insurance types offered for a payment's basket (`GET /products`):
    /// `currency` (required), `paymentIntentId` (without it, the types sold
    /// without a basket item), `insuranceType`. An empty list means insurance
    /// is not offered; it is not an error.
    pub async fn list_products(&self, filters: &Value) -> Result<Value> {
        let query = build_query(filters, "listProducts")?;
        require_filters(&query, &["currency"])?;
        Ok(unwrap(&self.get("/products", query).await?))
    }
    /// Insurance types offered for basket items before the payment exists
    /// (`POST /products`, read-only): `{currency, insuranceType?, context: {items}}`.
    pub async fn search_products(&self, request: &Value) -> Result<Value> {
        let body = keys_of(request, INSURANCE_PRODUCT_SEARCH_FIELDS)?;
        let mut payload = Map::new();
        payload.insert(
            "currency".to_owned(),
            Value::String(currency(body.get("currency"))?),
        );
        payload.insert("context".to_owned(), context_of(body.get("context"))?);
        if let Some(kind) = body.get("insuranceType").filter(|v| present(v)) {
            payload.insert(
                "insuranceType".to_owned(),
                Value::String(text(Some(kind), 64, "insurance insuranceType")?),
            );
        }
        self.post("/products", Some(Value::Object(payload)), None)
            .await
    }
    /// Lunixi reference codes of one kind ([`insurance_reference_kind`])
    /// (`GET /reference/{kind}`; `parentCode`, `limit`, `cursor`).
    pub async fn list_reference_values(&self, kind: &str, filters: &Value) -> Result<CursorPage> {
        if !insurance_reference_kind::ALL.contains(&kind) {
            return Err(Error::Invalid("insurance reference kind"));
        }
        let query = build_query(filters, "listReferenceValues")?;
        let path = format!("/reference/{}", path_segment(kind)?);
        Ok(to_page(&self.get(&path, query).await?))
    }
    /// Prices an insurance type (`POST /quotes`, HTTP 201). The basket is
    /// either a payment's recorded basket (`paymentIntentId`) or the items
    /// you will send in the payment (`context.items`), not both (both is
    /// `Error::Invalid`); neither for a type sold without a basket item.
    /// Every option has its own `quoteId`, `terms` and `expiresAt`. The key
    /// is optional and only correlates traces.
    pub async fn create_quote(
        &self,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<Value> {
        let mut body = keys_of(request, INSURANCE_QUOTE_FIELDS)?;
        let has_payment = body.get("paymentIntentId").is_some_and(present);
        let has_items = body.get("context").is_some_and(present);
        if has_payment && has_items {
            return Err(Error::Invalid(
                "insurance quote: send the basket as paymentIntentId or as context.items, not both",
            ));
        }
        let kind = text(
            body.get("insuranceType"),
            usize::MAX,
            "insurance insuranceType",
        )?;
        body.insert("insuranceType".to_owned(), Value::String(kind));
        let code = currency(body.get("currency"))?;
        body.insert("currency".to_owned(), Value::String(code));
        if has_payment {
            let id = uuid(
                body.get("paymentIntentId"),
                "insurance paymentIntentId (UUID)",
            )?;
            body.insert("paymentIntentId".to_owned(), Value::String(id));
        } else {
            body.remove("paymentIntentId");
        }
        if has_items {
            let context = context_of(body.get("context"))?;
            body.insert("context".to_owned(), context);
        } else {
            body.remove("context");
        }
        merchant_order_id(&mut body)?;
        let key = optional_key(idempotency_key)?;
        self.post("/quotes", Some(Value::Object(body)), key).await
    }
    /// One quote option; 404 `insurance.quote.not_found` for an unknown one.
    pub async fn get_quote(&self, quote_id: &str) -> Result<Value> {
        let path = format!("/quotes/{}", path_segment(quote_id)?);
        Ok(unwrap(&self.get(&path, vec![]).await?))
    }
    /// Turns a quote option into an order (`POST /orders`, HTTP 201) and
    /// records the buyer's consent: `{quoteId, merchantOrderId?, consent}`.
    /// [`consent_from_quote`](Self::consent_from_quote) builds `consent`.
    /// The same key with the same body returns the recorded order.
    pub async fn create_order(&self, request: &Value, idempotency_key: &str) -> Result<Value> {
        let key = required_key(idempotency_key)?;
        let mut body = keys_of(request, INSURANCE_ORDER_FIELDS)?;
        let consent = keys_of(
            body.get("consent").unwrap_or(&Value::Null),
            INSURANCE_CONSENT_FIELDS,
        )?;
        let consent = json!({
            "termsVersion": text(consent.get("termsVersion"), usize::MAX, "insurance consent.termsVersion")?,
            "termsSha256": text(consent.get("termsSha256"), usize::MAX, "insurance consent.termsSha256")?,
            "acceptedAt": iso_time(consent.get("acceptedAt"), "insurance consent.acceptedAt (ISO 8601 with a time zone)")?,
        });
        let quote_id = text(body.get("quoteId"), usize::MAX, "insurance quoteId")?;
        body.insert("quoteId".to_owned(), Value::String(quote_id));
        body.insert("consent".to_owned(), consent);
        merchant_order_id(&mut body)?;
        self.post("/orders", Some(Value::Object(body)), Some(key))
            .await
    }
    /// Your insurance orders, newest first (`limit`, `cursor`,
    /// `paymentIntentId`, `merchantOrderId`).
    pub async fn list_orders(&self, filters: &Value) -> Result<CursorPage> {
        let query = build_query(filters, "listOrders")?;
        Ok(to_page(&self.get("/orders", query).await?))
    }
    /// One order; 404 `insurance.order.not_found` for an unknown one.
    pub async fn get_order(&self, order_id: &str) -> Result<Value> {
        Ok(unwrap(&self.get(&order_path(order_id)?, vec![]).await?))
    }
    /// Binds an order to the Lunixi payment that collects its premium
    /// (`POST /orders/{orderId}/bind-payment`). When you collect it
    /// (`premiumCollectedBy: MERCHANT`) the payment basket must hold the
    /// premium item ([`premium_basket_item`](Self::premium_basket_item)).
    /// The key is optional.
    pub async fn bind_payment(
        &self,
        order_id: &str,
        payment_intent_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<Value> {
        let body = json!({
            "paymentIntentId": uuid(Some(&Value::from(payment_intent_id)), "insurance paymentIntentId (UUID)")?,
        });
        let key = optional_key(idempotency_key)?;
        let path = format!("{}/bind-payment", order_path(order_id)?);
        self.post(&path, Some(body), key).await
    }
    /// Records a premium you collected outside Lunixi
    /// (`POST /orders/{orderId}/confirm-payment`): `{externalPaymentId,
    /// paidAmount, paidCurrency, paidAt, paymentProvider?}`; `paidAmount` and
    /// `paidCurrency` must equal the order's premium.
    pub async fn confirm_payment(
        &self,
        order_id: &str,
        payment: &Value,
        idempotency_key: &str,
    ) -> Result<Value> {
        let key = required_key(idempotency_key)?;
        let body = keys_of(payment, INSURANCE_EXTERNAL_PAYMENT_FIELDS)?;
        let mut checked = Map::new();
        checked.insert(
            "externalPaymentId".to_owned(),
            Value::String(text(
                body.get("externalPaymentId"),
                TEXT_MAX_LENGTH,
                "insurance payment.externalPaymentId (at most 128 characters)",
            )?),
        );
        checked.insert(
            "paidAmount".to_owned(),
            Value::String(minor_text(
                body.get("paidAmount").unwrap_or(&Value::Null),
                true,
                "insurance payment.paidAmount: positive minor-unit text",
            )?),
        );
        checked.insert(
            "paidCurrency".to_owned(),
            Value::String(currency(body.get("paidCurrency"))?),
        );
        checked.insert(
            "paidAt".to_owned(),
            Value::String(iso_time(
                body.get("paidAt"),
                "insurance payment.paidAt (ISO 8601 with a time zone)",
            )?),
        );
        if let Some(provider) = body.get("paymentProvider").filter(|v| present(v)) {
            checked.insert(
                "paymentProvider".to_owned(),
                Value::String(text(
                    Some(provider),
                    64,
                    "insurance payment.paymentProvider (at most 64 characters)",
                )?),
            );
        }
        let path = format!("{}/confirm-payment", order_path(order_id)?);
        self.post(&path, Some(json!({ "payment": checked })), Some(key))
            .await
    }
    /// Requests the policy of an order whose premium you collected outside
    /// Lunixi (`POST /orders/{orderId}/issue-policy`; the result arrives by
    /// webhook). 412 `insurance.premium.not_captured` before the premium is collected.
    pub async fn issue_policy(&self, order_id: &str, idempotency_key: &str) -> Result<Value> {
        let key = required_key(idempotency_key)?;
        let path = format!("{}/issue-policy", order_path(order_id)?);
        self.post(&path, None, Some(key)).await
    }
    /// Opens a cancellation at the provider for every issued policy of the
    /// order (`POST /orders/{orderId}/cancel`). Cancelling does not refund
    /// the premium: call `refund_premium` after it.
    pub async fn cancel_order(&self, order_id: &str, idempotency_key: &str) -> Result<Value> {
        let key = required_key(idempotency_key)?;
        let path = format!("{}/cancel", order_path(order_id)?);
        self.post(&path, None, Some(key)).await
    }
    /// Refunds the premium (`POST /orders/{orderId}/refund-premium`). With a
    /// Lunixi payment pass `Value::Null` (or `{}`): the premium is refunded
    /// from that payment. For a premium collected outside Lunixi send the
    /// refund you made: `{externalRefund: {externalRefundId, refundedAt}}`.
    pub async fn refund_premium(
        &self,
        order_id: &str,
        options: &Value,
        idempotency_key: &str,
    ) -> Result<Value> {
        let key = required_key(idempotency_key)?;
        let options = if options.is_null() {
            &json!({})
        } else {
            options
        };
        let body = keys_of(options, &["externalRefund"])?;
        let mut payload = Map::new();
        if let Some(refund) = body.get("externalRefund").filter(|v| present(v)) {
            let refund = keys_of(refund, INSURANCE_EXTERNAL_REFUND_FIELDS)?;
            payload.insert(
                "externalRefund".to_owned(),
                json!({
                    "externalRefundId": text(
                        refund.get("externalRefundId"),
                        TEXT_MAX_LENGTH,
                        "insurance externalRefund.externalRefundId (at most 128 characters)",
                    )?,
                    "refundedAt": iso_time(
                        refund.get("refundedAt"),
                        "insurance externalRefund.refundedAt (ISO 8601 with a time zone)",
                    )?,
                }),
            );
        }
        let path = format!("{}/refund-premium", order_path(order_id)?);
        self.post(&path, Some(Value::Object(payload)), Some(key))
            .await
    }
    /// Submits a lead (`POST /leads`, HTTP 201) for an insurance type sold by
    /// request only: no premium, order, policy or document. `note` is sent as
    /// given (at most 1000 characters). The answer's `status` is `SUBMITTED`
    /// or `UNKNOWN` (not sent again); a provider refusal is 422
    /// `insurance.lead.rejected`.
    pub async fn create_lead(&self, request: &Value, idempotency_key: &str) -> Result<Value> {
        let key = required_key(idempotency_key)?;
        let mut body = keys_of(request, INSURANCE_LEAD_FIELDS)?;
        let kind = text(body.get("insuranceType"), 64, "insurance insuranceType")?;
        body.insert("insuranceType".to_owned(), Value::String(kind));
        match body.get("note") {
            Some(note) if present(note) => {
                let valid = note
                    .as_str()
                    .is_some_and(|text| text.chars().count() <= INSURANCE_LEAD_NOTE_MAX_LENGTH);
                if !valid {
                    return Err(Error::Invalid(
                        "insurance lead note: a string of at most 1000 characters",
                    ));
                }
            }
            Some(_) => {
                body.remove("note");
            }
            None => {}
        }
        self.post("/leads", Some(Value::Object(body)), Some(key))
            .await
    }
    /// Your leads, newest first (`limit`, `cursor`, `insuranceType`,
    /// `status`); personal data is masked in `requestMasked`.
    pub async fn list_leads(&self, filters: &Value) -> Result<CursorPage> {
        let query = build_query(filters, "listLeads")?;
        Ok(to_page(&self.get("/leads", query).await?))
    }
    /// One lead; 404 `insurance.lead.not_found` for an unknown one.
    pub async fn get_lead(&self, lead_id: &str) -> Result<Value> {
        let path = format!("/leads/{}", path_segment(lead_id)?);
        Ok(unwrap(&self.get(&path, vec![]).await?))
    }
    /// The policy PDF (`GET /policies/{policyId}/document`), read from the
    /// provider on every call while the policy is in force; 503
    /// `insurance.document.unavailable` is temporary.
    pub async fn get_policy_document(&self, policy_id: &str) -> Result<InsuranceDocument> {
        self.policy_file(policy_id, "document").await
    }
    /// The provider's premium receipt PDF (`GET /policies/{policyId}/receipt`);
    /// 412 `insurance.document.not_supported` for products without one.
    pub async fn get_policy_receipt(&self, policy_id: &str) -> Result<InsuranceDocument> {
        self.policy_file(policy_id, "receipt").await
    }
    /// The provider's policy certificate PDF (for example a Green Card)
    /// (`GET /policies/{policyId}/certificate`); 412
    /// `insurance.document.not_supported` for products without one.
    pub async fn get_policy_certificate(&self, policy_id: &str) -> Result<InsuranceDocument> {
        self.policy_file(policy_id, "certificate").await
    }
    /// Reconciliation totals grouped by provider connection, currency and
    /// `premiumCollectedBy` (`GET /reconciliation/summary`). `from`
    /// (inclusive) and `to` (exclusive) are required, ISO 8601 with a time
    /// zone, at most 366 days apart.
    pub async fn get_reconciliation_summary(&self, filters: &Value) -> Result<Value> {
        let query = build_query(filters, "getReconciliationSummary")?;
        require_filters(&query, &["from", "to"])?;
        Ok(unwrap(&self.get("/reconciliation/summary", query).await?))
    }
    /// The orders and policies behind the summary, newest first
    /// (`GET /reconciliation/orders`; `from`, `to` required).
    pub async fn list_reconciliation_orders(&self, filters: &Value) -> Result<CursorPage> {
        let query = build_query(filters, "listReconciliationOrders")?;
        require_filters(&query, &["from", "to"])?;
        Ok(to_page(&self.get("/reconciliation/orders", query).await?))
    }
    /// The reconciliation report as CSV (`GET /reconciliation/export`;
    /// `from`, `to` required, `cursor` continues a truncated file).
    pub async fn export_reconciliation(
        &self,
        filters: &Value,
    ) -> Result<InsuranceReconciliationExport> {
        let query = build_query(filters, "exportReconciliation")?;
        require_filters(&query, &["from", "to"])?;
        let mut request = Request::new(Method::GET, format!("{BASE}/reconciliation/export"));
        request.query = query;
        let response = self.client.send_binary(request, CSV_ACCEPT).await?;
        if response.envelope.is_some() {
            return Err(Error::Response("reconciliation CSV"));
        }
        let (truncated, next_cursor, aborted) = export_trailer(&response.bytes);
        Ok(InsuranceReconciliationExport {
            content_type: response.content_type,
            bytes: response.bytes,
            truncated,
            next_cursor,
            aborted,
        })
    }

    /// `consent` for `create_order`: the quote option's `terms` unchanged
    /// (`termsVersion`, `termsSha256`) and the moment the buyer accepted
    /// them (ISO 8601 with a time zone, not in the future).
    pub fn consent_from_quote(option: &Value, accepted_at: &str) -> Result<Value> {
        let terms = option
            .get("terms")
            .filter(|terms| terms.is_object())
            .ok_or(Error::Invalid(
                "insurance quote option has no terms (pass an option from create_quote or get_quote)",
            ))?;
        Ok(json!({
            "termsVersion": text(terms.get("version"), usize::MAX, "insurance terms.version")?,
            "termsSha256": text(terms.get("sha256"), usize::MAX, "insurance terms.sha256")?,
            "acceptedAt": iso_time(Some(&Value::from(accepted_at)), "insurance acceptedAt (ISO 8601 with a time zone)")?,
        }))
    }

    /// The premium basket item for a payment that collects the premium of an
    /// order (`premiumCollectedBy: MERCHANT`): `id` = `orderId`, item type
    /// `VIRTUAL`, `price` = `premiumAmount`. Add it to the payment's basket
    /// next to the insured items and add the premium to the payment amount.
    /// A `PROVIDER` order has none (`Error::Invalid`).
    pub fn premium_basket_item(
        order: &Value,
        name: &str,
        category1: &str,
        category2: Option<&str>,
    ) -> Result<BasketItem> {
        if order.get("premiumCollectedBy").and_then(Value::as_str)
            != Some(insurance_premium_collected_by::MERCHANT)
        {
            return Err(Error::Invalid(
                "only an order whose premium you collect (premiumCollectedBy MERCHANT) has a premium basket item",
            ));
        }
        let id = text(order.get("orderId"), usize::MAX, "insurance order.orderId")?;
        let price = minor_text(
            order.get("premiumAmount").unwrap_or(&Value::Null),
            true,
            "insurance order.premiumAmount: positive minor-unit text",
        )?;
        let name = text(
            Some(&Value::from(name)),
            usize::MAX,
            "insurance basket item name",
        )?;
        let category1 = text(
            Some(&Value::from(category1)),
            usize::MAX,
            "insurance basket item category1",
        )?;
        let category2 = category2
            .filter(|c| !c.is_empty())
            .map(|c| {
                text(
                    Some(&Value::from(c)),
                    usize::MAX,
                    "insurance basket item category2",
                )
            })
            .transpose()?;
        let mut item = BasketItem::new(id, name, "VIRTUAL", price);
        item.category1 = Some(category1);
        item.category2 = category2;
        Ok(item)
    }

    async fn get(&self, path: &str, query: Vec<(String, String)>) -> Result<Response> {
        let mut request = Request::new(Method::GET, format!("{BASE}{path}"));
        request.query = query;
        self.client.send(request).await
    }

    async fn post(&self, path: &str, body: Option<Value>, key: Option<String>) -> Result<Value> {
        let mut request = Request::new(Method::POST, format!("{BASE}{path}"));
        request.body = body;
        request.idempotency_key = key;
        Ok(unwrap(&self.client.send(request).await?))
    }

    async fn policy_file(&self, policy_id: &str, file: &str) -> Result<InsuranceDocument> {
        let path = format!("{BASE}/policies/{}/{file}", path_segment(policy_id)?);
        let response = self
            .client
            .send_binary(Request::new(Method::GET, path), PDF_ACCEPT)
            .await?;
        let pdf = response
            .content_type
            .as_deref()
            .is_some_and(|t| t.to_ascii_lowercase().starts_with("application/pdf"));
        if response.envelope.is_some() || !pdf || response.bytes.is_empty() {
            return Err(Error::Response("insurance PDF document"));
        }
        Ok(InsuranceDocument {
            content_type: response.content_type,
            bytes: response.bytes,
        })
    }
}

// ── Checks ──────────────────────────────────────────────────────────────────

fn order_path(order_id: &str) -> Result<String> {
    Ok(format!("/orders/{}", path_segment(order_id)?))
}

/// A JSON object with known keys only (`null` members dropped).
fn keys_of(value: &Value, allowed: &[&str]) -> Result<Map<String, Value>> {
    let object = object_of(value, "insurance request must be a JSON object")?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(Error::Invalid(
            "unknown insurance request field (the gateway rejects unknown fields with 400)",
        ));
    }
    Ok(object
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect())
}

/// A trimmed non-empty string of at most `max` characters.
fn text(value: Option<&Value>, max: usize, message: &'static str) -> Result<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.chars().count() <= max)
        .map(str::to_owned)
        .ok_or(Error::Invalid(message))
}

fn currency(value: Option<&Value>) -> Result<String> {
    require_currency(value, "insurance currency: an ISO 4217 code such as TRY")
}

/// A Lunixi id (UUID, either case).
fn uuid(value: Option<&Value>, message: &'static str) -> Result<String> {
    let id = value.and_then(Value::as_str).map(str::trim).unwrap_or("");
    let groups: Vec<&str> = id.split('-').collect();
    let lengths = [8, 4, 4, 4, 12];
    let valid = groups.len() == 5
        && groups
            .iter()
            .zip(lengths)
            .all(|(group, len)| group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit()));
    if valid {
        Ok(id.to_owned())
    } else {
        Err(Error::Invalid(message))
    }
}

/// An ISO 8601 time with a time zone, as non-empty text (the gateway checks
/// the format; a value without a zone is 422).
fn iso_time(value: Option<&Value>, message: &'static str) -> Result<String> {
    text(value, usize::MAX, message)
}

fn merchant_order_id(body: &mut Map<String, Value>) -> Result<()> {
    match body.get("merchantOrderId") {
        Some(value) if present(value) => {
            let id = text(
                Some(value),
                TEXT_MAX_LENGTH,
                "insurance merchantOrderId (at most 128 characters)",
            )?;
            body.insert("merchantOrderId".to_owned(), Value::String(id));
        }
        Some(_) => {
            body.remove("merchantOrderId");
        }
        None => {}
    }
    Ok(())
}

/// `context: {items}` — the basket items the payment carries (1–100).
fn context_of(value: Option<&Value>) -> Result<Value> {
    let context = value
        .filter(|v| present(v))
        .ok_or(Error::Invalid("insurance context.items is required"))?;
    let context = keys_of(context, &["items"])?;
    let items = context
        .get("items")
        .and_then(Value::as_array)
        .filter(|items| (1..=INSURANCE_MAX_CONTEXT_ITEMS).contains(&items.len()))
        .ok_or(Error::Invalid(
            "insurance context.items: 1 to 100 basket items",
        ))?;
    Ok(json!({ "items": items }))
}

fn optional_key(key: Option<&str>) -> Result<Option<String>> {
    match key.map(str::trim) {
        None | Some("") => Ok(None),
        Some(key) => required_key(key).map(Some),
    }
}

fn required_key(key: &str) -> Result<String> {
    let key = key.trim();
    if valid_printable_key(key) {
        Ok(key.to_owned())
    } else {
        Err(Error::Invalid(
            "insurance Idempotency-Key: 8-200 printable ASCII characters without spaces",
        ))
    }
}

fn require_filters(query: &[(String, String)], required: &[&str]) -> Result<()> {
    if required
        .iter()
        .all(|name| query.iter().any(|(key, _)| key == name))
    {
        Ok(())
    } else {
        Err(Error::Invalid("required insurance filter missing"))
    }
}

fn build_query(filters: &Value, route: &str) -> Result<Vec<(String, String)>> {
    let allowed = insurance_filters(route).ok_or(Error::Invalid("insurance route"))?;
    if filters.is_null() {
        return Ok(vec![]);
    }
    let object = object_of(filters, "insurance filters must be a JSON object")?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(Error::Invalid(
            "unknown insurance filter (the gateway rejects unknown query fields with 400)",
        ));
    }
    let mut query = vec![];
    for key in allowed {
        let Some(value) = object
            .get(*key)
            .filter(|v| !v.is_null() && v.as_str() != Some(""))
        else {
            continue;
        };
        query.push(((*key).to_owned(), filter_value(key, value)?));
    }
    Ok(query)
}

fn filter_value(key: &str, value: &Value) -> Result<String> {
    match key {
        "limit" => value
            .as_i64()
            .filter(|n| (1..=INSURANCE_MAX_LIMIT).contains(n))
            .map(|n| n.to_string())
            .ok_or(Error::Invalid("insurance limit: an integer from 1 to 100")),
        "cursor" => value
            .as_str()
            .filter(|cursor| cursor.len() <= CURSOR_MAX_LENGTH)
            .map(str::to_owned)
            .ok_or(Error::Invalid(
                "insurance cursor: the nextCursor of the previous page, unchanged",
            )),
        "currency" => currency(Some(value)),
        "paymentIntentId" | "connectionId" => uuid(Some(value), "insurance filter id (UUID)"),
        "from" | "to" => iso_time(
            Some(value),
            "insurance from / to (ISO 8601 with a time zone)",
        ),
        "insuranceType" | "parentCode" => text(
            Some(value),
            64,
            "insurance filter text (at most 64 characters)",
        ),
        "merchantOrderId" => text(
            Some(value),
            TEXT_MAX_LENGTH,
            "insurance merchantOrderId (at most 128 characters)",
        ),
        "status" => value
            .as_str()
            .filter(|status| insurance_lead_status::ALL.contains(status))
            .map(str::to_owned)
            .ok_or(Error::Invalid("insurance lead status filter")),
        _ => Err(Error::Invalid("unknown insurance filter")),
    }
}

/// `(truncated, next_cursor, aborted)` from the CSV's last non-empty line.
fn export_trailer(bytes: &[u8]) -> (bool, Option<String>, bool) {
    let text = String::from_utf8_lossy(bytes);
    let last = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .rfind(|line| !line.is_empty())
        .unwrap_or("");
    let line = last.trim_end();
    if line == "# export-aborted=true" {
        return (false, None, true);
    }
    let cursor = line
        .strip_prefix("# truncated=true orderCap=")
        .and_then(|rest| rest.split_once(" nextCursor="))
        .filter(|(cap, _)| !cap.is_empty() && cap.bytes().all(|b| b.is_ascii_digit()))
        .map(|(_, cursor)| cursor)
        .filter(|cursor| !cursor.is_empty() && !cursor.contains(char::is_whitespace));
    match cursor {
        Some(cursor) => (true, Some(cursor.to_owned()), false),
        None => (false, None, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_trailers() {
        assert_eq!(
            export_trailer(b"a,b\r\n# truncated=true orderCap=20000 nextCursor=abc\r\n"),
            (true, Some("abc".to_owned()), false)
        );
        assert_eq!(
            export_trailer(b"a,b\n# export-aborted=true\n"),
            (false, None, true)
        );
        assert_eq!(export_trailer(b"a,b\n1,2\n"), (false, None, false));
    }

    #[test]
    fn uuids() {
        let id = Value::from("7B0C6A1E-2F8D-4C55-9A51-3F0E8D1C2A90");
        assert!(uuid(Some(&id), "x").is_ok());
        assert!(uuid(Some(&Value::from("not-a-uuid")), "x").is_err());
    }
}
