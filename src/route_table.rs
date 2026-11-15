//! Route tables and the request plumbing shared by the wallet, sanction and
//! bank-connect transfer clients, plus the cursor page every new product
//! list returns. The subscription, KYC, identity and marketplace clients
//! send their routes through `route_request`.
//!
//! A route table describes each route once: the HTTP method, the path
//! template (`:name` segments filled from arguments, in order), whether the
//! gateway requires an `Idempotency-Key`, and the body / query fields it
//! accepts. The clients check a call against its route before anything is
//! sent; the repository's parity test compares the tables with the shared
//! catalog field by field.

use crate::{Error, Method, Response, Result, client::safe_header};
use serde_json::{Map, Value};
use std::fmt;

/// One gateway route of a route table ([`WALLET_ROUTES`](crate::WALLET_ROUTES),
/// [`SANCTION_ROUTES`](crate::SANCTION_ROUTES),
/// [`BANK_TRANSFER_ROUTES`](crate::BANK_TRANSFER_ROUTES),
/// [`SUBSCRIPTION_ROUTES`](crate::SUBSCRIPTION_ROUTES),
/// [`KYC_ROUTES`](crate::KYC_ROUTES),
/// [`IDENTITY_ROUTES`](crate::IDENTITY_ROUTES),
/// [`MARKETPLACE_ROUTES`](crate::MARKETPLACE_ROUTES)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Route {
    /// The route's name in the table (`createEndUser`, `screenName`,
    /// `plans.create` …).
    pub name: &'static str,
    /// `GET`, `POST`, `PUT`, `PATCH` or `DELETE`.
    pub method: &'static str,
    /// Path template; `:name` segments are filled from the arguments.
    pub path: &'static str,
    /// The gateway rejects the call without an `Idempotency-Key`.
    pub idempotency_key: bool,
    /// A sanction screening: the answer must be `status: "success"` to be read.
    pub screening: bool,
    /// A 2xx answer is a file (an image), not the JSON envelope.
    pub binary: bool,
    /// Body fields the call cannot do without.
    pub required: &'static [&'static str],
    /// Other body fields the gateway accepts.
    pub optional: &'static [&'static str],
    /// At least one of these body fields must be present (a list: non-empty).
    pub any_of: &'static [&'static str],
    /// Accepted query parameters.
    pub query: &'static [&'static str],
    /// The route accepts an `Idempotency-Key` without requiring it.
    pub idempotency_key_optional: bool,
    /// The catalog's `stepUp`: the gateway wants the per-request signature.
    /// This SDK signs every call, so the flag only describes the route.
    pub step_up: bool,
    /// The body is a JSON object the gateway validates (no field list here).
    pub free_body: bool,
    /// An empty body is sent as no body at all.
    pub optional_body: bool,
    /// Any scalar filter is accepted (the list has no fixed filter set).
    pub any_query: bool,
}
impl Route {
    pub(crate) const fn new(name: &'static str, method: &'static str, path: &'static str) -> Self {
        Self {
            name,
            method,
            path,
            idempotency_key: false,
            screening: false,
            binary: false,
            required: &[],
            optional: &[],
            any_of: &[],
            query: &[],
            idempotency_key_optional: false,
            step_up: false,
            free_body: false,
            optional_body: false,
            any_query: false,
        }
    }
    pub(crate) const fn with_key(mut self) -> Self {
        self.idempotency_key = true;
        self
    }
    pub(crate) const fn with_screening(mut self) -> Self {
        self.screening = true;
        self
    }
    pub(crate) const fn with_binary(mut self) -> Self {
        self.binary = true;
        self
    }
    pub(crate) const fn with_body(
        mut self,
        required: &'static [&'static str],
        optional: &'static [&'static str],
    ) -> Self {
        self.required = required;
        self.optional = optional;
        self
    }
    pub(crate) const fn with_any_of(mut self, any_of: &'static [&'static str]) -> Self {
        self.any_of = any_of;
        self
    }
    pub(crate) const fn with_query(mut self, query: &'static [&'static str]) -> Self {
        self.query = query;
        self
    }
    pub(crate) const fn with_optional_key(mut self) -> Self {
        self.idempotency_key_optional = true;
        self
    }
    pub(crate) const fn with_step_up(mut self) -> Self {
        self.step_up = true;
        self
    }
    pub(crate) const fn with_free_body(mut self) -> Self {
        self.free_body = true;
        self
    }
    pub(crate) const fn with_optional_body(mut self) -> Self {
        self.optional_body = true;
        self
    }
    pub(crate) const fn with_any_query(mut self) -> Self {
        self.any_query = true;
        self
    }
    /// `true` when the route takes a JSON body (a field list or a free body).
    pub fn has_body(&self) -> bool {
        self.method != "GET"
            && (self.free_body || !(self.required.is_empty() && self.optional.is_empty()))
    }
    /// The `Idempotency-Key` rule as the catalog writes it: `"required"`,
    /// `"optional"` or `"none"`.
    pub fn idempotency_key_rule(&self) -> &'static str {
        if self.idempotency_key {
            "required"
        } else if self.idempotency_key_optional {
            "optional"
        } else {
            "none"
        }
    }
    /// `true` when `field` is one of the route's body fields.
    pub fn accepts(&self, field: &str) -> bool {
        self.required.contains(&field) || self.optional.contains(&field)
    }
    pub(crate) fn http_method(&self) -> Method {
        match self.method {
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "PATCH" => Method::PATCH,
            "DELETE" => Method::DELETE,
            _ => Method::GET,
        }
    }
}

