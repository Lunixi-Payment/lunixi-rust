//! Request checks shared by the physical POS facades: field lists, card data
//! (field names and card-number values), filters, `Idempotency-Key`,
//! paging, the payment-write attempt loop and error mapping.

use super::{
    BASE,
    card_data::{physical_pos_contains_pan_like_value, physical_pos_find_card_data_field},
    money::validate_response_money,
    results::{PhysicalPosPage, PhysicalPosWriteResult},
    values::{
        PHYSICAL_POS_FREE_TEXT_FILTERS, PHYSICAL_POS_MAX_LIMIT, PHYSICAL_POS_NOT_STARTED_CODES,
        PHYSICAL_POS_PAYMENT_TARGET_FIELDS, PHYSICAL_POS_ROW_VERSION_MAX, physical_pos_filters,
        physical_pos_request_fields,
    },
};
use crate::{
    Client, Error, Method, Request, Response, Result,
    client::{Exchange, JSON_ACCEPT, api_error, error_code},
    money::parse_minor,
};
use serde_json::{Map, Value};
use std::time::Duration;

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

const SALE_ITEM_KINDS: &[(&str, &str)] = &[
    ("lineNo", "int*"),
    ("name", "string*"),
    ("quantityMilli", "int*"),
    ("unitCode", "string*"),
    ("unitPriceMinor", "money*"),
    ("taxGroupCode", "string*"),
    ("lineTotalMinor", "money*"),
    ("barcode", "string"),
];
const REFUND_ITEM_KINDS: &[(&str, &str)] = &[
    ("lineNo", "int*"),
    ("quantityMilli", "int*"),
    ("amountMinor", "money"),
];

// ── Card data ───────────────────────────────────────────────────────────────

pub(crate) fn card_data_rejected(field: impl Into<String>) -> Error {
    Error::PhysicalPosCardDataRejected {
        field: Some(field.into()),
        status: 0,
        request_id: None,
    }
}

fn reject_card_data(value: &Value, root: &str) -> Result<()> {
    match physical_pos_find_card_data_field(value, root) {
        Some(field) => Err(card_data_rejected(field)),
        None => Ok(()),
    }
}

/// Card-number values in the free-text fields the gateway scans (the value
/// never reaches the error).
fn reject_card_numbers(body: &Map<String, Value>) -> Result<()> {
    if body
        .get("merchantReference")
        .and_then(Value::as_str)
        .is_some_and(|text| physical_pos_contains_pan_like_value(text, false))
    {
        return Err(card_data_rejected("merchantReference"));
    }
    let items = body.get("items").and_then(Value::as_array);
    for (index, item) in items.into_iter().flatten().enumerate() {
        let text = |name: &str| item.get(name).and_then(Value::as_str);
        if text("name").is_some_and(|t| physical_pos_contains_pan_like_value(t, true)) {
            return Err(card_data_rejected(format!("items[{index}].name")));
        }
        if text("taxGroupCode").is_some_and(|t| physical_pos_contains_pan_like_value(t, false)) {
            return Err(card_data_rejected(format!("items[{index}].taxGroupCode")));
        }
    }
    Ok(())
}

// ── Bodies ──────────────────────────────────────────────────────────────────

fn money_text(value: &Value, positive: bool) -> Result<Value> {
    let text = value.as_str().ok_or(Error::Invalid(
        "physical POS amount: canonical minor-unit string",
    ))?;
    parse_minor(text, positive, false)?;
    Ok(Value::String(text.to_owned()))
}

fn safe_integer(value: &Value) -> Result<i64> {
    value
        .as_i64()
        .filter(|n| (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(n))
        .ok_or(Error::Invalid(
            "physical POS integer field (a JSON number, not a string)",
        ))
}

pub(crate) fn row_version(value: i64) -> Result<i64> {
    if (1..=PHYSICAL_POS_ROW_VERSION_MAX).contains(&value) {
        Ok(value)
    } else {
        Err(Error::Invalid(
            "physical POS rowVersion: 1-2147483647, the rowVersion you last read",
        ))
    }
}

fn object_of(
    value: &Value,
    allowed: &[&str],
    check: impl Fn(&str, &Value) -> Result<Option<Value>>,
) -> Result<Map<String, Value>> {
    let object = value.as_object().ok_or(Error::Invalid(
        "physical POS nested field must be an object",
    ))?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(Error::Invalid(
            "unknown physical POS request field (the gateway rejects unknown fields with 400)",
        ));
    }
    let mut out = Map::new();
    for key in allowed {
        if let Some(entry) = object.get(*key)
            && let Some(checked) = check(key, entry)?
        {
            out.insert((*key).to_owned(), checked);
        }
    }
    Ok(out)
}

