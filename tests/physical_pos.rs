//! Physical POS payments, management, outcomes and webhooks against a local
//! TLS gateway double.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
mod common;
use common::{Incoming, Reply, Server, validate_signature};
use hmac::{Hmac, KeyInit, Mac};
use lunixi::{
    Client, Error, PhysicalPos, PhysicalPosWebhookEvent, WebhookVerifier, physical_pos_events,
    physical_pos_payment_operation,
};
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

const TOKEN: &str = r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#;
const KEY: &str = "pos-key-1";
const ACCEPTED: &str = r#"{"data":{"payment":{"paymentId":"pp_1","paymentStatus":"NOT_STARTED","amountMinor":"4500"},"replayed":false}}"#;
const REPLAYED: &str = r#"{"data":{"payment":{"paymentId":"pp_1","paymentStatus":"APPROVED","amountMinor":"4500"},"replayed":true}}"#;

struct Seen {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    raw: Vec<u8>,
}
impl Seen {
    fn body(&self) -> Value {
        serde_json::from_slice(&self.raw).unwrap()
    }
}
type Log = Arc<Mutex<Vec<Seen>>>;

/// Answers the token and records every gateway call (signed, with the
/// token). `reply` gets the 0-based index of the call.
async fn gateway(
    reply: impl Fn(usize, &Incoming) -> Reply + Send + Sync + 'static,
    max_retries: u8,
) -> (Server, Client, Log) {
    let seen: Log = Arc::new(Mutex::new(vec![]));
    let log = seen.clone();
    let calls = AtomicUsize::new(0);
    let server = Server::start(move |r| {
        validate_signature(&r);
        if r.path == "/api/v1/auth/token" {
            return Reply::json(200, TOKEN);
        }
        assert_eq!(r.headers["authorization"], "Bearer at_TEST_fixture");
        let answer = reply(calls.fetch_add(1, Ordering::SeqCst), &r);
        log.lock().unwrap().push(Seen {
            method: r.method.clone(),
            path: r.path.clone(),
            headers: r.headers.clone(),
            raw: r.body.clone(),
        });
        answer
    })
    .await;
    let mut config = server.config();
    config.max_retries = max_retries;
    let client = Client::new(config).unwrap();
    (server, client, seen)
}
fn always(status: u16, body: &'static str) -> impl Fn(usize, &Incoming) -> Reply {
    move |_, _| Reply::json(status, body)
}
fn sale() -> Value {
    json!({
        "operation": physical_pos_payment_operation::SALE,
        "target": {"deviceId": "dev_1"},
        "amountMinor": "4500",
        "currency": "TRY",
        "merchantReference": "ORD-2026-000123",
        "allowDuplicateReference": false,
        "items": [{
            "lineNo": 1, "name": "Tea", "quantityMilli": 2000, "unitCode": "C62",
            "unitPriceMinor": "2250", "taxGroupCode": "VAT20", "lineTotalMinor": "4500"
        }],
    })
}
fn local_card_data(result: lunixi::Result<impl std::fmt::Debug>, field: &str) {
    match result {
        Err(Error::PhysicalPosCardDataRejected {
            field: Some(f),
            status: 0,
            ..
        }) => assert_eq!(f, field),
        other => panic!("{field}: {other:?}"),
    }
}

// ── Payment writes ─────────────────────────────────────────────────────────

#[tokio::test]
async fn create_payment_sends_the_checked_body_with_the_key_as_given() {
    let (_server, client, seen) = gateway(always(202, ACCEPTED), 0).await;
    let result = client
        .physical_pos()
        .create_payment(&sale(), KEY)
        .await
        .unwrap();
    assert_eq!(result.status_code, 202);
    assert!(result.accepted());
    assert!(!result.replayed);
    assert_eq!(result.idempotency_key, KEY);
    assert_eq!(result.payment().unwrap()["paymentId"], "pp_1");
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/api/v1/physical-pos/payments");
    assert_eq!(seen[0].headers["idempotency-key"], KEY);
    let body = seen[0].body();
    assert_eq!(body["amountMinor"], "4500");
    assert_eq!(body["items"][0]["quantityMilli"], 2000);
    assert_eq!(body["allowDuplicateReference"], false);
}

