//! Subscriptions: route-table checks, keys in header and body, cursor
//! lists, money checks and the portal against a local TLS gateway double.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
mod common;
use common::{Incoming, Reply, recording_gateway};
use lunixi::{
    Error, SUBSCRIPTION_ROUTES, is_subscription_active, is_subscription_terminal,
    subscription_customer_error_code, subscription_portal_url, subscription_status,
};
use serde_json::{Value, json};

fn ok(_: usize, _: &Incoming) -> Reply {
    Reply::json(
        200,
        r#"{"status":"success","data":{"id":"sub_1","status":"ACTIVE"}}"#,
    )
}

#[test]
fn route_table_and_helpers() {
    assert_eq!(SUBSCRIPTION_ROUTES.len(), 63);
    assert!(SUBSCRIPTION_ROUTES.iter().all(|route| !route.step_up));
    let create = SUBSCRIPTION_ROUTES
        .iter()
        .find(|route| route.name == "create")
        .unwrap();
    assert_eq!(create.idempotency_key_rule(), "required");
    assert!(create.accepts("idempotencyKey"));
    assert!(is_subscription_active(&json!({"status": "PAST_DUE"})));
    assert!(is_subscription_terminal(&json!(
        subscription_status::CANCELED
    )));
    assert!(!is_subscription_active(&json!({"status": "PAUSED"})));
    assert_eq!(
        subscription_portal_url(&json!({"token": "a b/c"}), None),
        "https://portal.lunixi.com/?token=a%20b%2Fc"
    );
    assert_eq!(
        subscription_portal_url(&json!("t1"), Some("https://p.example/")),
        "https://p.example/?token=t1"
    );
}

#[tokio::test]
async fn create_sends_the_key_in_the_header_and_the_body() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let data = client
        .subscriptions()
        .create(
            &json!({"customerId": "cus_1", "planId": "plan_1", "collectionMethod": "AUTO_CHARGE", "startsAt": null}),
            Some(" sub-key-1 "),
        )
        .await
        .unwrap();
    assert_eq!(data["id"], "sub_1");
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/api/v1/subscriptions/subscriptions");
    assert_eq!(seen[0].headers["idempotency-key"], "sub-key-1");
    let body = seen[0].body();
    assert_eq!(body["idempotencyKey"], "sub-key-1");
    assert!(body.get("startsAt").is_none());
}

#[tokio::test]
async fn create_is_refused_locally() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let subscriptions = client.subscriptions();
    let base = json!({"customerId": "cus_1", "planId": "plan_1"});
    assert!(matches!(
        subscriptions.create(&base, None).await,
        Err(Error::Invalid(_))
    ));
    let unknown = json!({"customerId": "cus_1", "planId": "plan_1", "trial": true});
    assert!(matches!(
        subscriptions.create(&unknown, Some("k1")).await,
        Err(Error::Invalid(_))
    ));
    let method = json!({"customerId": "cus_1", "planId": "plan_1", "collectionMethod": "CASH"});
    assert!(matches!(
        subscriptions.create(&method, Some("k1")).await,
        Err(Error::Invalid(_))
    ));
    let differ = json!({"customerId": "cus_1", "planId": "plan_1", "idempotencyKey": "k2"});
    assert!(matches!(
        subscriptions.create(&differ, Some("k1")).await,
        Err(Error::Invalid(_))
    ));
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn lifecycle_routes() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let subscriptions = client.subscriptions();
    subscriptions
        .cancel("sub_1", true, Some("moved"))
        .await
        .unwrap();
    subscriptions.pause("sub_1", None).await.unwrap();
    subscriptions.resume("sub_1").await.unwrap();
    subscriptions
        .change_plan(
            "sub_1",
            &json!({"newPlanId": "plan_2", "behavior": "next_period"}),
            Some("chg-1"),
        )
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen[0].path,
        "/api/v1/subscriptions/subscriptions/sub_1/cancel"
    );
    assert_eq!(
        seen[0].body(),
        json!({"cancelAtPeriodEnd": true, "reason": "moved"})
    );
    assert_eq!(
        seen[1].path,
        "/api/v1/subscriptions/subscriptions/sub_1/pause"
    );
    assert!(seen[1].raw.is_empty());
    assert!(seen[2].raw.is_empty());
    let body = seen[3].body();
    assert_eq!(body["behavior"], "NEXT_PERIOD");
    assert_eq!(body["idempotencyKey"], "chg-1");
    assert_eq!(seen[3].headers["idempotency-key"], "chg-1");
}