/// The element fields of a nested list (`rows`, `items`, `parties` …).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct NestedFields {
    /// Fields every element must carry.
    pub required: &'static [&'static str],
    /// Other fields an element may carry.
    pub optional: &'static [&'static str],
}
impl NestedFields {
    pub(crate) const fn new(
        required: &'static [&'static str],
        optional: &'static [&'static str],
    ) -> Self {
        Self { required, optional }
    }
}

/// One page of a list. Pass `next_cursor` back unchanged as the list's
/// cursor filter to read the next page (`cursor`; `pageToken` for the
/// subscription, plan, customer, card, invoice and audit-log lists).
#[derive(Clone, PartialEq)]
#[non_exhaustive]
pub struct CursorPage {
    /// The page's rows.
    pub items: Vec<Value>,
    /// `true` when another page exists (and `next_cursor` is set).
    pub has_more: bool,
    /// The opaque cursor of the next page.
    pub next_cursor: Option<String>,
    /// The total number of rows, when the list reports it.
    pub total_count: Option<i64>,
    /// The page size (subscription, plan, customer, card, invoice and
    /// audit-log lists: the answer's `pageSize`, else the row count).
    pub page_size: Option<i64>,
    /// The answer's whole `data` (summary fields beside the rows).
    pub raw: Value,
    /// The gateway's `X-Request-Id`.
    pub request_id: Option<String>,
}
impl fmt::Debug for CursorPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CursorPage")
            .field("items", &self.items.len())
            .field("has_more", &self.has_more)
            .field("total_count", &self.total_count)
            .finish_non_exhaustive()
    }
}

pub(crate) fn find(table: &'static [Route], name: &str) -> Result<&'static Route> {
    table
        .iter()
        .find(|route| route.name == name)
        .ok_or(Error::Invalid("route name"))
}

/// A path segment: trimmed, non-empty, not `.` / `..`, percent-encoded
/// (only unreserved characters stay as they are).
pub(crate) fn path_segment(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value == "." || value == ".." || value.len() > 512 {
        return Err(Error::Invalid("path identifier (a non-empty id)"));
    }
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    Ok(out)
}

/// Fills the `:name` segments of a route's path with `params`, in order.
pub(crate) fn fill_path(route: &Route, params: &[&str]) -> Result<String> {
    let mut values = params.iter();
    let mut out = Vec::new();
    for segment in route.path.split('/') {
        if segment.starts_with(':') {
            let value = values
                .next()
                .ok_or(Error::Invalid("path identifier (a non-empty id)"))?;
            out.push(path_segment(value)?);
        } else {
            out.push(segment.to_owned());
        }
    }
    if values.next().is_some() {
        return Err(Error::Invalid("path identifier count"));
    }
    Ok(out.join("/"))
}

/// Not null, not `""`, not an empty list.
pub(crate) fn present(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.is_empty(),
        Value::Array(list) => !list.is_empty(),
        _ => true,
    }
}

pub(crate) fn object_of<'v>(
    value: &'v Value,
    message: &'static str,
) -> Result<&'v Map<String, Value>> {
    value.as_object().ok_or(Error::Invalid(message))
}

/// Drops null members, then refuses an unknown field or a missing required
/// one. Field names are reported, values never.
pub(crate) fn check_fields(
    object: &Map<String, Value>,
    required: &[&str],
    optional: &[&str],
) -> Result<Map<String, Value>> {
    let plain: Map<String, Value> = object
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if plain
        .keys()
        .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    {
        return Err(Error::Invalid(
            "request field not accepted by this route (the gateway rejects unknown fields)",
        ));
    }
    if required
        .iter()
        .any(|key| !plain.get(*key).is_some_and(present))
    {
        return Err(Error::Invalid("required request field missing"));
    }
    Ok(plain)
}

