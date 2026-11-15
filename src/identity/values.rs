//! Identity (device and account-linking signals) values and the route
//! table (`/api/v1/identity`).

use crate::route_table::Route;
use serde_json::Value;

/// Body of `analyze` / `analyze_sync`; the nested objects are forwarded as given.
pub const IDENTITY_ANALYZE_FIELDS: &[&str] = &[
    "subjectType",
    "subjectId",
    "productContext",
    "integrationMode",
    "customer",
    "device",
    "session",
    "source",
    "account",
    "metadata",
    "paymentId",
];

/// Every identity route the SDK calls (bearer-only on the gateway). The
/// browser fingerprint SDK uses the publishable key, which is not here.
pub const IDENTITY_ROUTES: &[Route] = &[
    Route::new(
        "analyzeSync",
        "POST",
        "/api/v1/identity/live-sessions/analyze-sync",
    )
    .with_body(&[], IDENTITY_ANALYZE_FIELDS),
    Route::new("analyze", "POST", "/api/v1/identity/live-sessions/analyze")
        .with_body(&[], IDENTITY_ANALYZE_FIELDS),
    Route::new(
        "accountLinks",
        "GET",
        "/api/v1/identity/live-sessions/:traceId/account-links",
    ),
    Route::new(
        "getSession",
        "GET",
        "/api/v1/identity/live-sessions/:traceId",
    ),
    Route::new(
        "relatedSessions",
        "GET",
        "/api/v1/identity/live-sessions/:traceId/related-sessions",
    )
    .with_query(&["limit"]),
    Route::new(
        "timeline",
        "GET",
        "/api/v1/identity/live-sessions/:traceId/timeline",
    ),
    Route::new(
        "topSources",
        "GET",
        "/api/v1/identity/live-sessions/top-sources",
    )
    .with_query(&["days", "limit"]),
    Route::new("listSessions", "GET", "/api/v1/identity/live-sessions").with_query(&[
        "fingerprintId",
        "deviceId",
        "paymentId",
        "limit",
    ]),
];

/// `true` when the device of an `account_links` answer is shared by at least
/// `threshold` accounts, counting the current one (`linkedAccounts` excludes
/// it). With 2 (the smallest threshold), a second account on the device
/// trips it.
pub fn is_identity_device_shared(account_links: &Value, threshold: usize) -> bool {
    let linked = account_links
        .get("linkedAccounts")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    linked + 1 >= threshold.max(2)
}

/// The distinct `linkReasons` of an `account_links` answer's sessions
/// (`same_device_cluster`, `same_fingerprint` …), in first-seen order.
pub fn identity_link_reasons(account_links: &Value) -> Vec<String> {
    let mut reasons: Vec<String> = vec![];
    let sessions = account_links
        .get("sessions")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    for session in sessions {
        let Some(list) = session.get("linkReasons").and_then(Value::as_array) else {
            continue;
        };
        for reason in list {
            let text = match reason {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            if !reasons.contains(&text) {
                reasons.push(text);
            }
        }
    }
    reasons
}