#[tokio::test]
async fn cursor_lists_read_the_page_token_and_send_booleans() {
    let (_server, client, seen) = recording_gateway(
        |_, _| {
            Reply::json(
                200,
                r#"{"status":"success","data":[{"id":"plan_1","price":"9900"}],"nextPageToken":"t2","pageSize":1}"#,
            )
        },
        0,
    )
    .await;
    let page = client
        .subscriptions()
        .plans()
        .list(&json!({"includeArchived": true, "pageSize": 1, "productId": ""}))
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.next_cursor.as_deref(), Some("t2"));
    assert!(page.has_more);
    assert_eq!(page.page_size, Some(1));
    let cards = client
        .subscriptions()
        .cards()
        .list("cus_1", &Value::Null)
        .await
        .unwrap();
    assert_eq!(cards.page_size, Some(1));
    let refused = client
        .subscriptions()
        .plans()
        .list(&json!({"status": "ACTIVE"}))
        .await;
    assert!(matches!(refused, Err(Error::Invalid(_))));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(
        seen[0].path,
        "/api/v1/subscriptions/plans?includeArchived=true&pageSize=1"
    );
    assert_eq!(
        seen[1].path,
        "/api/v1/subscriptions/cards?customerId=cus_1&includeInactive=false"
    );
}

#[tokio::test]
async fn plans_create_checks_price_currency_and_billing() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let plans = client.subscriptions().plans();
    let plan = json!({
        "productId": " prod_1 ",
        "code": "pro",
        "name": "Pro",
        "price": "0",
        "currency": "try",
        "billing": {"frequency": "monthly"},
    });
    plans.create(&plan, None).await.unwrap();
    for bad in [
        json!({"productId": "p", "code": "c", "name": "n", "currency": "TRY", "billing": {"frequency": "MONTHLY"}}),
        json!({"productId": "p", "code": "c", "name": "n", "price": 9900, "currency": "TRY", "billing": {"frequency": "MONTHLY"}}),
        json!({"productId": "p", "code": "c", "name": "n", "price": "9900", "currency": "XYZ", "billing": {"frequency": "MONTHLY"}}),
        json!({"productId": "p", "code": "c", "name": "n", "price": "9900", "currency": "TRY", "billing": {"frequency": "HOURLY"}}),
        json!({"productId": "p", "code": "c", "name": "n", "price": "9900", "currency": "TRY", "billing": {"frequency": "MONTHLY", "byMonthDay": [32]}}),
        json!({"productId": "p", "code": "c", "name": "n", "price": "9900", "currency": "TRY", "billing": {"frequency": "MONTHLY", "every": 2}}),
        json!({"productId": "p", "code": "c", "name": "n", "price": "9900", "currency": "TRY", "billing": {"frequency": "MONTHLY"}, "trialPeriodDays": -1}),
    ] {
        assert!(
            matches!(plans.create(&bad, None).await, Err(Error::Invalid(_))),
            "{bad}"
        );
    }
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    let body = seen[0].body();
    assert_eq!(body["productId"], "prod_1");
    assert_eq!(body["currency"], "TRY");
    assert_eq!(
        body["billing"],
        json!({"frequency": "MONTHLY", "interval": 1})
    );
    assert!(!seen[0].headers.contains_key("idempotency-key"));
}

#[tokio::test]
async fn an_answer_with_a_numeric_amount_is_refused() {
    let (_server, client, _) = recording_gateway(
        |_, _| {
            Reply::json(
                200,
                r#"{"status":"success","data":{"id":"plan_1","price":9900}}"#,
            )
        },
        0,
    )
    .await;
    let result = client.subscriptions().plans().get("plan_1").await;
    assert!(matches!(result, Err(Error::Response(_))));
}

