//! Sanctions screening: route-table checks, the screening status rule and
//! the clear-read rule against a local TLS gateway double.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
mod common;
use common::{Incoming, Reply, recording_gateway};
use lunixi::{Error, is_clear_screening, sanction_coverage_status};
use serde_json::json;

const CLEAR: &str = r#"{"status":"success","data":{"screeningRequestId":"sr_1","matches":[],"coverageStatus":"COMPLETE"}}"#;

fn clear(_: usize, _: &Incoming) -> Reply {
    Reply::json(200, CLEAR)
}

#[tokio::test]
async fn screen_name_sends_the_checked_body() {
    let (_server, client, seen) = recording_gateway(clear, 0).await;
    let result = client
        .sanction()
        .screen_name(&json!({
            "name": "Ada Lovelace",
            "subjectType": "person",
            "riskDomains": ["sanctions", "pep"],
            "mode": "sanctions_pep",
            "identifiers": [{"type": "TCKN", "value": "10000000146"}],
            "country": null,
        }))
        .await
        .unwrap();
    assert!(is_clear_screening(&result));
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/api/v1/sanction/screening/name");
    assert!(!seen[0].headers.contains_key("idempotency-key"));
    let body = seen[0].body();
    assert_eq!(body["riskDomains"], json!(["sanctions", "pep"]));
    assert!(body.get("country").is_none());
}

#[tokio::test]
async fn bodies_are_checked_before_sending() {
    let (_server, client, seen) = recording_gateway(clear, 0).await;
    let sanction = client.sanction();
    for body in [
        json!({"subjectType": "person"}),
        json!({"name": "A", "nickname": "x"}),
        json!({"name": "A", "riskDomains": ["sanctions", "crime"]}),
        json!({"name": "A", "mode": "everything"}),
        json!({"name": "A", "identifiers": [{"type": "TCKN"}]}),
        json!({"name": "A", "identifiers": [{"type": "TCKN", "value": "1", "issuer": "x"}]}),
    ] {
        let result = sanction.screen_name(&body).await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{body}");
    }
    let result = sanction
        .screen_transaction(&json!({"amount": "100", "currency": "TRY"}))
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = sanction
        .screen_transaction(&json!({"parties": [{"role": "payer"}]}))
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = sanction
        .screen_public_procurement_ban(&json!({"onlyActive": true}))
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = sanction
        .screen_wallet(&json!({"walletAddress": "0xabc", "mode": "full_risk"}))
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = sanction
        .register_monitoring(
            &json!({"externalCustomerId": "c1", "profile": {}, "profileJson": "{}"}),
        )
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = sanction
        .list_monitoring_alerts(&json!({"severity": "high"}))
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_screening_that_did_not_complete_is_an_error() {
    let (_server, client, _) = recording_gateway(
        |call, _| {
            if call == 0 {
                Reply::json(
                    200,
                    r#"{"status":"failure","code":"sanction.screen_name.failed","data":{"matches":[]}}"#,
                )
            } else {
                Reply::json(
                    503,
                    r#"{"status":"failure","code":"sanction.screen_wallet.failed"}"#,
                )
            }
        },
        0,
    )
    .await;
    let result = client.sanction().screen_name(&json!({"name": "A"})).await;
    match result {
        Err(Error::Api { code, .. }) => assert_eq!(code, "sanction.screen_name.failed"),
        other => panic!("{other:?}"),
    }
    let result = client
        .sanction()
        .screen_wallet(&json!({"walletAddress": "0xabc"}))
        .await;
    assert!(matches!(result, Err(Error::Api { status: 503, .. })));
}

#[tokio::test]
async fn monitoring_and_intelligence_paths() {
    let (_server, client, seen) = recording_gateway(clear, 0).await;
    let sanction = client.sanction();
    sanction
        .register_monitoring(&json!({"externalCustomerId": "c1", "profile": {"name": "Ada"}}))
        .await
        .unwrap();
    sanction
        .get_indirect_exposure("ent 1", Some(2), &["e1", " ", "e2"])
        .await
        .unwrap();
    sanction
        .list_monitored_entities(&json!({"activeOnly": false, "includeTotal": true}))
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].body()["profileJson"], r#"{"name":"Ada"}"#);
    assert!(seen[0].body().get("profile").is_none());
    assert_eq!(
        seen[1].path,
        "/api/v1/sanction/intelligence/indirect-exposure/ent%201?maxDepth=2&entityIds=e1%2Ce2"
    );
    assert_eq!(
        seen[2].path,
        "/api/v1/sanction/monitoring/entities?activeOnly=false&includeTotal=true"
    );
}

#[test]
fn clear_read_rule() {
    assert!(is_clear_screening(
        &json!({"matches": [], "coverageStatus": "COMPLETE"})
    ));
    assert!(is_clear_screening(&json!({"hits": []})));
    for result in [
        json!({"matches": [], "coverageStatus": sanction_coverage_status::PARTIAL}),
        json!({"matches": [], "coverageStatus": "DEGRADED"}),
        json!({"matches": [], "coverageStatus": null}),
        json!({"matches": [{"entityId": "e1"}], "coverageStatus": "COMPLETE"}),
        json!({"publicationHits": [{"listedAddress": "0x"}]}),
        json!({"isSanctioned": true}),
        json!({"matched": true}),
        json!({"caseRequired": true}),
        json!({"decision": "REVIEW"}),
        json!(null),
    ] {
        assert!(!is_clear_screening(&result), "{result}");
    }
}