fn kind_of(kinds: &[(&str, &'static str)], key: &str) -> &'static str {
    kinds
        .iter()
        .find(|(name, _)| *name == key)
        .map_or("string", |(_, kind)| *kind)
}

fn items_of(value: &Value, kinds: &[(&str, &'static str)]) -> Result<Value> {
    let list = value
        .as_array()
        .filter(|list| !list.is_empty())
        .ok_or(Error::Invalid(
            "physical POS items must be a non-empty array (leave the field out for none)",
        ))?;
    let allowed: Vec<&str> = kinds.iter().map(|(name, _)| *name).collect();
    let mut out = Vec::with_capacity(list.len());
    for item in list {
        let checked = object_of(item, &allowed, |key, entry| {
            check_value(kind_of(kinds, key), entry)
        })?;
        if kinds
            .iter()
            .any(|(name, kind)| kind.ends_with('*') && !checked.contains_key(*name))
        {
            return Err(Error::Invalid("required physical POS item field"));
        }
        out.push(Value::Object(checked));
    }
    Ok(Value::Array(out))
}

/// Checks one value against its kind (`PHYSICAL_POS_REQUEST_FIELDS`). Money
/// is canonical text only; a present null or blank amount is refused.
/// `None` leaves the field out. `null` is sent only for a nullable (`?`)
/// field — an explicit clear — and left out for any other optional field;
/// `""` is left out for an optional string and refused for a required one.
fn check_value(kind_spec: &str, value: &Value) -> Result<Option<Value>> {
    let required = kind_spec.ends_with('*');
    let nullable = kind_spec.ends_with('?');
    let kind = kind_spec.trim_end_matches(['*', '?']);
    if kind == "money" {
        return money_text(value, true).map(Some);
    }
    if value.is_null() {
        return if nullable {
            Ok(Some(Value::Null))
        } else if required {
            Err(Error::Invalid("required physical POS request field"))
        } else {
            Ok(None)
        };
    }
    if value.as_str() == Some("") && !required {
        return Ok(None);
    }
    let checked = match kind {
        "string" => {
            value
                .as_str()
                .filter(|text| !text.is_empty())
                .ok_or(Error::Invalid(
                    "physical POS field must be a non-empty string",
                ))?;
            value.clone()
        }
        "int" => Value::from(safe_integer(value)?),
        "rowVersion" => Value::from(row_version(safe_integer(value)?)?),
        "bool" => Value::Bool(
            value
                .as_bool()
                .ok_or(Error::Invalid("physical POS field must be a boolean"))?,
        ),
        "strings" => {
            let list = value
                .as_array()
                .filter(|list| {
                    list.iter()
                        .all(|e| e.as_str().is_some_and(|s| !s.is_empty()))
                })
                .ok_or(Error::Invalid(
                    "physical POS field must be an array of non-empty strings",
                ))?;
            Value::Array(list.clone())
        }
        "target" => {
            let target = object_of(value, PHYSICAL_POS_PAYMENT_TARGET_FIELDS, |_, entry| {
                check_value("string", entry)
            })?;
            Value::Object(target)
        }
        "saleItems" => items_of(value, SALE_ITEM_KINDS)?,
        "refundItems" => items_of(value, REFUND_ITEM_KINDS)?,
        _ => return Err(Error::Invalid("physical POS field kind")),
    };
    Ok(Some(checked))
}

/// Builds a request body from its field list: card-data field names first
/// (`Error::PhysicalPosCardDataRejected`), then unknown fields, required
/// fields and value types (`Error::Invalid`), then card-number values in the
/// scanned free-text fields. Nothing is sent when a check fails.
pub(crate) fn build_body(request: &Value, operation: &str) -> Result<Value> {
    let fields =
        physical_pos_request_fields(operation).ok_or(Error::Invalid("physical POS operation"))?;
    if !request.is_null() {
        reject_card_data(request, "(body)")?;
    }
    let object = request
        .as_object()
        .ok_or(Error::Invalid("physical POS request must be a JSON object"))?;
    if object
        .keys()
        .any(|key| !fields.iter().any(|(name, _)| name == key))
    {
        return Err(Error::Invalid(
            "unknown physical POS request field (the gateway rejects unknown fields with 400)",
        ));
    }
    let mut out = Map::new();
    for (key, kind) in fields {
        match object.get(*key) {
            None if kind.ends_with('*') => {
                return Err(Error::Invalid("required physical POS request field"));
            }
            None => {}
            Some(value) => {
                if let Some(checked) = check_value(kind, value)? {
                    out.insert((*key).to_owned(), checked);
                }
            }
        }
    }
    reject_card_numbers(&out)?;
    Ok(Value::Object(out))
}

// ── Filters ─────────────────────────────────────────────────────────────────

fn filter_value(key: &str, value: &Value) -> Result<String> {
    match key {
        "limit" => value
            .as_i64()
            .filter(|n| (1..=PHYSICAL_POS_MAX_LIMIT).contains(n))
            .map(|n| n.to_string())
            .ok_or(Error::Invalid(
                "physical POS limit: an integer from 1 to 100",
            )),
        "includeTotal" => value.as_bool().map(|b| b.to_string()).ok_or(Error::Invalid(
            "physical POS includeTotal must be a boolean",
        )),
        _ => value
            .as_str()
            .map(str::to_owned)
            .ok_or(Error::Invalid("physical POS filter must be a string")),
    }
}

/// Query parameters of one route from a JSON object (`Value::Null` for
/// none): card-data names, unknown filters, value types and card numbers in
/// `merchantReference` / `q` are refused before sending. `null` and `""`
/// leave a filter out.
pub(crate) fn build_query(filters: &Value, route: &str) -> Result<Vec<(String, String)>> {
    if filters.is_null() {
        return Ok(vec![]);
    }
    let allowed = physical_pos_filters(route).ok_or(Error::Invalid("physical POS route"))?;
    reject_card_data(filters, "(query)")?;
    let object = filters
        .as_object()
        .ok_or(Error::Invalid("physical POS filters must be a JSON object"))?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(Error::Invalid(
            "unknown physical POS filter (the gateway rejects unknown query fields with 400)",
        ));
    }
    let mut query = vec![];
    for key in allowed {
        let Some(value) = object.get(*key) else {
            continue;
        };
        if *key == "amountFromMinor" || *key == "amountToMinor" {
            if let Value::String(text) = money_text(value, true)? {
                query.push(((*key).to_owned(), text));
            }
            continue;
        }
        if value.is_null() || value.as_str() == Some("") {
            continue;
        }
        let text = filter_value(key, value)?;
        if PHYSICAL_POS_FREE_TEXT_FILTERS.contains(key)
            && physical_pos_contains_pan_like_value(&text, false)
        {
            return Err(card_data_rejected(*key));
        }
        query.push(((*key).to_owned(), text));
    }
    Ok(query)
}

// ── Identifiers, Idempotency-Key ────────────────────────────────────────────

/// A path segment: non-empty, not `.` / `..`, percent-encoded (`a/b` →
/// `a%2Fb`; only unreserved characters stay as they are).
pub(crate) fn path_id(value: &str) -> Result<String> {
    if value.is_empty() || value == "." || value == ".." || value.len() > 512 {
        return Err(Error::Invalid("physical POS identifier"));
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

/// `Idempotency-Key`: 8–200 printable ASCII characters without spaces, sent
/// exactly as given (never trimmed).
pub(crate) fn valid_idempotency_key(key: &str) -> bool {
    key.len() >= super::values::PHYSICAL_POS_IDEMPOTENCY_KEY_MIN
        && crate::client::safe_header(key, super::values::PHYSICAL_POS_IDEMPOTENCY_KEY_MAX)
}

/// A new `Idempotency-Key` (UUID v4).
pub(crate) fn new_idempotency_key() -> String {
    uuid::Uuid::new_v4().to_string()
}

const KEY_RULE: &str = "physical POS Idempotency-Key: 8-200 printable ASCII characters without spaces, sent exactly as given";

/// The caller's key (checked, never trimmed), or a new UUID for `None`.
pub(crate) fn key_of(key: Option<&str>) -> Result<String> {
    match key {
        None => Ok(new_idempotency_key()),
        Some(key) => require_key(key),
    }
}

/// Payment start and reversal: the caller passes the key it stored.
pub(crate) fn require_key(key: &str) -> Result<String> {
    if valid_idempotency_key(key) {
        Ok(key.to_owned())
    } else {
        Err(Error::Invalid(KEY_RULE))
    }
}

// ── Responses ───────────────────────────────────────────────────────────────

fn data_of(response: &Response) -> &Value {
    response.data.as_ref().unwrap_or(&Value::Null)
}

/// `data.<name>` when present, else `data`.
pub(crate) fn record_of(response: &Response, name: &str) -> Value {
    let data = data_of(response);
    match data.get(name) {
        Some(record) if !record.is_null() => record.clone(),
        _ => data.clone(),
    }
}

/// `data` of an answer (`Value::Null` when it carried none).
pub(crate) fn unwrap(response: &Response) -> Value {
    data_of(response).clone()
}

/// Paging from the envelope's top-level `pageInfo`.
pub(crate) fn to_page(response: &Response) -> PhysicalPosPage {
    let data = data_of(response);
    let items = data
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let info = response
        .page_info
        .as_ref()
        .filter(|info| info.is_object())
        .or_else(|| data.get("pageInfo"))
        .cloned()
        .unwrap_or(Value::Null);
    let next_cursor = info
        .get("nextCursor")
        .and_then(Value::as_str)
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_owned);
    PhysicalPosPage {
        items,
        has_more: info.get("hasMore") == Some(&Value::Bool(true)) && next_cursor.is_some(),
        next_cursor,
        total_count: info
            .get("totalCount")
            .and_then(Value::as_i64)
            .filter(|n| (0..=MAX_SAFE_INTEGER).contains(n)),
        request_id: response.request_id.clone(),
    }
}

/// A keyed write: `data`, `replayed`, the status and the key sent.
pub(crate) fn write_result(response: &Response, idempotency_key: String) -> PhysicalPosWriteResult {
    let data = data_of(response).as_object().cloned().unwrap_or_default();
    let status_code = response
        .envelope
        .as_ref()
        .and_then(|envelope| envelope.get("statusCode"))
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok())
        .unwrap_or(response.status);
    PhysicalPosWriteResult {
        replayed: data.get("replayed") == Some(&Value::Bool(true)),
        data,
        status_code,
        idempotency_key,
        request_id: response.request_id.clone(),
    }
}

fn details(response: &Response) -> Option<&Map<String, Value>> {
    response
        .envelope
        .as_ref()
        .and_then(|envelope| envelope.get("details"))
        .and_then(Value::as_object)
}

fn detail_text(response: &Response, name: &str) -> Option<String> {
    details(response)
        .and_then(|d| d.get(name))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// `Retry-After`, else `details.retryAfterSeconds` (a number or numeric text).
fn retry_after(answer: &Exchange) -> Option<u64> {
    answer.retry_after.or_else(|| {
        let value = details(&answer.response)?.get("retryAfterSeconds")?;
        let seconds = match value {
            Value::String(text) => text.trim().parse::<f64>().ok()?,
            other => other.as_f64()?,
        };
        (seconds.is_finite() && seconds >= 0.0).then(|| seconds.ceil() as u64)
    })
}

/// Typed errors of one answered request: card data → field path, void
/// after end-of-day → `allowed_operation`, 429 → `retry_after`.
fn answer_error(answer: &Exchange) -> Error {
    let response = &answer.response;
    let code = error_code(response);
    match code.as_deref() {
        Some(Error::PHYSICAL_POS_CARD_DATA_REJECTED_CODE) => Error::PhysicalPosCardDataRejected {
            field: detail_text(response, "field"),
            status: response.status,
            request_id: response.request_id.clone(),
        },
        Some(Error::PHYSICAL_POS_VOID_AFTER_SETTLEMENT_CODE) => {
            Error::PhysicalPosVoidAfterSettlement {
                allowed_operation: detail_text(response, "allowedOperation"),
                request_id: response.request_id.clone(),
            }
        }
        _ if response.status == 429 => Error::PhysicalPosRateLimit {
            code: code.unwrap_or_else(|| "RATE_LIMITED".to_owned()),
            request_id: response.request_id.clone(),
            retry_after: retry_after(answer),
        },
        _ => api_error(response),
    }
}

fn request(method: Method, path: &str, body: Option<Value>, key: Option<&str>) -> Request {
    let mut request = Request::new(method, format!("{BASE}{path}"));
    request.body = body;
    request.idempotency_key = key.map(str::to_owned);
    request
}

/// Sends one signed physical POS request (reads and management writes) with
/// the client's retries, checks the answer's money fields and maps errors.
pub(crate) async fn send(
    client: &Client,
    method: Method,
    path: &str,
    body: Option<Value>,
    query: Vec<(String, String)>,
    key: Option<&str>,
) -> Result<Response> {
    let mut request = request(method, path, body, key);
    request.query = query;
    let answer = client.dispatch(request, JSON_ACCEPT, true).await?;
    if !(200..300).contains(&answer.response.status) {
        return Err(answer_error(&answer));
    }
    if let Some(envelope) = &answer.response.envelope {
        validate_response_money(envelope, false)?;
    }
    Ok(answer.response)
}

/// A keyed management or end-of-day write.
pub(crate) async fn keyed_write(
    client: &Client,
    path: &str,
    body: Value,
    key: Option<&str>,
) -> Result<PhysicalPosWriteResult> {
    let key = key_of(key)?;
    let response = send(client, Method::POST, path, Some(body), vec![], Some(&key)).await?;
    Ok(write_result(&response, key))
}

// ── Payment writes: start, reversal, cancel ─────────────────────────────────

fn has_payment(response: &Response) -> bool {
    if response
        .envelope
        .as_ref()
        .is_some_and(|envelope| validate_response_money(envelope, false).is_err())
    {
        return false;
    }
    data_of(response)
        .get("payment")
        .and_then(|payment| payment.get("paymentId"))
        .and_then(Value::as_str)
        .is_some_and(|id| !id.is_empty())
}

fn outcome_unknown(last: Option<&Exchange>, idempotency_key: String, attempts: u32) -> Error {
    Error::PhysicalPosOutcomeUnknown {
        idempotency_key,
        attempts,
        status: last.map_or(0, |answer| answer.response.status),
        code: last.and_then(|answer| error_code(&answer.response)),
        request_id: last.and_then(|answer| answer.response.request_id.clone()),
        retry_after: last.and_then(retry_after),
    }
}

async fn backoff(attempt: u32) {
    let delay = (100u64 << (attempt - 1).min(5)).min(2000);
    tokio::time::sleep(Duration::from_millis(delay)).await;
}

/// Sends a payment write with its own attempts (the transport's retries are
/// off for it): 1 + `max_retries` attempts, the same key and body every time.
///
/// Success is only a 2xx carrying a payment. A 2xx without a readable
/// payment, no HTTP answer, or a 5xx that does not say "not started" leaves
/// the outcome unknown; it is retried while attempts remain. A 4xx stops at
/// once. When any attempt was ambiguous the call fails with
/// `Error::PhysicalPosOutcomeUnknown` whatever the last answer was (a later
/// 429 or 409 included), because the key may already hold a payment.
pub(crate) async fn payment_write(
    client: &Client,
    path: &str,
    body: Value,
    idempotency_key: String,
) -> Result<PhysicalPosWriteResult> {
    let attempts = 1 + u32::from(client.max_retries());
    let mut ambiguous = false;
    let mut last: Option<Exchange> = None;
    let mut made = 0;
    for attempt in 1..=attempts {
        made = attempt;
        let sent = request(
            Method::POST,
            path,
            Some(body.clone()),
            Some(&idempotency_key),
        );
        match client.dispatch(sent, JSON_ACCEPT, false).await {
            Err(Error::Transport | Error::Response(_)) => {
                ambiguous = true;
                last = None;
            }
            Err(local) => {
                if ambiguous {
                    return Err(outcome_unknown(last.as_ref(), idempotency_key, made));
                }
                return Err(local);
            }
            Ok(answer) if (200..300).contains(&answer.response.status) => {
                if has_payment(&answer.response) {
                    return Ok(write_result(&answer.response, idempotency_key));
                }
                ambiguous = true;
                last = Some(answer);
            }
            Ok(answer) => {
                let status = answer.response.status;
                let not_started = error_code(&answer.response)
                    .is_some_and(|code| PHYSICAL_POS_NOT_STARTED_CODES.contains(&code.as_str()));
                last = Some(answer);
                if status < 500 {
                    break;
                }
                if !not_started {
                    ambiguous = true;
                }
            }
        }
        if attempt < attempts {
            backoff(attempt).await;
        }
    }
    match last {
        _ if ambiguous => Err(outcome_unknown(last.as_ref(), idempotency_key, made)),
        Some(answer) => Err(answer_error(&answer)),
        None => Err(Error::Transport),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical_pos::values::{
        PHYSICAL_POS_REFUND_ITEM_FIELDS, PHYSICAL_POS_SALE_ITEM_FIELDS,
    };

    #[test]
    fn item_kinds_follow_the_public_field_lists() {
        fn names(kinds: &[(&'static str, &'static str)]) -> Vec<&'static str> {
            kinds.iter().map(|(name, _)| *name).collect()
        }
        assert_eq!(names(SALE_ITEM_KINDS), PHYSICAL_POS_SALE_ITEM_FIELDS);
        assert_eq!(names(REFUND_ITEM_KINDS), PHYSICAL_POS_REFUND_ITEM_FIELDS);
    }

    #[test]
    fn path_ids_are_encoded() {
        assert_eq!(path_id("a/b").ok().as_deref(), Some("a%2Fb"));
        assert!(path_id("..").is_err());
        assert!(path_id("").is_err());
    }
}