#[tokio::test]
async fn cards_add_refuses_card_numbers_without_echoing_them() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let cards = client.subscriptions().cards();
    let result = cards
        .add(
            &json!({"customerId": "cus_1", "paymentMethodToken": "pm_1", "cardNumber": "5400000000000004"}),
            None,
        )
        .await;
    let error = result.unwrap_err();
    assert!(matches!(error, Error::Invalid(_)));
    assert!(!format!("{error:?} {error}").contains("5400000000000004"));
    cards
        .add(
            &json!({"customerId": "cus_1", "paymentMethodToken": "pm_1", "supportedCurrencies": ["try", "usd"]}),
            Some("card-key-1"),
        )
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].body()["supportedCurrencies"], json!(["TRY", "USD"]));
    assert_eq!(seen[0].headers["idempotency-key"], "card-key-1");
}

#[tokio::test]
async fn card_and_invoice_operations() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let subscriptions = client.subscriptions();
    subscriptions.cards().set_default("card_1").await.unwrap();
    subscriptions
        .cards()
        .deactivate("card_1", Some("lost"))
        .await
        .unwrap();
    subscriptions
        .cards()
        .verify("card_1", &Value::Null)
        .await
        .unwrap();
    subscriptions.invoices().void("inv_1", None).await.unwrap();
    subscriptions
        .invoices()
        .retry_charge("inv_1", Some("retry-1"))
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    let calls: Vec<(&str, &str)> = seen
        .iter()
        .map(|call| (call.method.as_str(), call.path.as_str()))
        .collect();
    assert_eq!(
        calls,
        [
            ("POST", "/api/v1/subscriptions/cards/card_1/set-default"),
            ("DELETE", "/api/v1/subscriptions/cards/card_1"),
            ("POST", "/api/v1/subscriptions/cards/card_1/verify"),
            ("POST", "/api/v1/subscriptions/invoices/inv_1/void"),
            ("POST", "/api/v1/subscriptions/invoices/inv_1/retry-charge"),
        ]
    );
    assert_eq!(seen[1].body(), json!({"reason": "lost"}));
    assert!(seen[2].raw.is_empty());
    assert!(seen[3].raw.is_empty());
    assert_eq!(seen[4].headers["idempotency-key"], "retry-1");
}

/// A customer with every field the gateway requires.
fn customer() -> Value {
    json!({
        "externalId": "u_1",
        "name": "Ada",
        "surname": "Yilmaz",
        "email": "ada@example.com",
        "gsmNumber": "+905350000000",
        "identityNumber": "11111111111",
        "billingAddress": "Example Street 1",
        "billingCity": "Istanbul",
        "billingCountry": "Turkey",
        "billingZipCode": "34000",
    })
}

#[tokio::test]
async fn customer_upsert_requires_identity_and_billing_fields() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let customers = client.subscriptions().customers();
    for field in [
        "name",
        "surname",
        "email",
        "gsmNumber",
        "identityNumber",
        "billingAddress",
        "billingCity",
        "billingCountry",
        "billingZipCode",
    ] {
        let mut body = customer();
        body.as_object_mut().unwrap().remove(field);
        let result = customers.upsert(&body, None).await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{field}");
    }
    for (field, value) in [
        ("phone", json!("+905350000000")),
        ("locale", json!("tr")),
        ("metadata", json!({})),
    ] {
        let mut body = customer();
        body[field] = value;
        let result = customers.upsert(&body, None).await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{field}");
    }
    assert!(seen.lock().unwrap().is_empty());
    let mut shipping = customer();
    shipping["shippingCity"] = json!("Ankara");
    customers.upsert(&shipping, None).await.unwrap();
    assert_eq!(seen.lock().unwrap()[0].body()["shippingCity"], "Ankara");
    assert_eq!(
        subscription_customer_error_code::ALL,
        [
            "subscription.customer.field_required",
            "subscription.customer.field_invalid"
        ]
    );
}

#[tokio::test]
async fn customer_upsert_is_checked_locally() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let result = client
        .subscriptions()
        .customers()
        .upsert(&json!({"externalId": "u_1"}), Some("has space"))
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = client
        .subscriptions()
        .customers()
        .upsert(&json!({"email": "a@b.example"}), None)
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn customers_by_external_id() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let customers = client.subscriptions().customers();
    customers.upsert(&customer(), None).await.unwrap();
    customers.get_by_external_id("u/1").await.unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "PUT");
    assert_eq!(seen[0].path, "/api/v1/subscriptions/customers");
    assert_eq!(
        seen[1].path,
        "/api/v1/subscriptions/customers/by-external/u%2F1"
    );
}

