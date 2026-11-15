//! Request plumbing of the subscription, KYC, identity and marketplace
//! clients. Each product describes its routes once in a table
//! ([`SUBSCRIPTION_ROUTES`](crate::SUBSCRIPTION_ROUTES),
//! [`KYC_ROUTES`](crate::KYC_ROUTES), [`IDENTITY_ROUTES`](crate::IDENTITY_ROUTES),
//! [`MARKETPLACE_ROUTES`](crate::MARKETPLACE_ROUTES)); a call is checked
//! against its route before anything is sent:
//!
//! - the `Idempotency-Key` rule (`required`, `optional`, `none`);
//! - the body: the route's field list (unknown or missing fields refused), a
//!   free object the gateway validates, or no body; an empty optional body
//!   is sent as none;
//! - the filters: the route's query list (or any scalar filter);
//! - money: on subscription and marketplace routes every money member of the
//!   body, the filters and the answer is canonical minor-unit text.
//!
//! Errors name the rule, never a value (card data and document images
//! included).

use crate::{
    Client, Error, Request, Response, Result,
    route_table::{
        CursorPage, Route, check_fields, data_of, fill_path, find, object_of, scalar_text, text,
    },
};
use serde_json::{Map, Value};

/// One call of a route.
#[derive(Default)]
pub(crate) struct RouteCall<'v> {
    /// The `:name` path segments, in order.
    pub(crate) params: &'v [&'v str],
    /// Filters (a JSON object; `None` for none).
    pub(crate) query: Option<&'v Value>,
    /// The body (`None` or `Value::Null` for none).
    pub(crate) body: Option<&'v Value>,
    /// The `Idempotency-Key`.
    pub(crate) key: Option<&'v str>,
}

/// Sends one route of `table` after checking the call against it.
pub(crate) async fn send_route(
    client: &Client,
    table: &'static [Route],
    name: &str,
    call: RouteCall<'_>,
) -> Result<Response> {
    let route = find(table, name)?;
    let path = fill_path(route, call.params)?;
    let key = route_key(route, call.key)?;
    let (filters, pairs) = match call.query {
        Some(query) => route_filters(route, query)?,
        None => (Map::new(), vec![]),
    };
    let body = route_body(route, call.body)?;
    let money = crate::money::wire_money_route(route.path);
    if money
        && (!body.as_ref().is_none_or(crate::money::wire_money_ok)
            || !crate::money::wire_money_ok(&Value::Object(filters)))
    {
        return Err(Error::Invalid(
            "money member must be canonical minor-unit text (\"9900\" = 99.00)",
        ));
    }
    let mut request = Request::new(route.http_method(), path);
    request.query = pairs;
    request.body = body;
    request.idempotency_key = key;
    let response = client.send(request).await?;
    if money
        && response
            .envelope
            .as_ref()
            .is_some_and(|envelope| !crate::money::wire_money_ok(envelope))
    {
        return Err(Error::Response(
            "money member of the answer is not canonical minor-unit text",
        ));
    }
    Ok(response)
}

fn route_key(route: &Route, key: Option<&str>) -> Result<Option<String>> {
    match key.map(str::trim) {
        None if route.idempotency_key => Err(Error::Invalid(
            "a stable Idempotency-Key is required for this route; reuse it when retrying the same operation",
        )),
        None => Ok(None),
        Some(_) if !route.idempotency_key && !route.idempotency_key_optional => Err(
            Error::Invalid("this route does not take an Idempotency-Key"),
        ),
        Some("") => Err(Error::Invalid("Idempotency-Key must be a non-empty string")),
        Some(key) => Ok(Some(key.to_owned())),
    }
}

/// The given filters without nulls (for the money check) and their query
/// pairs.
type Filters = (Map<String, Value>, Vec<(String, String)>);

/// Filters: nulls dropped, unknown ones refused unless the route takes any,
/// scalars only; `""` is left out and booleans are sent as `true` / `false`.
fn route_filters(route: &Route, filters: &Value) -> Result<Filters> {
    if filters.is_null() {
        return Ok((Map::new(), vec![]));
    }
    let given: Map<String, Value> = object_of(filters, "filters must be a JSON object")?
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if !route.any_query && given.keys().any(|key| !route.query.contains(&key.as_str())) {
        return Err(Error::Invalid(
            "filter not accepted by this route (the gateway rejects unknown query fields)",
        ));
    }
    let mut pairs = vec![];
    for (key, value) in &given {
        if let Some(text) = scalar_text(value)? {
            pairs.push((key.clone(), text));
        }
    }
    Ok((given, pairs))
}