#[tokio::test]
async fn a_replay_is_200_with_replayed() {
    let (_server, client, _) = gateway(always(200, REPLAYED), 0).await;
    let result = client
        .physical_pos()
        .create_payment(&sale(), KEY)
        .await
        .unwrap();
    assert_eq!(result.status_code, 200);
    assert!(result.replayed);
    assert!(!result.accepted());
}

#[tokio::test]
async fn create_reversal_and_cancel_paths() {
    let (_server, client, seen) = gateway(always(202, ACCEPTED), 0).await;
    let refund = json!({
        "operation": physical_pos_payment_operation::REFUND,
        "currency": "TRY",
        "items": [{"lineNo": 1, "quantityMilli": 1000}],
    });
    client
        .physical_pos()
        .create_reversal("pp_1", &refund, KEY)
        .await
        .unwrap();
    let cancelled = client
        .physical_pos()
        .cancel_payment("pp_1", None)
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].path, "/api/v1/physical-pos/payments/pp_1/reversals");
    assert_eq!(
        seen[0].body()["items"],
        json!([{"lineNo": 1, "quantityMilli": 1000}])
    );
    assert_eq!(seen[1].path, "/api/v1/physical-pos/payments/pp_1/cancel");
    assert_eq!(seen[1].body(), json!({}));
    assert_eq!(cancelled.idempotency_key.len(), 36);
    assert_eq!(
        seen[1].headers["idempotency-key"],
        cancelled.idempotency_key
    );
}

