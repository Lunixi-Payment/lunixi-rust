//! `client.kyc()`: KYC, KYB and evidence sessions and the KYC admin routes.

use super::values::{KYC_ROUTES, KYC_SESSION_TTL_MAX_MINUTES, kyc_session_type};
use crate::{
    Client, Error, Result,
    route_request::{
        RouteCall, compact, data, idempotency_key_of, list_page, object_or_empty, optional_text,
        require_text, send_route,
    },
    route_table::{CursorPage, object_of},
};
use serde_json::{Map, Value, json};

/// KYC / KYB / evidence sessions — `/api/v1/kyc/*` (`client.kyc()`).
///
/// Your server creates the session (`externalCustomerId` = your own user
/// id, echoed on every webhook); the browser SDK then runs the capture with
/// the publishable key. The `kyc.*` webhook is the authority for the
/// verification result.
///
/// Bodies and filters are JSON objects checked against
/// [`KYC_ROUTES`](crate::KYC_ROUTES) before anything is sent. Session calls
/// return the envelope's `data` (`{session, capturePolicy, …}`; no personal
/// data comes back, only status, decision, risk and the achieved
/// assurance). Lists are [`CursorPage`]. A document image never appears in
/// an error.
pub struct Kyc<'a> {
    client: &'a Client,
}
impl Client {
    /// Returns the KYC facade; no independent HTTP client is constructed.
    pub fn kyc(&self) -> Kyc<'_> {
        Kyc { client: self }
    }
}

