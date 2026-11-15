//! KYC: session bodies, keys in header and body, the two-filter list and
//! the admin routes against a local TLS gateway double.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
mod common;
use common::{Incoming, Reply, recording_gateway};
use lunixi::{Error, KYC_ROUTES, is_kyc_session_terminal, kyc_assurance_meets, kyc_assurance_rank};
use serde_json::{Value, json};

fn ok(_: usize, _: &Incoming) -> Reply {
    Reply::json(
        200,
        r#"{"status":"success","data":{"session":{"id":"ks_1","status":"CREATED"}}}"#,
    )
}

#[test]
fn route_table_and_helpers() {
    assert_eq!(KYC_ROUTES.len(), 16);
    assert!(
        KYC_ROUTES
            .iter()
            .all(|route| !route.step_up && route.path.starts_with("/api/v1/kyc/"))
    );
    assert!(is_kyc_session_terminal("approved"));
    assert!(!is_kyc_session_terminal("PROCESSING"));
    assert_eq!(kyc_assurance_rank("substantial"), 2);
    assert!(kyc_assurance_meets("HIGH", "SUBSTANTIAL"));
    assert!(!kyc_assurance_meets("LOW", "SUBSTANTIAL"));
    assert!(kyc_assurance_meets("NONE", "NOT_PERFORMED"));
}

#[tokio::test]
async fn create_session_maps_person_and_metadata_and_sends_the_key_twice() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let data = client
        .kyc()
        .create_session(
            &json!({
                "subjectId": "u_1",
                "sessionType": "kyc",
                "externalCustomerId": "u_1",
                "person": {"firstName": "Ada", "lastName": "Lovelace"},
                "metadata": {"orderId": "o_1"},
                "ttlMinutes": 60,
            }),
            Some("kyc-key-1"),
        )
        .await
        .unwrap();
    assert_eq!(data["session"]["id"], "ks_1");
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/api/v1/kyc/sessions");
    assert_eq!(seen[0].headers["idempotency-key"], "kyc-key-1");
    let body = seen[0].body();
    assert_eq!(body["sessionType"], "KYC");
    assert_eq!(body["idempotencyKey"], "kyc-key-1");
    assert_eq!(body["customer"]["customerType"], "PERSON");
    assert_eq!(body["customer"]["firstName"], "Ada");
    assert_eq!(body["evidenceContextJson"], r#"{"orderId":"o_1"}"#);
    assert!(body.get("person").is_none() && body.get("metadata").is_none());
}

#[tokio::test]
async fn create_session_is_refused_locally() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let kyc = client.kyc();
    for bad in [
        json!({"subjectId": "u_1", "sessionType": "AML"}),
        json!({"subjectId": "u_1", "sessionType": "KYC", "person": {}, "company": {}}),
        json!({"subjectId": "u_1", "sessionType": "KYC", "ttlMinutes": 1441}),
        json!({"subjectId": "u_1", "sessionType": "KYC", "ttlMinutes": 0}),
        json!({"subjectId": "u_1", "sessionType": "KYC", "metadata": {}, "evidenceContextJson": "{}"}),
        json!({"subjectId": "u_1", "sessionType": "KYC", "requiredDocumentTypes": "PASSPORT"}),
        json!({"subjectId": "u_1", "sessionType": "KYC", "status": "APPROVED"}),
        json!({"sessionType": "KYC"}),
    ] {
        assert!(
            matches!(kyc.create_session(&bad, None).await, Err(Error::Invalid(_))),
            "{bad}"
        );
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn list_sessions_takes_only_limit_and_cursor() {
    let (_server, client, seen) = recording_gateway(
        |_, _| {
            Reply::json(
                200,
                r#"{"status":"success","data":{"items":[{"id":"ks_1"}],"pageInfo":{"nextCursor":"c2","hasMore":true}}}"#,
            )
        },
        0,
    )
    .await;
    let kyc = client.kyc();
    let page = kyc
        .list_sessions(&json!({"pageSize": 10, "pageToken": "c1"}))
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.next_cursor.as_deref(), Some("c2"));
    assert!(page.has_more);
    let refused = kyc.list_sessions(&json!({"status": "APPROVED"})).await;
    assert!(matches!(refused, Err(Error::Invalid(_))));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].path, "/api/v1/kyc/sessions?limit=10&cursor=c1");
}

#[tokio::test]
async fn a_page_info_without_a_cursor_is_the_last_page() {
    let (_server, client, _) = recording_gateway(
        |_, _| {
            Reply::json(
                200,
                r#"{"status":"success","data":{"items":[{"id":"ks_1"}],"nextCursor":"stale","pageInfo":{"nextCursor":null}}}"#,
            )
        },
        0,
    )
    .await;
    let page = client.kyc().list_sessions(&Value::Null).await.unwrap();
    assert_eq!(page.next_cursor, None);
    assert!(!page.has_more);
}

#[tokio::test]
async fn session_operations_and_admin_routes() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let kyc = client.kyc();
    kyc.set_jurisdiction("ks_1", " TR ").await.unwrap();
    kyc.set_primary_document("ks_1", "PASSPORT").await.unwrap();
    kyc.submit("ks_1").await.unwrap();
    kyc.manual_review("ks_1", None, None).await.unwrap();
    kyc.rotate_webhook_secret().await.unwrap();
    kyc.webhook_outbox(&json!({"limit": 5, "status": "FAILED"}))
        .await
        .unwrap();
    kyc.retry_webhook_event("ev_1").await.unwrap();
    let refused = kyc.set_jurisdiction("ks_1", " ").await;
    assert!(matches!(refused, Err(Error::Invalid(_))));
    let seen = seen.lock().unwrap();
    let calls: Vec<(&str, &str)> = seen
        .iter()
        .map(|call| (call.method.as_str(), call.path.as_str()))
        .collect();
    assert_eq!(
        calls,
        [
            ("POST", "/api/v1/kyc/sessions/ks_1/jurisdiction"),
            ("POST", "/api/v1/kyc/sessions/ks_1/primary-document"),
            ("POST", "/api/v1/kyc/sessions/ks_1/submit"),
            ("POST", "/api/v1/kyc/sessions/ks_1/manual-review"),
            (
                "POST",
                "/api/v1/kyc/admin/merchant-policy/webhook-secret/rotate"
            ),
            (
                "GET",
                "/api/v1/kyc/admin/webhook-outbox?limit=5&status=FAILED"
            ),
            ("POST", "/api/v1/kyc/admin/webhook-outbox/ev_1/retry"),
        ]
    );
    assert_eq!(seen[0].body(), json!({"jurisdiction": "TR"}));
    assert!(seen[2].raw.is_empty());
    assert!(seen[3].raw.is_empty());
    assert!(
        seen.iter()
            .all(|call| !call.headers.contains_key("idempotency-key"))
    );
}

#[tokio::test]
async fn upload_document_never_echoes_the_image() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let image = "SECRETIMAGEBYTES0123456789";
    let result = client
        .kyc()
        .upload_document(
            "ks_1",
            &json!({"documentType": "PASSPORT", "fileBase64": image, "mimeType": "image/jpeg", "face": image}),
        )
        .await;
    let error = result.unwrap_err();
    assert!(matches!(error, Error::Invalid(_)));
    assert!(!format!("{error:?} {error}").contains(image));
    client
        .kyc()
        .upload_document(
            "ks_1",
            &json!({"documentType": "PASSPORT", "fileBase64": image, "mimeType": "image/jpeg"}),
        )
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].path, "/api/v1/kyc/sessions/ks_1/documents");
    assert_eq!(seen[0].body()["fileBase64"], image);
}