#[tokio::test]
async fn portal_sdk_session_reads_the_proof_at_the_envelope_root() {
    let (_server, client, seen) = recording_gateway(
        |_, _| {
            Reply::json(
                200,
                r#"{"status":"success","portalToken":"pt_1","sessionProof":"sp","nonce":"n1","expiresAt":"2026-11-15T10:00:00Z"}"#,
            )
        },
        0,
    )
    .await;
    let portal = client.subscriptions().portal();
    let session = portal
        .create_sdk_session("cus_1", "https://shop.example", Some("HEADLESS"), None)
        .await
        .unwrap();
    assert_eq!(session["portalToken"], "pt_1");
    assert_eq!(session["proofNonce"], "n1");
    assert_eq!(session["proofExpiresAt"], "2026-11-15T10:00:00Z");
    let refused = portal
        .create_sdk_session("cus_1", "https://shop.example", Some("POPUP"), None)
        .await;
    assert!(matches!(refused, Err(Error::Invalid(_))));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].path, "/api/v1/subscriptions/portal/sdk-session");
    assert_eq!(
        seen[0].body(),
        json!({"customerId": "cus_1", "origin": "https://shop.example", "sdkMode": "HEADLESS"})
    );
}

#[tokio::test]
async fn portal_links_use_the_panel_routes() {
    let (_server, client, seen) = recording_gateway(
        |_, _| {
            Reply::json(
                200,
                r#"{"status":"success","data":{"id":"pl_1","token":"tok_1"}}"#,
            )
        },
        0,
    )
    .await;
    let portal = client.subscriptions().portal();
    let link = portal
        .create_portal_link("cus_1", &json!({"allowedPlanIds": ["plan_1"]}))
        .await
        .unwrap();
    assert_eq!(
        subscription_portal_url(&link, None),
        "https://portal.lunixi.com/?token=tok_1"
    );
    portal.revoke_link("pl_1", None).await.unwrap();
    portal.get_settings().await.unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].path, "/api/panel/v1/subscriptions/portal/links");
    assert_eq!(seen[0].body()["customerId"], "cus_1");
    assert_eq!(seen[1].method, "DELETE");
    assert_eq!(
        seen[1].path,
        "/api/panel/v1/subscriptions/portal/links/pl_1"
    );
    assert_eq!(seen[2].path, "/api/panel/v1/subscriptions/portal/settings");
}

#[tokio::test]
async fn catalog_feature_addon_and_usage_routes() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let subscriptions = client.subscriptions();
    subscriptions
        .products()
        .upsert_translation("prod_1", "en", &json!({"name": "Pro"}))
        .await
        .unwrap();
    subscriptions
        .features()
        .list(&json!({"productId": "prod_1", "anything": true}))
        .await
        .unwrap();
    subscriptions.features().delete("feat_1").await.unwrap();
    subscriptions.addons().deactivate("add_1").await.unwrap();
    subscriptions
        .usage()
        .record(&json!({"customerId": "cus_1", "featureKey": "api", "quantity": 3}))
        .await
        .unwrap();
    let refused = subscriptions
        .products()
        .list(&json!({"anything": true}))
        .await;
    assert!(matches!(refused, Err(Error::Invalid(_))));
    let refused = subscriptions.usage().record(&json!([1])).await;
    assert!(matches!(refused, Err(Error::Invalid(_))));
    let seen = seen.lock().unwrap();
    let calls: Vec<(&str, &str)> = seen
        .iter()
        .map(|call| (call.method.as_str(), call.path.as_str()))
        .collect();
    assert_eq!(
        calls,
        [
            (
                "PATCH",
                "/api/v1/subscriptions/products/prod_1/translations/en"
            ),
            (
                "GET",
                "/api/v1/subscriptions/features?productId=prod_1&anything=true"
            ),
            ("DELETE", "/api/v1/subscriptions/features/feat_1"),
            ("DELETE", "/api/v1/subscriptions/addons/add_1"),
            ("POST", "/api/v1/subscriptions/usage/record"),
        ]
    );
}
