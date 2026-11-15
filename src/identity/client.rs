//! `client.identity()`: device and account-linking signals.

use super::values::IDENTITY_ROUTES;
use crate::{
    Client, Error, Result,
    route_request::{RouteCall, data, list_page, object_member, send_route},
    route_table::{CursorPage, object_of},
};
use serde_json::{Value, json};

/// Identity signals — `/api/v1/identity/*` (`client.identity()`).
///
/// The browser fingerprint SDK produces a `traceId`; your server reads the
/// linked accounts of that trace and flags or blocks when the device is
/// shared ("one person, many accounts",
/// [`is_identity_device_shared`](crate::is_identity_device_shared)).
///
/// This is a signal surface: it carries risk flags, reason codes,
/// similarity probabilities and the identity snapshot, not an allow /
/// review / block verdict (that is the fraud decision). Bodies and filters
/// are checked against [`IDENTITY_ROUTES`](crate::IDENTITY_ROUTES). Calls
/// return the envelope's `data`; `list_sessions` returns a [`CursorPage`].
pub struct Identity<'a> {
    client: &'a Client,
}
impl Client {
    /// Returns the identity facade; no independent HTTP client is constructed.
    pub fn identity(&self) -> Identity<'_> {
        Identity { client: self }
    }
}

impl Identity<'_> {
    async fn entity(&self, name: &str, call: RouteCall<'_>) -> Result<Value> {
        Ok(data(
            &send_route(self.client, IDENTITY_ROUTES, name, call).await?,
        ))
    }
    async fn on_trace(&self, name: &str, trace_id: &str, query: Option<&Value>) -> Result<Value> {
        let call = RouteCall {
            params: &[trace_id],
            query,
            ..RouteCall::default()
        };
        self.entity(name, call).await
    }

    /// Synchronous analysis: `{subjectType?, subjectId?, productContext?,
    /// integrationMode?, customer?, device?, session?, source?, account?,
    /// metadata?, paymentId?}`. Returns `{traceId, analysisStatus,
    /// riskFlags, reasonCodes, identitySnapshot, …}`.
    pub async fn analyze_sync(&self, request: &Value) -> Result<Value> {
        let call = RouteCall {
            body: Some(analyze_body(request)?),
            ..RouteCall::default()
        };
        self.entity("analyzeSync", call).await
    }
    /// Queues the analysis and returns the `traceId` at once.
    pub async fn analyze(&self, request: &Value) -> Result<Value> {
        let call = RouteCall {
            body: Some(analyze_body(request)?),
            ..RouteCall::default()
        };
        self.entity("analyze", call).await
    }
    /// Other accounts seen on this device / fingerprint / IP (account keys hashed).
    pub async fn account_links(&self, trace_id: &str) -> Result<Value> {
        self.on_trace("accountLinks", trace_id, None).await
    }
    /// One analysed session.
    pub async fn get_session(&self, trace_id: &str) -> Result<Value> {
        self.on_trace("getSession", trace_id, None).await
    }
    /// Recent sessions of the same device-identity cluster.
    pub async fn related_sessions(&self, trace_id: &str, limit: Option<u32>) -> Result<Value> {
        let query = json!({ "limit": limit });
        self.on_trace("relatedSessions", trace_id, Some(&query))
            .await
    }
    /// Session summary and recorded lifecycle events.
    pub async fn timeline(&self, trace_id: &str) -> Result<Value> {
        self.on_trace("timeline", trace_id, None).await
    }
    /// Source domains ranked by repeated risky activity; filters `days`, `limit`.
    pub async fn top_sources(&self, filters: &Value) -> Result<Value> {
        let call = RouteCall {
            query: Some(filters),
            ..RouteCall::default()
        };
        self.entity("topSources", call).await
    }
    /// Analysed sessions; filters `fingerprintId`, `deviceId`, `paymentId`, `limit`.
    pub async fn list_sessions(&self, filters: &Value) -> Result<CursorPage> {
        let call = RouteCall {
            query: Some(filters),
            ..RouteCall::default()
        };
        Ok(list_page(
            &send_route(self.client, IDENTITY_ROUTES, "listSessions", call).await?,
        ))
    }
}

/// The nested signal blocks are objects; `subjectType` and `subjectId` go together.
fn analyze_body(request: &Value) -> Result<&Value> {
    let body = object_of(request, "identity analyze request must be a JSON object")?;
    for field in [
        "customer", "device", "session", "source", "account", "metadata",
    ] {
        object_member(
            body,
            field,
            "customer, device, session, source, account and metadata must be JSON objects",
        )?;
    }
    let given = |field: &str| body.get(field).is_some_and(|value| !value.is_null());
    if given("subjectType") != given("subjectId") {
        return Err(Error::Invalid("subjectType and subjectId go together"));
    }
    Ok(request)
}