fn route_body(route: &Route, body: Option<&Value>) -> Result<Option<Value>> {
    let given = body.filter(|body| !body.is_null());
    let empty = Value::Object(Map::new());
    if !route.required.is_empty() || !route.optional.is_empty() {
        let object = object_of(
            given.unwrap_or(&empty),
            "request body must be a JSON object",
        )?;
        let plain = check_fields(object, route.required, route.optional)?;
        return Ok((!(route.optional_body && plain.is_empty())).then_some(Value::Object(plain)));
    }
    if route.free_body {
        let plain = compact(object_of(
            given.unwrap_or(&empty),
            "request body must be a JSON object",
        )?);
        return Ok((!(route.optional_body && plain.is_empty())).then_some(Value::Object(plain)));
    }
    if given.is_some() {
        return Err(Error::Invalid("this route takes no body"));
    }
    Ok(None)
}

/// The object without its null members.
pub(crate) fn compact(object: &Map<String, Value>) -> Map<String, Value> {
    object
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// A JSON object (`Value::Null` reads as `{}`), copied.
pub(crate) fn object_or_empty(value: &Value, message: &'static str) -> Result<Map<String, Value>> {
    if value.is_null() {
        Ok(Map::new())
    } else {
        Ok(object_of(value, message)?.clone())
    }
}

/// The request's own `idempotencyKey` or the argument; both given, they
/// must agree.
pub(crate) fn idempotency_key_of(
    request: &Map<String, Value>,
    given: Option<&str>,
) -> Result<Option<String>> {
    let own = request
        .get("idempotencyKey")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty());
    let given = given.map(str::trim).filter(|key| !key.is_empty());
    match (own, given) {
        (Some(own), Some(given)) if own != given => Err(Error::Invalid(
            "the request's idempotencyKey and the idempotency_key argument differ",
        )),
        (own, given) => Ok(given.or(own).map(str::to_owned)),
    }
}

/// A trimmed, non-empty text.
pub(crate) fn require_text(value: &str, message: &'static str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        Err(Error::Invalid(message))
    } else {
        Ok(value.to_owned())
    }
}

/// `Some(text)` for a non-empty argument, `None` for `None` or `""`.
pub(crate) fn optional_text(value: Option<&str>) -> Option<Value> {
    value
        .filter(|text| !text.is_empty())
        .map(|text| Value::String(text.to_owned()))
}

/// The member is absent / null, or one of `allowed`.
pub(crate) fn check_member(
    object: &Map<String, Value>,
    field: &str,
    allowed: &[&str],
    message: &'static str,
) -> Result<()> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(()),
        Some(value) if value.as_str().is_some_and(|text| allowed.contains(&text)) => Ok(()),
        Some(_) => Err(Error::Invalid(message)),
    }
}

/// The member is absent / null, or a boolean.
pub(crate) fn boolean_member(
    object: &Map<String, Value>,
    field: &str,
    message: &'static str,
) -> Result<()> {
    match object.get(field) {
        None | Some(Value::Null | Value::Bool(_)) => Ok(()),
        Some(_) => Err(Error::Invalid(message)),
    }
}

/// The member is absent / null, or a JSON object.
pub(crate) fn object_member(
    object: &Map<String, Value>,
    field: &str,
    message: &'static str,
) -> Result<()> {
    match object.get(field) {
        None | Some(Value::Null | Value::Object(_)) => Ok(()),
        Some(_) => Err(Error::Invalid(message)),
    }
}

/// The member is absent / null, or an integer of at least `min`.
pub(crate) fn count_member(
    object: &Map<String, Value>,
    field: &str,
    min: i64,
    message: &'static str,
) -> Result<()> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(()),
        Some(value) if value.as_i64().is_some_and(|n| n >= min) => Ok(()),
        Some(_) => Err(Error::Invalid(message)),
    }
}

/// An array of integers from `min` to `max`.
pub(crate) fn integer_list(value: &Value, min: i64, max: i64, message: &'static str) -> Result<()> {
    let valid = value.as_array().is_some_and(|list| {
        list.iter()
            .all(|entry| entry.as_i64().is_some_and(|n| (min..=max).contains(&n)))
    });
    if valid {
        Ok(())
    } else {
        Err(Error::Invalid(message))
    }
}

/// `data` (`Value::Null` when the answer carried none).
pub(crate) fn data(response: &Response) -> Value {
    data_of(response).clone()
}

/// Answers that put their fields at the envelope root and / or under
/// `data`: both, merged (`data` wins).
pub(crate) fn merge_root(response: &Response) -> Value {
    let mut out = Map::new();
    if let Some(Value::Object(root)) = &response.envelope {
        for (key, value) in root {
            if key != "data" {
                out.insert(key.clone(), value.clone());
            }
        }
    }
    if let Some(Value::Object(data)) = &response.data {
        for (key, value) in data {
            out.insert(key.clone(), value.clone());
        }
    }
    Value::Object(out)
}

fn object(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value.and_then(Value::as_object)
}

fn count(value: Option<&Value>) -> Option<i64> {
    value
        .and_then(Value::as_i64)
        .filter(|n| (0..=crate::MAX_SAFE_INTEGER).contains(n))
}