/// Query parameters of a route from a JSON object (`Value::Null` for
/// none), in the route's order. Strings, numbers and booleans are sent as
/// text; `null` and `""` leave a filter out; an unknown filter is refused.
pub(crate) fn query_pairs(allowed: &[&str], filters: &Value) -> Result<Vec<(String, String)>> {
    if filters.is_null() {
        return Ok(vec![]);
    }
    let object = object_of(filters, "filters must be a JSON object")?;
    if object
        .iter()
        .any(|(key, value)| !value.is_null() && !allowed.contains(&key.as_str()))
    {
        return Err(Error::Invalid(
            "filter not accepted by this route (the gateway rejects unknown query fields)",
        ));
    }
    let mut pairs = vec![];
    for key in allowed {
        let Some(value) = object.get(*key) else {
            continue;
        };
        if let Some(text) = scalar_text(value)? {
            pairs.push(((*key).to_owned(), text));
        }
    }
    Ok(pairs)
}

/// A scalar filter as text; `None` for null and `""`.
pub(crate) fn scalar_text(value: &Value) -> Result<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) if text.is_empty() => Ok(None),
        Value::String(text) => Ok(Some(text.clone())),
        Value::Number(number) => Ok(Some(number.to_string())),
        Value::Bool(flag) => Ok(Some(flag.to_string())),
        _ => Err(Error::Invalid(
            "filter must be a string, a number or a boolean",
        )),
    }
}

/// `true` for an `Idempotency-Key` of 8–200 printable ASCII characters
/// without spaces (bank-connect transfers, insurance, physical POS).
pub(crate) fn valid_printable_key(key: &str) -> bool {
    key.len() >= 8 && safe_header(key, 200)
}

/// Subscription, KYC, identity and marketplace `Idempotency-Key`: 1–200
/// printable ASCII characters without spaces.
pub(crate) fn valid_route_key(key: &str) -> bool {
    safe_header(key, 200)
}

/// Wallet `Idempotency-Key`: 1–128 printable ASCII characters (inner
/// spaces allowed), without surrounding whitespace.
pub(crate) fn valid_wallet_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key.trim() == key
        && key.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

fn under(path: &str, base: &str) -> bool {
    path.strip_prefix(base)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The `Idempotency-Key` rule of a merchant API path.
pub(crate) fn key_rule(path: &str) -> fn(&str) -> bool {
    if crate::physical_pos::is_physical_pos_path(path) {
        crate::physical_pos::valid_idempotency_key
    } else if under(path, "/api/v1/bank-connect") || under(path, "/api/v1/insurance") {
        valid_printable_key
    } else if under(path, "/api/v1/wallet") {
        valid_wallet_key
    } else if [
        "/api/v1/subscriptions",
        "/api/v1/kyc",
        "/api/v1/identity",
        "/api/v1/marketplace",
        crate::client::PORTAL_PANEL_BASE,
    ]
    .iter()
    .any(|base| under(path, base))
    {
        valid_route_key
    } else {
        crate::client::valid_key
    }
}

pub(crate) fn data_of(response: &Response) -> &Value {
    response.data.as_ref().unwrap_or(&Value::Null)
}

/// `data` of an answer (`Value::Null` when it carried none).
pub(crate) fn unwrap(response: &Response) -> Value {
    data_of(response).clone()
}

pub(crate) fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Reads every list shape in use: `data` as the row array, `data.items`, or
/// the first array of `data` (wallet operations); the cursor in
/// `data.nextCursor` or `pageInfo.nextCursor` (the envelope's, else
/// `data.pageInfo`); `hasMore` from `pageInfo` (it needs a cursor), else
/// "a cursor is present".
pub(crate) fn to_page(response: &Response) -> CursorPage {
    let data = data_of(response);
    let items = match data {
        Value::Array(rows) => rows.clone(),
        Value::Object(map) => map
            .get("items")
            .and_then(Value::as_array)
            .or_else(|| map.values().find_map(Value::as_array))
            .cloned()
            .unwrap_or_default(),
        _ => vec![],
    };
    let info = response
        .page_info
        .as_ref()
        .filter(|info| info.is_object())
        .or_else(|| data.get("pageInfo").filter(|info| info.is_object()));
    let next_cursor =
        text(data.get("nextCursor")).or_else(|| text(info.and_then(|info| info.get("nextCursor"))));
    let has_more = match info.and_then(|info| info.get("hasMore")) {
        Some(Value::Bool(flag)) => *flag && next_cursor.is_some(),
        _ => next_cursor.is_some(),
    };
    let total_count = data
        .get("totalCount")
        .or_else(|| info.and_then(|info| info.get("totalCount")))
        .and_then(Value::as_i64)
        .filter(|n| (0..=crate::MAX_SAFE_INTEGER).contains(n));
    CursorPage {
        items,
        has_more,
        next_cursor,
        total_count,
        page_size: None,
        raw: data.clone(),
        request_id: response.request_id.clone(),
    }
}

/// A canonical minor-unit amount as text (`"12550"`); `positive` refuses `"0"`.
pub(crate) fn minor_text(value: &Value, positive: bool, message: &'static str) -> Result<String> {
    let text = value.as_str().ok_or(Error::Invalid(message))?;
    crate::money::parse_minor(text, positive, false).map_err(|_| Error::Invalid(message))?;
    Ok(text.to_owned())
}

/// A three-letter ISO 4217 code, trimmed and upper-cased.
pub(crate) fn require_currency(value: Option<&Value>, message: &'static str) -> Result<String> {
    let currency = value
        .and_then(Value::as_str)
        .map(|text| text.trim().to_ascii_uppercase())
        .unwrap_or_default();
    if currency.len() == 3 && currency.bytes().all(|b| b.is_ascii_uppercase()) {
        Ok(currency)
    } else {
        Err(Error::Invalid(message))
    }
}

/// A new `Idempotency-Key` (UUID v4).
pub(crate) fn new_idempotency_key() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The `data` object of a verified event whose type is one of `types`.
pub(crate) fn webhook_data<'e>(
    event: &'e crate::WebhookEvent,
    types: &[&str],
    message: &'static str,
) -> Result<&'e Map<String, Value>> {
    if !types.contains(&event.event_type()) {
        return Err(Error::Invalid(message));
    }
    event
        .data()
        .as_object()
        .ok_or(Error::Invalid("webhook event carries no data object"))
}

