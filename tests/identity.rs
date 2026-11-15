//! Identity signals: analysis bodies, account links and the session reads
//! against a local TLS gateway double.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
mod common;
use common::{Incoming, Reply, recording_gateway};
use lunixi::{Error, IDENTITY_ROUTES, identity_link_reasons, is_identity_device_shared};
use serde_json::json;

fn ok(_: usize, _: &Incoming) -> Reply {
    Reply::json(
        200,
        r#"{"status":"success","data":{"traceId":"tr_1","analysisStatus":"COMPLETED","riskFlags":[]}}"#,
    )
}

#[test]
fn route_table_and_helpers() {
    assert_eq!(IDENTITY_ROUTES.len(), 8);
    assert!(IDENTITY_ROUTES.iter().all(|route| !route.step_up));
    let links = json!({
        "linkedAccounts": [{"accountKeyHash": "h1"}],
        "sessions": [
            {"linkReasons": ["same_device_cluster", "same_fingerprint"]},
            {"linkReasons": ["same_fingerprint"]},
        ],
    });
    assert!(is_identity_device_shared(&links, 2));
    assert!(!is_identity_device_shared(&links, 3));
    assert!(!is_identity_device_shared(
        &json!({"linkedAccounts": []}),
        1
    ));
    assert_eq!(
        identity_link_reasons(&links),
        ["same_device_cluster", "same_fingerprint"]
    );
}

#[tokio::test]
async fn analyze_sync_sends_the_signal_body() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let result = client
        .identity()
        .analyze_sync(&json!({
            "subjectType": "CUSTOMER",
            "subjectId": "u_1",
            "device": {"fingerprintId": "fp_1"},
            "paymentId": null,
        }))
        .await
        .unwrap();
    assert_eq!(result["traceId"], "tr_1");
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/api/v1/identity/live-sessions/analyze-sync");
    assert_eq!(seen[0].body()["device"]["fingerprintId"], "fp_1");
    assert!(seen[0].body().get("paymentId").is_none());
}

#[tokio::test]
async fn analysis_bodies_are_checked_before_sending() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let identity = client.identity();
    for bad in [
        json!({"subjectType": "CUSTOMER"}),
        json!({"subjectId": "u_1"}),
        json!({"device": "fp_1"}),
        json!({"verdict": "ALLOW"}),
        json!([]),
    ] {
        assert!(
            matches!(identity.analyze(&bad).await, Err(Error::Invalid(_))),
            "{bad}"
        );
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn session_reads_and_lists() {
    let (_server, client, seen) = recording_gateway(
        |_, _| {
            Reply::json(
                200,
                r#"{"status":"success","data":{"items":[{"traceId":"tr_1"}],"nextCursor":"c2"}}"#,
            )
        },
        0,
    )
    .await;
    let identity = client.identity();
    identity.account_links("tr_1").await.unwrap();
    identity.related_sessions("tr_1", Some(5)).await.unwrap();
    identity.timeline("tr_1").await.unwrap();
    identity.top_sources(&json!({"days": 7})).await.unwrap();
    let page = identity
        .list_sessions(&json!({"deviceId": "d_1", "limit": 20}))
        .await
        .unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("c2"));
    let refused = identity.list_sessions(&json!({"cursor": "c2"})).await;
    assert!(matches!(refused, Err(Error::Invalid(_))));
    identity.get_session("tr_1").await.unwrap();
    let seen = seen.lock().unwrap();
    let paths: Vec<&str> = seen.iter().map(|call| call.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/api/v1/identity/live-sessions/tr_1/account-links",
            "/api/v1/identity/live-sessions/tr_1/related-sessions?limit=5",
            "/api/v1/identity/live-sessions/tr_1/timeline",
            "/api/v1/identity/live-sessions/top-sources?days=7",
            "/api/v1/identity/live-sessions?deviceId=d_1&limit=20",
            "/api/v1/identity/live-sessions/tr_1",
        ]
    );
    assert!(seen.iter().all(|call| call.method == "GET"));
}