/// A list answer. Reads every list shape in use: `data` as the row array,
/// `data.items` / `operations` / `rows`; the cursor in `pageInfo`
/// (`nextCursor` / `endCursor`; a `pageInfo` naming one without a value is
/// the last page), else `nextCursor` / `nextPageToken` in `data` or at the
/// envelope root. `has_more` is `pageInfo.hasMore` when given, else "a
/// cursor is present". The cursor is opaque: send it back unchanged.
pub(crate) fn list_page(response: &Response) -> CursorPage {
    let data = data_of(response);
    let container = data.as_object();
    let root = object(response.envelope.as_ref());
    let items = match data {
        Value::Array(rows) => rows.clone(),
        Value::Object(map) => ["items", "operations", "rows"]
            .iter()
            .find_map(|key| map.get(*key).and_then(Value::as_array))
            .cloned()
            .unwrap_or_default(),
        _ => vec![],
    }
    .into_iter()
    .filter(Value::is_object)
    .collect();
    let info = object(container.and_then(|map| map.get("pageInfo")))
        .or_else(|| object(root.and_then(|map| map.get("pageInfo"))));
    let info_cursor =
        info.and_then(|info| text(info.get("nextCursor")).or_else(|| text(info.get("endCursor"))));
    let info_names_cursor =
        info.is_some_and(|info| info.contains_key("nextCursor") || info.contains_key("endCursor"));
    let next_cursor = info_cursor.or_else(|| {
        if info_names_cursor {
            return None;
        }
        [
            container.and_then(|map| map.get("nextCursor")),
            container.and_then(|map| map.get("nextPageToken")),
            root.and_then(|map| map.get("nextPageToken")),
            root.and_then(|map| map.get("nextCursor")),
        ]
        .into_iter()
        .find_map(text)
    });
    let has_more = match info.and_then(|info| info.get("hasMore")) {
        Some(Value::Bool(flag)) => *flag,
        _ => next_cursor.is_some(),
    };
    let total_count = count(info.and_then(|info| info.get("totalCount")))
        .or_else(|| count(container.and_then(|map| map.get("totalCount"))))
        .or_else(|| count(container.and_then(|map| map.get("total"))));
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn response(envelope: Value) -> Response {
        Response {
            status: 200,
            request_id: None,
            data: Some(envelope.get("data").cloned().unwrap_or(envelope.clone())),
            page_info: envelope.get("pageInfo").cloned(),
            envelope: Some(envelope),
            bytes: vec![],
            content_type: None,
        }
    }

    #[test]
    fn list_shapes_are_read() {
        let page = list_page(&response(
            json!({"data": [{"id": "a"}, 1], "nextPageToken": "t2", "pageSize": 1}),
        ));
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.next_cursor.as_deref(), Some("t2"));
        assert!(page.has_more);
        let page = list_page(&response(json!({
            "data": {"items": [{"id": "a"}], "nextCursor": "c", "pageInfo": {"hasMore": false, "nextCursor": null}}
        })));
        assert_eq!(page.next_cursor, None);
        assert!(!page.has_more);
        let page = list_page(&response(
            json!({"data": {"rows": [{"id": "a"}], "total": 7}}),
        ));
        assert_eq!(page.total_count, Some(7));
        assert!(!page.has_more);
    }

    #[test]
    fn keys_follow_the_route_rule() {
        let required = Route::new("a", "POST", "/api/v1/kyc/x").with_key();
        let optional = Route::new("b", "POST", "/api/v1/kyc/x").with_optional_key();
        let none = Route::new("c", "POST", "/api/v1/kyc/x");
        assert!(route_key(&required, None).is_err());
        assert_eq!(
            route_key(&required, Some(" k ")).ok().flatten().as_deref(),
            Some("k")
        );
        assert!(route_key(&optional, None).is_ok_and(|key| key.is_none()));
        assert!(route_key(&optional, Some(" ")).is_err());
        assert!(route_key(&none, Some("k")).is_err());
    }

    #[test]
    fn bodies_follow_the_route() {
        let fields = Route::new("a", "POST", "/x")
            .with_body(&[], &["reason"])
            .with_optional_body();
        assert_eq!(route_body(&fields, None).ok(), Some(None));
        assert!(route_body(&fields, Some(&json!({"other": 1}))).is_err());
        let free = Route::new("b", "PATCH", "/x").with_free_body();
        assert_eq!(
            route_body(&free, Some(&json!({"a": 1, "b": null}))).ok(),
            Some(Some(json!({"a": 1})))
        );
        let bare = Route::new("c", "POST", "/x");
        assert!(route_body(&bare, Some(&json!({}))).is_err());
        assert_eq!(route_body(&bare, Some(&Value::Null)).ok(), Some(None));
    }
}
