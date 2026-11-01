#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use common::{Reply, Server, seed, validate_signature};
use lunixi::{BasketItem, Client, Config, Error, IntentRequest, MAX_SAFE_INTEGER, Method, Request};
use serde_json::json;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tls_auth_signatures_query_envelope_and_single_flight() {
    let auth = Arc::new(AtomicUsize::new(0));
    let seen = auth.clone();
    let server = Server::start(move |r| {
        validate_signature(&r);
        if r.path == "/api/v1/auth/token" {
            seen.fetch_add(1, Ordering::SeqCst);
            Reply::json(
                200,
                r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#,
            )
        } else {
            assert_eq!(r.headers["authorization"], "Bearer at_TEST_fixture");
            assert!(r.path.starts_with("/api/v1/payments?"));
            assert!(r.path.contains("%C5%9E"));
            Reply::json(
                200,
                r#"{"data":{"items":[]},"pageInfo":{"hasNextPage":false}}"#,
            )
        }
    })
    .await;
    let client = server.client();
    let mut tasks = vec![];
    for _ in 0..24 {
        let c = client.clone();
        tasks.push(tokio::spawn(async move {
            let mut r = Request::new(Method::GET, "/api/v1/payments");
            r.query = vec![("search".into(), "Şükrü & Co".into())];
            let response = c.send(r).await.unwrap();
            assert_eq!(response.status, 200);
            assert!(response.page_info.is_some());
            assert!(response.envelope.is_some());
        }));
    }
    for t in tasks {
        t.await.unwrap()
    }
    assert_eq!(auth.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn token_401_refreshes_once_and_keeps_payment_identity() {
    let auth = Arc::new(AtomicUsize::new(0));
    let count = auth.clone();
    let server = Server::start(move |r| {
        validate_signature(&r);
        if r.path == "/api/v1/auth/token" {
            let n = count.fetch_add(1, Ordering::SeqCst);
            Reply::json(
                200,
                &format!(r#"{{"data":{{"access_token":"at_TEST_{n}","expires_in":1800}}}}"#),
            )
        } else if r.headers["authorization"] == "Bearer at_TEST_0" {
            Reply::json(401, r#"{"code":"expired"}"#)
        } else {
            Reply::json(200, r#"{"data":{"status":"CAPTURED"}}"#)
        }
    })
    .await;
    let response = server.client().payments().get("payment-1").await.unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(auth.load(Ordering::SeqCst), 2);
}
#[tokio::test]
async fn retries_same_key_body_fresh_nonce_unkeyed_never_retries() {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let previous = Arc::new(Mutex::new(None));
    let first = previous.clone();
    let server = Server::start(move |r| {
        validate_signature(&r);
        if r.path == "/api/v1/auth/token" {
            return Reply::json(
                200,
                r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#,
            );
        }
        assert_eq!(r.headers["idempotency-key"], "attempt-0000000001");
        let n = seen.fetch_add(1, Ordering::SeqCst);
        if n == 0 {
            *first.lock().unwrap() = Some((r.body.clone(), r.headers["x-nonce"].clone()));
            Reply::json(503, r#"{"code":"unavailable"}"#)
        } else {
            let previous = first.lock().unwrap();
            let (body, nonce) = previous.as_ref().unwrap();
            assert_eq!(*body, r.body);
            assert_ne!(*nonce, r.headers["x-nonce"]);
            Reply::json(200, r#"{"data":{"status":"CAPTURED"}}"#)
        }
    })
    .await;
    let mut config = server.config();
    config.max_retries = 2;
    let client = Client::new(config).unwrap();
    client
        .payments()
        .capture("p-1", Some("100"), "attempt-0000000001")
        .await
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let server = Server::start(move |r| {
        if r.path == "/api/v1/auth/token" {
            return Reply::json(
                200,
                r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#,
            );
        }
        seen.fetch_add(1, Ordering::SeqCst);
        Reply::json(
            503,
            r#"{"code":"unavailable","message":"private customer secret"}"#,
        )
    })
    .await;
    let mut c = server.config();
    c.max_retries = 3;
    let err = Client::new(c)
        .unwrap()
        .fraud()
        .evaluate(json!({"transactionRef":"order-1"}))
        .await
        .unwrap_err();
    assert!(!format!("{err:?} {err}").contains("private customer secret"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn no_redirects_or_credentials_to_a_second_origin() {
    let leaked = Arc::new(AtomicUsize::new(0));
    let seen = leaked.clone();
    let attacker = Server::start(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
        Reply::json(200, "{}")
    })
    .await;
    let target = attacker.base.clone();
    let server = Server::start(move |r| {
        if r.path == "/api/v1/auth/token" {
            return Reply::json(
                200,
                r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#,
            );
        }
        Reply {
            status: 307,
            body: String::new(),
            headers: vec![("Location".into(), format!("{target}/api/v1/payments"))],
        }
    })
    .await;
    let result = server.client().payments().get("p").await;
    assert!(matches!(result, Err(Error::Api { status: 307, .. })));
    assert_eq!(leaked.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn configuration_path_amount_and_tenant_guards_precede_network() {
    for base in [
        "http://example.com",
        "http://localhost.evil.test",
        "https://user:secret@example.com",
        "https://example.com/path",
        "https://example.com?",
        "https://example.com?x=1",
        "https://example.com#x",
    ] {
        let mut c = Config::new(base, "key", seed());
        c.allow_insecure_localhost = true;
        assert!(Client::new(c).is_err(), "accepted {base}")
    }
    assert!(Client::new(Config::new("http://127.0.0.1:1", "key", seed())).is_err());
    let mut c = Config::new("http://127.0.0.1:1", "key", seed());
    c.allow_insecure_localhost = true;
    assert!(Client::new(c).is_ok());
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let server = Server::start(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
        Reply::json(500, "{}")
    })
    .await;
    let client = server.client();
    for path in [
        "https://evil.example/api/v1/payments",
        "//evil.example/api/v1/payments",
        "/api/v1/../auth",
        "/api/v1/%2e%2e/auth",
        "/api/v1/payments?token=secret",
        "/api/v1/payments\\x",
        "/api/v1/auth/token",
        "/api/v1/auth/token/",
    ] {
        assert!(client.send(Request::new(Method::GET, path)).await.is_err())
    }
    for body in [
        json!({"amount":"100","price":"100"}),
        json!({"amount":"100","paidPrice":null}),
        json!({"amount":100.5}),
        json!({"amount":MAX_SAFE_INTEGER+1}),
        json!({"amount":"0"}),
    ] {
        assert!(
            client
                .send(Request::new(Method::POST, "/api/v1/payments/intents").json(body))
                .await
                .is_err()
        )
    }
    assert!(
        client
            .send(Request::new(Method::POST, "/api/v1/payments/intents"))
            .await
            .is_err()
    );
    assert!(
        client
            .payments()
            .refund("p", Some("-1"), None, "attempt-0000000001")
            .await
            .is_err()
    );
    assert!(
        client
            .payments()
            .capture("p", Some("100"), "short")
            .await
            .is_err()
    );
    assert!(
        client
            .fraud()
            .evaluate(json!({"tenantId":"another-merchant"}))
            .await
            .is_err()
    );
    let mut intent = IntentRequest::new("200", "TRY", "order-1", "https://merchant.example/return");
    intent
        .basket_items
        .push(BasketItem::new("one", "Product", "PHYSICAL", "100"));
    assert!(
        client
            .payments()
            .create_intent(&intent, "attempt-0000000001")
            .await
            .is_err()
    );
    assert!(client.payments().split(json!({"totalAmount":"100","currency":"TRY","ownerRef":"owner","legs":[{"amount":"99"}]}),"attempt-0000000001").await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn bounded_response_malformed_json_and_timeout() {
    let server =
        Server::start(|_| Reply::json(200, &format!(r#"{{"value":"{}"}}"#, "x".repeat(1000))))
            .await;
    let mut config = server.config();
    config.max_response_bytes = 100;
    let client = Client::new(config).unwrap();
    assert!(matches!(
        client.payments().get("p").await,
        Err(Error::Response("size limit"))
    ));
    let server = Server::start(|_| Reply::json(200, "{invalid")).await;
    assert!(matches!(
        server.client().payments().get("p").await,
        Err(Error::Response("JSON body"))
    ));
    let server = Server::start(|_| {
        std::thread::sleep(Duration::from_millis(100));
        Reply::json(200, "{}")
    })
    .await;
    let mut config = server.config();
    config.timeout = Duration::from_millis(10);
    assert!(matches!(
        Client::new(config).unwrap().payments().get("p").await,
        Err(Error::Transport)
    ));
}
#[tokio::test]
async fn split_pending_stays_explicit_and_debug_omits_body() {
    let server=Server::start(|r|{if r.path=="/api/v1/auth/token"{return Reply::json(200,r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#)}Reply::json(400,r#"{"data":{"splitPaymentId":"split-1","status":"AWAITING_3DS","private":"customer-secret"}}"#)}).await;
    let response = server
        .client()
        .payments()
        .split(
            json!({"totalAmount":"100","currency":"TRY","ownerRef":"owner","legs":[{"amount":"100"}]}),
            "attempt-0000000001",
        )
        .await
        .unwrap();
    assert_eq!(response.status, 400);
    assert_eq!(response.data.as_ref().unwrap()["status"], "AWAITING_3DS");
    assert!(!format!("{response:?}").contains("customer-secret"));
}

#[tokio::test]
async fn tls_certificate_validation_is_enabled() {
    let server = Server::start(|_| {
        Reply::json(
            200,
            r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#,
        )
    })
    .await;
    let config = Config::new(&server.base, "mk_test_fixture", seed());
    assert!(matches!(
        Client::new(config).unwrap().payments().get("p").await,
        Err(Error::Transport)
    ));
}

#[tokio::test]
async fn token_expiry_margin_renews_before_reuse() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let server = Server::start(move |r| {
        if r.path == "/api/v1/auth/token" {
            seen.fetch_add(1, Ordering::SeqCst);
            Reply::json(200, r#"{"access_token":"at_TEST_fixture","expires_in":1}"#)
        } else {
            Reply::json(200, r#"{"data":{}}"#)
        }
    })
    .await;
    let client = server.client();
    client.payments().get("p").await.unwrap();
    tokio::time::sleep(Duration::from_millis(650)).await;
    client.payments().get("p").await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn typed_payment_and_fraud_routes_preserve_wire_contract() {
    let seen = Arc::new(Mutex::new(vec![]));
    let captured = seen.clone();
    let server = Server::start(move |r| {
        validate_signature(&r);
        if r.path == "/api/v1/auth/token" {
            return Reply::json(
                200,
                r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#,
            );
        }
        let body: serde_json::Value = if r.body.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&r.body).unwrap()
        };
        if r.method == "POST" {
            assert_eq!(r.headers["idempotency-key"], "attempt-0000000001")
        }
        captured
            .lock()
            .unwrap()
            .push((r.method.clone(), r.path.clone(), body));
        Reply::json(
            200,
            r#"{"data":{"token":"synthetic_checkout_token","status":"CAPTURED"}}"#,
        )
    })
    .await;
    let client = server.client();
    let key = "attempt-0000000001";
    let mut intent = IntentRequest::new("100", "TRY", "order-1", "https://merchant.example/return");
    intent
        .basket_items
        .push(BasketItem::new("one", "Example", "PHYSICAL", "100"));
    intent.force_3d = Some(false);
    client.payments().create_intent(&intent, key).await.unwrap();
    client
        .payments()
        .refund("p", Some("50"), Some("Return"), key)
        .await
        .unwrap();
    client.payments().void("p", key).await.unwrap();
    client.payments().get_split("s").await.unwrap();
    client
        .payments()
        .refund_split("s", "25", Some("Return"), key)
        .await
        .unwrap();
    client.payments().methods(Some("CARD")).await.unwrap();
    client.fraud().record_event(json!({"eventId":"ev-1","subjectType":"CUSTOMER","eventType":"LOGIN","occurredAt":"2026-10-31T00:00:00Z"}),key).await.unwrap();
    client.fraud().decision("trace-1").await.unwrap();
    let routes = seen.lock().unwrap();
    assert_eq!(routes[0].1, "/api/v1/payments/intents");
    assert_eq!(routes[0].2["amount"], "100");
    assert_eq!(routes[0].2["force3D"], false);
    assert_eq!(routes[1].1, "/api/v1/payments/p/refund");
    assert_eq!(
        routes[1].2,
        json!({"paymentIntentId":"p","amount":"50","reason":"Return"})
    );
    assert_eq!(routes[2].1, "/api/v1/payments/p/void");
    assert!(routes[2].2.is_null());
    assert_eq!(routes[3].1, "/api/v1/payments/split/s");
    assert_eq!(routes[4].1, "/api/v1/payments/split/s/refund");
    assert_eq!(routes[4].2, json!({"amount":"25","reason":"Return"}));
    assert_eq!(routes[5].1, "/api/v1/payments/methods?paymentMethod=CARD");
    assert_eq!(routes[6].1, "/api/v1/fraud/events/transactions");
    assert_eq!(routes[7].1, "/api/v1/fraud/decision-logs/trace-1");
}

#[tokio::test]
async fn optional_money_none_omits_while_json_null_is_rejected() {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let server = Server::start(move |request| {
        validate_signature(&request);
        if request.path == "/api/v1/auth/token" {
            return Reply::json(
                200,
                r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#,
            );
        }
        seen.fetch_add(1, Ordering::SeqCst);
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert!(body.get("amount").is_none());
        Reply::json(200, r#"{"data":{}}"#)
    })
    .await;
    let client = server.client();
    for operation in ["capture", "refund"] {
        assert!(
            client
                .send(
                    Request::new(Method::POST, format!("/api/v1/payments/p/{operation}"))
                        .json(json!({"amount":null}))
                        .idempotency_key("stable-operation-key")
                )
                .await
                .is_err()
        );
    }
    assert_eq!(count.load(Ordering::SeqCst), 0);
    client
        .payments()
        .capture("p", None, "stable-capture-key")
        .await
        .unwrap();
    client
        .payments()
        .refund("p", None, None, "stable-refund-key")
        .await
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
}
