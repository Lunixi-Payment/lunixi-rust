#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
mod common;
use common::{Incoming, Reply, Server, validate_signature};
use lunixi::{
    Client, Error, FRAUD_EVENT_BATCH_LIMIT, InstallmentOptionsRequest, IntentRequest,
    MarketplaceIntentRequest, MarketplaceSeller, StoreCardRequest,
};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

const KEY: &str = "attempt-0000000001";
const TOKEN: &str = r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#;
const PAN: &str = "4111111111111111";
const CVC: &str = "987";

struct Seen {
    method: String,
    path: String,
    idempotency_key: Option<String>,
    raw: String,
    body: Value,
}

/// A signed-request recording server; `reply` answers every merchant call.
async fn recording(
    reply: impl Fn(&Incoming) -> Reply + Send + Sync + 'static,
) -> (Server, Arc<Mutex<Vec<Seen>>>) {
    let seen = Arc::new(Mutex::new(vec![]));
    let log = seen.clone();
    let server = Server::start(move |r| {
        validate_signature(&r);
        if r.path == "/api/v1/auth/token" {
            return Reply::json(200, TOKEN);
        }
        assert_eq!(r.headers["authorization"], "Bearer at_TEST_fixture");
        let body = if r.body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&r.body).unwrap()
        };
        let answer = reply(&r);
        log.lock().unwrap().push(Seen {
            method: r.method.clone(),
            path: r.path.clone(),
            idempotency_key: r.headers.get("idempotency-key").cloned(),
            raw: String::from_utf8(r.body.clone()).unwrap(),
            body,
        });
        answer
    })
    .await;
    (server, seen)
}
fn ok(_: &Incoming) -> Reply {
    Reply::json(200, r#"{"data":{}}"#)
}

/// A server that counts merchant calls; every guard below must keep it at zero.
async fn counting() -> (Server, Client, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let server = Server::start(move |r| {
        if r.path == "/api/v1/auth/token" {
            return Reply::json(200, TOKEN);
        }
        seen.fetch_add(1, Ordering::SeqCst);
        Reply::json(200, r#"{"data":{}}"#)
    })
    .await;
    let client = server.client();
    (server, client, calls)
}
fn invalid<T: std::fmt::Debug>(result: lunixi::Result<T>, case: &str) {
    assert!(
        matches!(result, Err(Error::Invalid(_))),
        "{case}: {result:?}"
    );
}
fn card() -> Value {
    json!({"cardHolderName":"Test Buyer","cardNumber":PAN,"expireMonth":"12","expireYear":"2030","cvc":CVC})
}
fn intent() -> IntentRequest {
    IntentRequest::new(
        "10000",
        "TRY",
        "order-1",
        "https://merchant.example/payments/return",
    )
}

#[tokio::test]
async fn bin_info_and_card_taxonomy_are_signed() {
    let (server, seen) = recording(ok).await;
    let client = server.client();
    client.payments().bin_info("41111111").await.unwrap();
    client.payments().card_taxonomy().await.unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/api/v1/bin/info");
    assert_eq!(seen[0].raw, r#"{"binOrPan":"41111111"}"#);
    assert_eq!(seen[1].method, "GET");
    assert_eq!(seen[1].path, "/api/v1/bin/card-taxonomy");
    assert_eq!(seen[1].body, Value::Null);
}

#[tokio::test]
async fn bin_info_rejects_a_malformed_prefix_without_echoing_it() {
    let (_server, client, calls) = counting().await;
    for value in ["41111", "4111a111", "41111111111111111111", ""] {
        let result = client.payments().bin_info(value).await;
        let message = format!("{:?}", result);
        if !value.is_empty() {
            assert!(!message.contains(value), "error echoes the card prefix");
        }
        invalid(result, value);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn installment_options_body_and_enabled_installments_rule() {
    let (server, seen) = recording(ok).await;
    let client = server.client();
    let request = InstallmentOptionsRequest::new("10000", "TRY")
        .with_bin_or_pan("41111111")
        .with_installment(3)
        .with_enabled_installments(vec![3, 1, 2]);
    client
        .payments()
        .installment_options(&request)
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/api/v1/bin/installments");
    assert_eq!(
        seen[0].raw,
        r#"{"amount":"10000","currency":"TRY","binOrPan":"41111111","installment":3,"enabledInstallments":[1,2,3]}"#
    );
    assert!(!format!("{request:?}").contains("41111111"));
}

#[tokio::test]
async fn installment_options_refuse_broken_requests_before_sending() {
    let (_server, client, calls) = counting().await;
    for counts in [vec![], vec![2, 3], vec![1, 1], vec![0, 1], vec![1, 37]] {
        let request = InstallmentOptionsRequest::new("10000", "TRY")
            .with_enabled_installments(counts.clone());
        invalid(
            client.payments().installment_options(&request).await,
            &format!("{counts:?}"),
        );
    }
    for request in [
        InstallmentOptionsRequest::new("0", "TRY"),
        InstallmentOptionsRequest::new("10000", "try"),
        InstallmentOptionsRequest::new("10000", "TRY").with_installment(37),
        InstallmentOptionsRequest::new("10000", "TRY").with_bin_or_pan("4111"),
    ] {
        invalid(
            client.payments().installment_options(&request).await,
            "request",
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn stored_cards_use_the_documented_routes_and_keys() {
    let (server, seen) = recording(ok).await;
    let client = server.client();
    let request = StoreCardRequest::new(card(), "user-1", "https://merchant.example/cards/return")
        .with_currency("TRY");
    client.payments().store_card(&request, KEY).await.unwrap();
    client.payments().list_stored_cards("user-1").await.unwrap();
    client
        .payments()
        .get_stored_card("card_tok-1")
        .await
        .unwrap();
    client
        .payments()
        .deactivate_stored_card("card_tok-1", KEY)
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(
        (seen[0].method.as_str(), seen[0].path.as_str()),
        ("POST", "/api/v1/payments/cards")
    );
    assert_eq!(seen[0].idempotency_key.as_deref(), Some(KEY));
    assert_eq!(seen[0].body["card"]["cardNumber"], PAN);
    assert_eq!(seen[0].body["cardUserKey"], "user-1");
    assert_eq!(seen[0].body["currency"], "TRY");
    assert_eq!(
        (seen[1].method.as_str(), seen[1].path.as_str()),
        ("GET", "/api/v1/payments/cards?cardUserKey=user-1")
    );
    assert_eq!(
        (seen[2].method.as_str(), seen[2].path.as_str()),
        ("GET", "/api/v1/payments/cards/card_tok-1")
    );
    assert_eq!(
        (seen[3].method.as_str(), seen[3].path.as_str()),
        ("DELETE", "/api/v1/payments/cards/card_tok-1")
    );
    assert_eq!(seen[3].idempotency_key.as_deref(), Some(KEY));
    assert_eq!(seen[3].body, Value::Null);
}

#[tokio::test]
async fn stored_card_guards_send_nothing_and_never_show_the_card() {
    let (_server, client, calls) = counting().await;
    let request = StoreCardRequest::new(card(), "user-1", "https://merchant.example/cards/return");
    let shown = format!("{request:?}");
    assert!(!shown.contains(PAN) && !shown.contains(CVC));
    for key in ["", "short"] {
        let result = client.payments().store_card(&request, key).await;
        assert!(!format!("{result:?}").contains(PAN));
        invalid(result, "store card key");
        invalid(
            client
                .payments()
                .deactivate_stored_card("card_tok-1", key)
                .await,
            "deactivate key",
        );
    }
    let insecure = StoreCardRequest::new(card(), "user-1", "http://merchant.example/return");
    let result = client.payments().store_card(&insecure, KEY).await;
    assert!(!format!("{result:?}").contains(PAN));
    invalid(result, "http callback");
    invalid(
        client
            .payments()
            .store_card(
                &StoreCardRequest::new(json!({}), "user-1", "https://m.example/r"),
                KEY,
            )
            .await,
        "empty card",
    );
    invalid(
        client.payments().list_stored_cards(" ").await,
        "blank card user key",
    );
    invalid(
        client.payments().get_stored_card("../x").await,
        "token path",
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn marketplace_intent_sends_sellers_with_the_key() {
    let (server, seen) = recording(ok).await;
    let client = server.client();
    let request = MarketplaceIntentRequest::new(
        intent(),
        vec![
            MarketplaceSeller::new("dealer-1", "6000"),
            MarketplaceSeller::new("dealer-2", "4000").with_scenario(
                json!({"commissionRuleId": "rule-1"})
                    .as_object()
                    .cloned()
                    .unwrap(),
            ),
        ],
    );
    client
        .payments()
        .create_marketplace_intent(&request, KEY)
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/api/v1/payments/marketplace/intents");
    assert_eq!(seen[0].idempotency_key.as_deref(), Some(KEY));
    assert_eq!(seen[0].body["amount"], "10000");
    assert_eq!(
        seen[0].body["sellers"],
        json!([
            {"subDealerId":"dealer-1","amountMinor":"6000"},
            {"subDealerId":"dealer-2","amountMinor":"4000","scenario":{"commissionRuleId":"rule-1"}}
        ])
    );
    assert!(seen[0].body.get("price").is_none());
}

#[tokio::test]
async fn marketplace_total_mismatch_and_bad_sellers_fail_locally() {
    let (_server, client, calls) = counting().await;
    let cases = [
        vec![MarketplaceSeller::new("dealer-1", "6000")],
        vec![
            MarketplaceSeller::new("dealer-1", "6000"),
            MarketplaceSeller::new("dealer-2", "4001"),
        ],
        vec![MarketplaceSeller::new("dealer-1", "010000")],
        vec![MarketplaceSeller::new("dealer-1", "100.00")],
        vec![MarketplaceSeller::new(" ", "10000")],
        vec![],
    ];
    for sellers in cases {
        let request = MarketplaceIntentRequest::new(intent(), sellers);
        invalid(
            client
                .payments()
                .create_marketplace_intent(&request, KEY)
                .await,
            "sellers",
        );
    }
    let valid = MarketplaceIntentRequest::new(intent(), vec![MarketplaceSeller::new("d", "10000")]);
    invalid(
        client
            .payments()
            .create_marketplace_intent(&valid, "")
            .await,
        "key",
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn payment_analytics_routes() {
    let (server, seen) = recording(ok).await;
    let client = server.client();
    client
        .payments()
        .failover_recovery_analytics(vec![
            ("from".into(), "2026-11-01".into()),
            ("to".into(), String::new()),
        ])
        .await
        .unwrap();
    client
        .payments()
        .provider_credential_analytics("cred-1", vec![("range".into(), "7d".into())])
        .await
        .unwrap();
    {
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen[0].path,
            "/api/v1/payments/analytics/failover-recovery?from=2026-11-01"
        );
        assert_eq!(
            seen[1].path,
            "/api/v1/payments/analytics/provider-credentials/cred-1?range=7d"
        );
    }
    let (_server, client, calls) = counting().await;
    invalid(
        client
            .payments()
            .failover_recovery_analytics(vec![("bad name".into(), "x".into())])
            .await,
        "filter name",
    );
    invalid(
        client
            .payments()
            .provider_credential_analytics("", vec![])
            .await,
        "credential",
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

fn event(id: &str) -> Value {
    json!({"eventId":id,"subjectType":"payment","eventType":"payment.captured","occurredAt":"2026-11-15T10:00:00.000Z"})
}

#[tokio::test]
async fn fraud_record_batch_sends_an_array_with_the_key() {
    let (server, seen) = recording(ok).await;
    let client = server.client();
    client
        .fraud()
        .record_batch(vec![event("evt-1"), event("evt-2")], KEY)
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/api/v1/fraud/events/transactions/batch");
    assert_eq!(seen[0].idempotency_key.as_deref(), Some(KEY));
    assert_eq!(seen[0].body, json!([event("evt-1"), event("evt-2")]));
}

#[tokio::test]
async fn fraud_record_batch_guards() {
    let (_server, client, calls) = counting().await;
    invalid(client.fraud().record_batch(vec![], KEY).await, "empty");
    let too_many = (0..=FRAUD_EVENT_BATCH_LIMIT)
        .map(|i| event(&format!("evt-{i}")))
        .collect();
    invalid(
        client.fraud().record_batch(too_many, KEY).await,
        "501 events",
    );
    let mut missing = event("evt-1");
    missing.as_object_mut().unwrap().remove("occurredAt");
    invalid(
        client.fraud().record_batch(vec![missing], KEY).await,
        "field",
    );
    invalid(
        client.fraud().record_batch(vec![json!("x")], KEY).await,
        "not an object",
    );
    invalid(
        client.fraud().record_batch(vec![event("evt-1")], "").await,
        "key",
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn fraud_latest_for_payment_reads_the_first_decision() {
    let (server, seen) = recording(|r| {
        if r.path.contains("paymentId=pay-1") {
            Reply::json(
                200,
                r#"{"data":[{"traceId":"tr-2","decision":"allow"}],"pageInfo":{"hasMore":true}}"#,
            )
        } else {
            Reply::json(200, r#"{"data":[]}"#)
        }
    })
    .await;
    let client = server.client();
    let latest = client.fraud().latest_for_payment("pay-1").await.unwrap();
    assert_eq!(latest.unwrap()["traceId"], "tr-2");
    assert!(
        client
            .fraud()
            .latest_for_payment("pay-2")
            .await
            .unwrap()
            .is_none()
    );
    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].method, "GET");
        assert_eq!(
            seen[0].path,
            "/api/v1/fraud/decision-logs?paymentId=pay-1&limit=1"
        );
    }
    invalid(client.fraud().latest_for_payment("").await, "payment id");
}