impl Kyc<'_> {
    async fn entity(&self, name: &str, call: RouteCall<'_>) -> Result<Value> {
        Ok(data(
            &send_route(self.client, KYC_ROUTES, name, call).await?,
        ))
    }
    async fn page(&self, name: &str, call: RouteCall<'_>) -> Result<CursorPage> {
        Ok(list_page(
            &send_route(self.client, KYC_ROUTES, name, call).await?,
        ))
    }
    async fn on_session(
        &self,
        name: &str,
        session_id: &str,
        body: Option<&Value>,
    ) -> Result<Value> {
        let call = RouteCall {
            params: &[session_id],
            body,
            ..RouteCall::default()
        };
        self.entity(name, call).await
    }

    /// Creates a session: `subjectId`, `sessionType`
    /// ([`kyc_session_type`](crate::kyc_session_type), case-insensitive),
    /// `externalCustomerId?`, `customerMode?`, one of `person?` / `company?`
    /// / `customer?`, `jurisdiction?`, `requiredDocumentTypes?`,
    /// `primaryDocumentType?`, `evidencePurpose?`, `ttlMinutes?` (1–1440),
    /// `metadata?`. `person` / `company` are sent as `customer` with
    /// `customerType` `PERSON` / `COMPANY`; `metadata` is sent as the
    /// `evidenceContextJson` text. An `Idempotency-Key` (argument or the
    /// request's own `idempotencyKey`) goes in the header and the body.
    pub async fn create_session(
        &self,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<Value> {
        let (body, key) = session_body(request, idempotency_key)?;
        let call = RouteCall {
            body: Some(&body),
            key: key.as_deref(),
            ..RouteCall::default()
        };
        self.entity("createSession", call).await
    }
    /// One session.
    pub async fn get_session(&self, session_id: &str) -> Result<Value> {
        self.on_session("getSession", session_id, None).await
    }
    /// Sessions; filters `limit`, `cursor` (`pageSize` / `pageToken` are read
    /// as the same). Anything else is refused: the merchant list has no
    /// status or customer filter, and dropping one would return every session.
    pub async fn list_sessions(&self, filters: &Value) -> Result<CursorPage> {
        let given = compact(&object_or_empty(filters, "filters must be a JSON object")?);
        if given
            .keys()
            .any(|key| !["limit", "cursor", "pageSize", "pageToken"].contains(&key.as_str()))
        {
            return Err(Error::Invalid(
                "listSessions takes only limit and cursor (pageSize / pageToken)",
            ));
        }
        let query = json!({
            "limit": given.get("limit").or_else(|| given.get("pageSize")),
            "cursor": given.get("cursor").or_else(|| given.get("pageToken")),
        });
        let call = RouteCall {
            query: Some(&query),
            ..RouteCall::default()
        };
        self.page("listSessions", call).await
    }
    /// Applicant jurisdiction (ISO country) of a server-created session.
    pub async fn set_jurisdiction(&self, session_id: &str, jurisdiction: &str) -> Result<Value> {
        let body =
            json!({ "jurisdiction": require_text(jurisdiction, "jurisdiction is required")? });
        self.on_session("setJurisdiction", session_id, Some(&body))
            .await
    }
    /// `PASSPORT`, `NATIONAL_ID`, `DRIVERS_LICENSE` or `RESIDENCE_PERMIT`.
    pub async fn set_primary_document(
        &self,
        session_id: &str,
        document_type: &str,
    ) -> Result<Value> {
        let body =
            json!({ "documentType": require_text(document_type, "documentType is required")? });
        self.on_session("setPrimaryDocument", session_id, Some(&body))
            .await
    }
    /// Server-side document upload (most flows use the browser SDK):
    /// `{documentType, fileBase64, mimeType, side?, jurisdiction?,
    /// metadataJson?, idempotencyKey?}`.
    pub async fn upload_document(&self, session_id: &str, document: &Value) -> Result<Value> {
        object_of(document, "uploadDocument document must be a JSON object")?;
        self.on_session("uploadDocument", session_id, Some(document))
            .await
    }
    /// Session event timeline (audit).
    pub async fn timeline(&self, session_id: &str) -> Result<Value> {
        self.on_session("timeline", session_id, None).await
    }
    /// Submits the session for processing.
    pub async fn submit(&self, session_id: &str) -> Result<Value> {
        self.on_session("submit", session_id, None).await
    }
    /// Sends the session to manual review.
    pub async fn manual_review(
        &self,
        session_id: &str,
        reason_code: Option<&str>,
        reason_detail: Option<&str>,
    ) -> Result<Value> {
        let body = json!({
            "reasonCode": optional_text(reason_code),
            "reasonDetail": optional_text(reason_detail),
        });
        self.on_session("manualReview", session_id, Some(&body))
            .await
    }
    /// An `EVIDENCE` session (proof-of-X), optionally linked to a payment or
    /// order; the same body and key rule as `create_session`.
    pub async fn evidence_request(
        &self,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<Value> {
        let (body, key) = session_body(request, idempotency_key)?;
        let call = RouteCall {
            body: Some(&body),
            key: key.as_deref(),
            ..RouteCall::default()
        };
        self.entity("evidenceRequest", call).await
    }
    /// Your KYC policy (required assurance, verification steps …).
    pub async fn merchant_policy(&self) -> Result<Value> {
        self.entity("merchantPolicy", RouteCall::default()).await
    }
    /// Updates your KYC policy (the gateway validates the body).
    pub async fn update_merchant_policy(&self, policy: &Value) -> Result<Value> {
        object_of(policy, "merchant policy must be a JSON object")?;
        let call = RouteCall {
            body: Some(policy),
            ..RouteCall::default()
        };
        self.entity("updateMerchantPolicy", call).await
    }
    /// Rotates the KYC webhook signing secret.
    pub async fn rotate_webhook_secret(&self) -> Result<Value> {
        self.entity("rotateWebhookSecret", RouteCall::default())
            .await
    }
    /// KYC webhook deliveries; filters `limit`, `status`.
    pub async fn webhook_outbox(&self, filters: &Value) -> Result<CursorPage> {
        let call = RouteCall {
            query: Some(filters),
            ..RouteCall::default()
        };
        self.page("webhookOutbox", call).await
    }
    /// Delivers a KYC webhook event again.
    pub async fn retry_webhook_event(&self, event_id: &str) -> Result<Value> {
        let call = RouteCall {
            params: &[event_id],
            ..RouteCall::default()
        };
        self.entity("retryWebhookEvent", call).await
    }
    /// Data erasure requests; filters `limit`, `status`.
    pub async fn erasure_requests(&self, filters: &Value) -> Result<CursorPage> {
        let call = RouteCall {
            query: Some(filters),
            ..RouteCall::default()
        };
        self.page("erasureRequests", call).await
    }
}

fn session_body(request: &Value, idempotency_key: Option<&str>) -> Result<(Value, Option<String>)> {
    let mut body = compact(&object_or_empty(
        request,
        "KYC session request must be a JSON object",
    )?);
    let person = body.remove("person");
    let company = body.remove("company");
    let metadata = body.remove("metadata");
    let key = idempotency_key_of(&body, idempotency_key)?;
    if let Some(key) = &key {
        body.insert("idempotencyKey".to_owned(), Value::from(key.as_str()));
    }
    if let Some(session_type) = body.get("sessionType") {
        let upper = session_type
            .as_str()
            .map(str::to_ascii_uppercase)
            .filter(|value| kyc_session_type::ALL.contains(&value.as_str()))
            .ok_or(Error::Invalid("sessionType must be KYC, KYB or EVIDENCE"))?;
        body.insert("sessionType".to_owned(), Value::String(upper));
    }
    let identities = [
        person.is_some(),
        company.is_some(),
        body.contains_key("customer"),
    ];
    if identities.iter().filter(|given| **given).count() > 1 {
        return Err(Error::Invalid(
            "a KYC session takes one of person, company or customer",
        ));
    }
    for (value, customer_type) in [(person, "PERSON"), (company, "COMPANY")] {
        if let Some(value) = value {
            let mut customer = Map::new();
            customer.insert("customerType".to_owned(), Value::from(customer_type));
            for (field, entry) in object_of(&value, "person / company must be a JSON object")? {
                customer.insert(field.clone(), entry.clone());
            }
            body.insert("customer".to_owned(), Value::Object(customer));
        }
    }
    if let Some(metadata) = metadata {
        if body.contains_key("evidenceContextJson") {
            return Err(Error::Invalid(
                "a KYC session takes metadata or evidenceContextJson, not both",
            ));
        }
        object_of(&metadata, "metadata must be a JSON object")?;
        let text = serde_json::to_string(&metadata)
            .map_err(|_| Error::Invalid("metadata must be serializable"))?;
        body.insert("evidenceContextJson".to_owned(), Value::String(text));
    }
    if let Some(ttl) = body.get("ttlMinutes")
        && !ttl
            .as_i64()
            .is_some_and(|minutes| (1..=KYC_SESSION_TTL_MAX_MINUTES).contains(&minutes))
    {
        return Err(Error::Invalid(
            "ttlMinutes must be an integer from 1 to 1440",
        ));
    }
    if let Some(types) = body.get("requiredDocumentTypes")
        && !types
            .as_array()
            .is_some_and(|list| list.iter().all(Value::is_string))
    {
        return Err(Error::Invalid(
            "requiredDocumentTypes must be an array of document types",
        ));
    }
    Ok((Value::Object(body), key))
}