#[tokio::test]
async fn bad_requests_fail_before_sending() {
    let (_server, client, seen) = gateway(always(202, ACCEPTED), 0).await;
    let pos = client.physical_pos();
    for key in ["short", " pos-key-1", "pos-key-1 ", "pos key 01"] {
        assert!(matches!(
            pos.create_payment(&sale(), key).await,
            Err(Error::Invalid(_))
        ));
    }
    let mut unknown = sale();
    unknown["note"] = json!("x");
    let mut number_amount = sale();
    number_amount["amountMinor"] = json!(4500);
    let mut blank_amount = sale();
    blank_amount["amountMinor"] = json!("");
    let mut missing = sale();
    missing.as_object_mut().unwrap().remove("target");
    let mut string_count = sale();
    string_count["installmentCount"] = json!("3");
    let mut empty_items = sale();
    empty_items["items"] = json!([]);
    let mut item_without_tax = sale();
    item_without_tax["items"][0]
        .as_object_mut()
        .unwrap()
        .remove("taxGroupCode");
    let mut unknown_target = sale();
    unknown_target["target"] = json!({"terminalId": "t"});
    for body in [
        unknown,
        number_amount,
        blank_amount,
        missing,
        string_count,
        empty_items,
        item_without_tax,
        unknown_target,
        json!([]),
        Value::Null,
    ] {
        assert!(
            matches!(pos.create_payment(&body, KEY).await, Err(Error::Invalid(_))),
            "{body}"
        );
    }
    assert!(matches!(
        pos.create_reversal("..", &json!({}), KEY).await,
        Err(Error::Invalid(_))
    ));
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn card_data_is_refused_before_sending() {
    let (_server, client, seen) = gateway(always(202, ACCEPTED), 0).await;
    let pos = client.physical_pos();
    let mut pan = sale();
    pan["pan"] = json!("x");
    local_card_data(pos.create_payment(&pan, KEY).await, "pan");
    let mut nested = sale();
    nested["items"][0]["cvv"] = json!("123");
    local_card_data(pos.create_payment(&nested, KEY).await, "items[0].cvv");
    let mut reference = sale();
    reference["merchantReference"] = json!("kart 4111 1111 1111 1111");
    local_card_data(
        pos.create_payment(&reference, KEY).await,
        "merchantReference",
    );
    let mut name = sale();
    name["items"][0]["name"] = json!("4111111111111111");
    local_card_data(pos.create_payment(&name, KEY).await, "items[0].name");
    let mut barcode = sale();
    barcode["items"][0]["name"] = json!("Simit 8691234567012");
    let mut tax = sale();
    tax["items"][0]["taxGroupCode"] = json!("8691234567012");
    local_card_data(pos.create_payment(&tax, KEY).await, "items[0].taxGroupCode");
    local_card_data(
        pos.list_payments(&json!({"merchantReference": "4111111111111111"}))
            .await,
        "merchantReference",
    );
    local_card_data(
        client
            .physical_pos_management()
            .list_devices(&json!({"q": "4111-1111-1111-1111"}))
            .await,
        "q",
    );
    local_card_data(
        pos.list_payments(&json!({"cardNumber": "x"})).await,
        "cardNumber",
    );
    assert!(seen.lock().unwrap().is_empty());
    pos.create_payment(&barcode, KEY).await.unwrap();
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn gateway_card_data_rejection_keeps_the_field_path() {
    let body =
        r#"{"code":"physical_pos.payment.card_data_rejected","details":{"field":"items[0].name"}}"#;
    let (_server, client, _) = gateway(always(400, body), 2).await;
    match client.physical_pos().create_payment(&sale(), KEY).await {
        Err(Error::PhysicalPosCardDataRejected { field, status, .. }) => {
            assert_eq!(field.as_deref(), Some("items[0].name"));
            assert_eq!(status, 400);
        }
        other => panic!("{other:?}"),
    }
}

// ── Outcomes ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_5xx_without_a_not_started_code_is_an_unknown_outcome() {
    let (_server, client, seen) = gateway(always(500, r#"{"code":"INTERNAL"}"#), 0).await;
    match client.physical_pos().create_payment(&sale(), KEY).await {
        Err(Error::PhysicalPosOutcomeUnknown {
            idempotency_key,
            attempts,
            status,
            code,
            ..
        }) => {
            assert_eq!(idempotency_key, KEY);
            assert_eq!(attempts, 1);
            assert_eq!(status, 500);
            assert_eq!(code.as_deref(), Some("INTERNAL"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn an_unknown_outcome_is_retried_with_the_same_key_and_body() {
    let reply = |call: usize, _: &Incoming| match call {
        0 => Reply::json(503, r#"{"code":"UPSTREAM"}"#),
        _ => Reply::json(200, REPLAYED),
    };
    let (_server, client, seen) = gateway(reply, 1).await;
    let result = client
        .physical_pos()
        .create_payment(&sale(), KEY)
        .await
        .unwrap();
    assert!(result.replayed);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].raw, seen[1].raw);
    assert_eq!(seen[0].headers["idempotency-key"], KEY);
    assert_eq!(seen[1].headers["idempotency-key"], KEY);
}

#[tokio::test]
async fn a_later_4xx_after_an_unknown_outcome_stays_unknown() {
    let reply = |call: usize, _: &Incoming| match call {
        0 => Reply::json(502, "{}"),
        _ => Reply::json(409, r#"{"code":"physical_pos.idempotency_in_progress"}"#),
    };
    let (_server, client, seen) = gateway(reply, 3).await;
    match client.physical_pos().create_payment(&sale(), KEY).await {
        Err(Error::PhysicalPosOutcomeUnknown {
            attempts, status, ..
        }) => {
            assert_eq!(attempts, 2);
            assert_eq!(status, 409);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn a_not_started_5xx_is_a_definite_error() {
    let body = r#"{"code":"physical_pos.presence.unavailable"}"#;
    let (_server, client, seen) = gateway(always(503, body), 1).await;
    match client.physical_pos().create_payment(&sale(), KEY).await {
        Err(Error::Api { status, code, .. }) => {
            assert_eq!(status, 503);
            assert_eq!(code, "physical_pos.presence.unavailable");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn a_4xx_stops_at_once() {
    let body = r#"{"code":"physical_pos.payment.previous_outcome_unknown"}"#;
    let (_server, client, seen) = gateway(always(409, body), 3).await;
    assert!(matches!(
        client.physical_pos().create_payment(&sale(), KEY).await,
        Err(Error::Api { status: 409, .. })
    ));
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_2xx_without_a_payment_or_with_an_unreadable_body_is_unknown() {
    for body in [
        r#"{"data":{}}"#,
        "not json",
        r#"{"data":{"payment":{"paymentId":"pp_1","amountMinor":45}}}"#,
    ] {
        let (_server, client, _) = gateway(always(202, body), 0).await;
        assert!(
            matches!(
                client.physical_pos().create_payment(&sale(), KEY).await,
                Err(Error::PhysicalPosOutcomeUnknown { attempts: 1, .. })
            ),
            "{body}"
        );
    }
}

#[tokio::test]
async fn no_http_answer_is_an_unknown_outcome() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut config = lunixi::Config::new(
        format!("http://127.0.0.1:{port}"),
        "mk_test_fixture",
        common::seed(),
    );
    config.allow_insecure_localhost = true;
    let client = Client::new(config).unwrap();
    match client.physical_pos().create_payment(&sale(), KEY).await {
        Err(Error::PhysicalPosOutcomeUnknown {
            idempotency_key,
            status: 0,
            code: None,
            ..
        }) => assert_eq!(idempotency_key, KEY),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn rate_limit_and_void_after_settlement_errors() {
    let reply = |_: usize, r: &Incoming| {
        if r.path.ends_with("/reversals") {
            return Reply::json(
                409,
                r#"{"code":"physical_pos.payment.void_after_settlement","details":{"allowedOperation":"REFUND"}}"#,
            );
        }
        let mut reply = Reply::json(
            429,
            r#"{"code":"RATE_LIMITED","details":{"retryAfterSeconds":7}}"#,
        );
        if r.method == "POST" {
            reply.headers.push(("Retry-After".into(), "3".into()));
        }
        reply
    };
    let (_server, client, _) = gateway(reply, 0).await;
    let pos = client.physical_pos();
    assert!(matches!(
        pos.create_payment(&sale(), KEY).await,
        Err(Error::PhysicalPosRateLimit {
            retry_after: Some(3),
            ..
        })
    ));
    assert!(matches!(
        pos.list_payments(&Value::Null).await,
        Err(Error::PhysicalPosRateLimit {
            retry_after: Some(7),
            ..
        })
    ));
    let void = json!({"operation": "VOID", "currency": "TRY"});
    match pos.create_reversal("pp_1", &void, KEY).await {
        Err(Error::PhysicalPosVoidAfterSettlement {
            allowed_operation, ..
        }) => assert_eq!(allowed_operation.as_deref(), Some("REFUND")),
        other => panic!("{other:?}"),
    }
}

// ── Reads, filters, end-of-day ─────────────────────────────────────────────

#[tokio::test]
async fn reads_unwrap_records_and_pages() {
    let reply = |_: usize, r: &Incoming| {
        if r.path.starts_with("/api/v1/physical-pos/transactions?") {
            return Reply::json(
                200,
                r#"{"data":{"items":[{"paymentId":"pp_1","amountMinor":"100"}]},"pageInfo":{"hasMore":true,"nextCursor":"c2","totalCount":9}}"#,
            );
        }
        if r.path.starts_with("/api/v1/physical-pos/reports/") {
            return Reply::json(
                200,
                r#"{"data":{"rows":[{"saleMinor":"100","netMinor":"-20","difference":{"amountMinor":"-5"}}]}}"#,
            );
        }
        Reply::json(
            200,
            r#"{"data":{"payment":{"paymentId":"pp_1","approvedAmountMinor":null}}}"#,
        )
    };
    let (_server, client, seen) = gateway(reply, 0).await;
    let pos = client.physical_pos();
    assert_eq!(pos.get_payment("pp_1").await.unwrap()["paymentId"], "pp_1");
    let page = pos
        .list_transactions(&json!({
            "limit": 50, "cursor": "c1", "amountFromMinor": "100", "operation": "SALE", "origin": ""
        }))
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.has_more);
    assert_eq!(page.next_cursor.as_deref(), Some("c2"));
    assert_eq!(page.total_count, Some(9));
    let summary = pos
        .get_sales_summary(&json!({"dateFrom": "2026-11-01", "groupBy": "DAY"}))
        .await
        .unwrap();
    assert_eq!(summary["rows"][0]["netMinor"], "-20");
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "GET");
    assert_eq!(seen[0].path, "/api/v1/physical-pos/payments/pp_1");
    assert_eq!(
        seen[1].path,
        "/api/v1/physical-pos/transactions?limit=50&cursor=c1&amountFromMinor=100&operation=SALE"
    );
    assert_eq!(
        seen[2].path,
        "/api/v1/physical-pos/reports/sales-summary?dateFrom=2026-11-01&groupBy=DAY"
    );
}

#[tokio::test]
async fn filters_are_checked_before_sending() {
    let (_server, client, seen) = gateway(always(200, r#"{"data":{"items":[]}}"#), 0).await;
    let pos = client.physical_pos();
    for filters in [
        json!({"status": "x"}),
        json!({"limit": 0}),
        json!({"limit": 101}),
        json!({"limit": "50"}),
        json!({"includeTotal": "true"}),
        json!({"deviceId": 1}),
        json!([]),
    ] {
        assert!(
            matches!(
                pos.list_settlement_runs(&filters).await,
                Err(Error::Invalid(_))
            ),
            "{filters}"
        );
    }
    assert!(matches!(
        pos.list_transactions(&json!({"amountToMinor": 100})).await,
        Err(Error::Invalid(_))
    ));
    assert!(seen.lock().unwrap().is_empty());
    let page = pos
        .list_settlement_runs(&json!({"includeTotal": true, "runStatus": null}))
        .await
        .unwrap();
    assert!(page.items.is_empty());
    assert!(!page.has_more);
    assert_eq!(
        seen.lock().unwrap()[0].path,
        "/api/v1/physical-pos/settlement-runs?includeTotal=true"
    );
}

#[tokio::test]
async fn a_malformed_response_amount_is_refused() {
    let (_server, client, _) = gateway(
        always(200, r#"{"data":{"payment":{"amountMinor":"12.5"}}}"#),
        0,
    )
    .await;
    assert!(matches!(
        client.physical_pos().get_payment("pp_1").await,
        Err(Error::Response(_))
    ));
}

#[tokio::test]
async fn start_settlement_is_keyed() {
    let body = r#"{"data":{"run":{"runId":"run_1","runStatus":"PENDING"},"replayed":false}}"#;
    let (_server, client, seen) = gateway(always(202, body), 0).await;
    let pos = client.physical_pos();
    let generated = pos.start_settlement("dev_1", None).await.unwrap();
    assert_eq!(generated.status_code, 202);
    assert_eq!(generated.record("run").unwrap()["runId"], "run_1");
    let own = PhysicalPos::new_idempotency_key();
    let mine = pos.start_settlement("dev_1", Some(&own)).await.unwrap();
    assert_eq!(mine.idempotency_key, own);
    assert!(matches!(
        pos.start_settlement("dev_1", Some("")).await,
        Err(Error::Invalid(_))
    ));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(
        seen[0].path,
        "/api/v1/physical-pos/devices/dev_1/settlement"
    );
    assert_eq!(
        seen[0].headers["idempotency-key"],
        generated.idempotency_key
    );
    assert_eq!(seen[1].headers["idempotency-key"], own);
}

// ── Management ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn management_writes() {
    let reply = |_: usize, r: &Incoming| match r.method.as_str() {
        "POST" if r.path == "/api/v1/physical-pos/stores" => {
            Reply::json(201, r#"{"data":{"store":{"id":"st_1","rowVersion":1}}}"#)
        }
        "PATCH" => Reply::json(200, r#"{"data":{"store":{"id":"st_1","rowVersion":2}}}"#),
        _ => Reply::json(
            200,
            r#"{"data":{"policy":{"mode":"MANUAL","rowVersion":4}}}"#,
        ),
    };
    let (_server, client, seen) = gateway(reply, 0).await;
    let management = client.physical_pos_management();
    let created = management
        .create_store(&json!({"code": "S1", "name": "Main", "city": ""}), None)
        .await
        .unwrap();
    assert_eq!(created.status_code, 201);
    assert_eq!(created.record("store").unwrap()["id"], "st_1");
    let updated = management
        .update_store(
            "st_1",
            &json!({"rowVersion": 1, "city": null, "timezone": null}),
        )
        .await
        .unwrap();
    assert_eq!(updated["rowVersion"], 2);
    let policy = management
        .clear_store_settlement_policy("st_1", Some(4))
        .await
        .unwrap();
    assert_eq!(policy["mode"], "MANUAL");
    management
        .set_device_settlement_policy(
            "dev_1",
            &json!({"mode": "SCHEDULED", "scheduleTime": "23:30", "scheduleDays": ["MON"]}),
        )
        .await
        .unwrap();
    for bad in [
        management
            .update_store("st_1", &json!({"rowVersion": 0}))
            .await,
        management
            .update_store("st_1", &json!({"rowVersion": "1"}))
            .await,
        management.update_store("st_1", &json!({"name": "x"})).await,
    ] {
        assert!(matches!(bad, Err(Error::Invalid(_))));
    }
    assert!(matches!(
        management.clear_account_settlement_policy(Some(0)).await,
        Err(Error::Invalid(_))
    ));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 4);
    assert_eq!(seen[0].body(), json!({"code": "S1", "name": "Main"}));
    assert_eq!(seen[0].headers["idempotency-key"], created.idempotency_key);
    assert_eq!(seen[1].method, "PATCH");
    assert_eq!(seen[1].path, "/api/v1/physical-pos/stores/st_1");
    assert_eq!(seen[1].body(), json!({"rowVersion": 1, "city": null}));
    assert!(!seen[1].headers.contains_key("idempotency-key"));
    assert_eq!(seen[2].method, "DELETE");
    assert_eq!(
        seen[2].path,
        "/api/v1/physical-pos/stores/st_1/settlement-policy?expectedRowVersion=4"
    );
    assert_eq!(seen[3].method, "PUT");
    assert_eq!(
        seen[3].path,
        "/api/v1/physical-pos/devices/dev_1/settlement-policy"
    );
}

#[tokio::test]
async fn device_identity_paths() {
    let (_server, client, seen) =
        gateway(always(200, r#"{"data":{"key":{"keyId":"k1"}}}"#), 0).await;
    let management = client.physical_pos_management();
    management
        .revoke_device_key(
            "dev_1",
            "k1",
            &json!({"rowVersion": 3, "reasonCode": "DEVICE_LOST"}),
        )
        .await
        .unwrap();
    management
        .revert_device_enrollment("dev_1", "k1", &json!({"reasonCode": "WRONG_DEVICE"}))
        .await
        .unwrap();
    management
        .revoke_activation_code("dev_1", &json!({"reasonCode": "OTHER"}))
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen[0].path,
        "/api/v1/physical-pos/devices/dev_1/keys/k1/revoke"
    );
    assert_eq!(
        seen[1].path,
        "/api/v1/physical-pos/devices/dev_1/keys/k1/revert-enrollment"
    );
    assert_eq!(
        seen[2].path,
        "/api/v1/physical-pos/devices/dev_1/activation-code/revoke"
    );
}

// ── Webhooks ───────────────────────────────────────────────────────────────

const SECRET: &str = "whsec_physical_pos_fixture_0001";

fn signed(raw: &[u8], id: &str) -> HeaderMap {
    let ts = common::rfc3339_now();
    let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(format!("{id}.{ts}.").as_bytes());
    mac.update(raw);
    let hex: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut headers = HeaderMap::new();
    headers.insert("x-lunixi-event-id", HeaderValue::from_str(id).unwrap());
    headers.insert(
        "x-lunixi-signature-timestamp",
        HeaderValue::from_str(&ts).unwrap(),
    );
    headers.insert(
        "x-lunixi-signature",
        HeaderValue::from_str(&format!("v2={hex}")).unwrap(),
    );
    headers
}
fn parse(envelope: Value) -> lunixi::Result<PhysicalPosWebhookEvent> {
    let raw = serde_json::to_vec(&envelope).unwrap();
    let id = envelope["id"].as_str().unwrap().to_owned();
    let verifier = WebhookVerifier::new(vec![SECRET.into()]).unwrap();
    PhysicalPosWebhookEvent::parse(&verifier, &raw, &signed(&raw, &id))
}

#[test]
fn payment_webhook_with_cashier_and_original() {
    let event = parse(json!({
        "id": "evt_1",
        "type": physical_pos_events::PAYMENT_APPROVED,
        "environment": "LIVE",
        "createdAt": "2026-11-15T10:00:00Z",
        "data": {
            "paymentId": "pp_2",
            "historySeq": 4,
            "amountMinor": "4500",
            "operation": "REFUND",
            "original": {"paymentId": "pp_1", "origin": "DEVICE_DIRECT"},
            "cashier": {"cashierId": "c0", "cashierCode": "K01", "displayName": "Till 1"}
        }
    }))
    .unwrap();
    assert_eq!(event.id, "evt_1");
    assert_eq!(event.category, "payment");
    assert_eq!(event.environment.as_deref(), Some("LIVE"));
    assert_eq!(event.history_seq, Some(4));
    assert_eq!(event.original.as_ref().unwrap().origin, "DEVICE_DIRECT");
    assert_eq!(event.cashier.as_ref().unwrap().cashier_code, "K01");
    assert!(!event.is_device_sale);
}

#[test]
fn device_sale_without_cashier() {
    let event = parse(json!({
        "id": "evt_2",
        "type": physical_pos_events::PAYMENT_DEVICE_SALE,
        "data": {"paymentId": "pp_3", "historySeq": 1, "origin": "DEVICE_DIRECT", "amountMinor": "100"}
    }))
    .unwrap();
    assert!(event.is_device_sale);
    assert!(event.cashier.is_none());
    assert!(event.original.is_none());
}

#[test]
fn settlement_webhook_from_payload_json() {
    let payload = json!({
        "deviceId": "dev_1",
        "lines": [{"totals": {"saleMinor": "1000"}, "difference": {"saleMinor": "-10"}}]
    });
    let event = parse(json!({
        "id": "evt_3",
        "type": physical_pos_events::SETTLEMENT_COMPLETED,
        "payloadJson": payload.to_string()
    }))
    .unwrap();
    assert_eq!(event.category, "settlement");
    assert_eq!(event.history_seq, None);
    assert_eq!(event.data["deviceId"], "dev_1");
}

#[test]
fn webhook_failures() {
    let verifier = WebhookVerifier::new(vec![SECRET.into()]).unwrap();
    let raw = br#"{"id":"evt_4","type":"physical_pos.payment.approved","data":{"paymentId":"p"}}"#;
    let mut headers = signed(raw, "evt_4");
    headers.insert("x-lunixi-signature", HeaderValue::from_static("v2=00"));
    assert_eq!(
        PhysicalPosWebhookEvent::parse(&verifier, raw, &headers).unwrap_err(),
        Error::Webhook
    );
    for envelope in [
        json!({"id": "evt_5", "type": "payment.succeeded", "data": {"paymentId": "p"}}),
        json!({"id": "evt_6", "type": "physical_pos.payment.approved", "data": {}}),
        json!({"id": "evt_7", "type": "physical_pos.payment.approved", "data": {"amountMinor": 4500}}),
        json!({"id": "evt_8", "type": "physical_pos.payment.approved", "payloadJson": "{"}),
    ] {
        assert!(
            matches!(parse(envelope.clone()), Err(Error::Invalid(_))),
            "{envelope}"
        );
    }
}

#[tokio::test]
async fn encoded_identifiers_cannot_escape_the_path() {
    let (_server, client, seen) = gateway(always(200, r#"{"data":{}}"#), 0).await;
    for path in [
        "/api/v1/physical-pos/payments%2F..%2F..%2Fauth",
        "/api/v1/physical-pos/%2e%2e/auth",
        "/api/v1%2Fauth%2Ftoken",
        "/api/v1/physical-pos/payments/%zz",
    ] {
        let result = client
            .send(lunixi::Request::new(lunixi::Method::GET, path))
            .await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{path}");
    }
    assert!(seen.lock().unwrap().is_empty());
}