/// A non-empty string member (`None` for null, missing or `""`).
pub(crate) fn webhook_text(data: &Map<String, Value>, key: &str) -> Option<String> {
    text(data.get(key))
}

/// A minor-unit amount member (non-negative canonical text).
pub(crate) fn webhook_minor(
    data: &Map<String, Value>,
    key: &str,
    message: &'static str,
) -> Result<String> {
    minor_text(data.get(key).unwrap_or(&Value::Null), false, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn paths_are_filled_in_order_and_encoded() {
        let route = Route::new("x", "GET", "/api/v1/wallet/qr/:qrId/image");
        assert_eq!(
            fill_path(&route, &[" a/b "]).ok().as_deref(),
            Some("/api/v1/wallet/qr/a%2Fb/image")
        );
        assert!(fill_path(&route, &[""]).is_err());
        assert!(fill_path(&route, &[]).is_err());
        assert!(fill_path(&route, &["a", "b"]).is_err());
    }

    #[test]
    fn fields_and_filters_are_checked() {
        let fields = json!({"a": "1", "b": null});
        let object = fields.as_object().cloned().unwrap_or_default();
        assert!(check_fields(&object, &["a"], &[]).is_ok());
        assert!(check_fields(&object, &["a", "c"], &[]).is_err());
        assert!(check_fields(&object, &[], &[]).is_err());
        let pairs = query_pairs(&["limit", "flag"], &json!({"flag": true, "limit": 5}));
        assert_eq!(
            pairs.ok(),
            Some(vec![
                ("limit".to_owned(), "5".to_owned()),
                ("flag".to_owned(), "true".to_owned())
            ])
        );
        assert!(query_pairs(&["limit"], &json!({"other": 1})).is_err());
    }

    #[test]
    fn key_rules_follow_the_product() {
        assert!(key_rule("/api/v1/bank-connect/transfers")("12345678"));
        assert!(!key_rule("/api/v1/bank-connect/transfers")("1234567"));
        assert!(key_rule("/api/v1/wallet/end-users")("k"));
        assert!(!key_rule("/api/v1/wallet/end-users")(&"k".repeat(129)));
        assert!(!key_rule("/api/v1/payments/intents")("short"));
        assert!(key_rule("/api/v1/subscriptions/subscriptions")("k"));
        assert!(!key_rule("/api/v1/kyc/sessions")("a b"));
        assert!(key_rule("/api/panel/v1/subscriptions/portal/links")("k"));
    }
}
