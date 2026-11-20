//! Marketplace: split charges (legs, totals, card data kept out of errors),
//! the dealer lifecycle, payouts and commissions against a local TLS
//! gateway double.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
mod common;
use common::{Incoming, Reply, recording_gateway, with_order_details};
use lunixi::{
    Error, MARKETPLACE_ROUTES, marketplace_multi_seller_policy, marketplace_onboarding_url,
};
use serde_json::{Value, json};

const KEY: &str = "marketplace-key-0001";
const PAN: &str = "5400000000000004";

fn ok(_: usize, _: &Incoming) -> Reply {
    Reply::json(
        200,
        r#"{"status":"success","data":{"paymentId":"pay_1","status":"SUCCEEDED","token":"tok_1"}}"#,
    )
}

fn legs() -> Value {
    json!([
        {"subDealerId": "d_1", "amountMinor": "600"},
        {"subDealerId": "d_2", "amountMinor": "400", "scenario": {"commissionRuleId": "r_1"}},
    ])
}

fn charge_request() -> Value {
    with_order_details(
        json!({
            "amount": "1000",
            "currency": "TRY",
            "orderId": "o_1",
            "card": {"cardHolderName": "Ada Lovelace", "cardNumber": PAN, "expireMonth": "12", "expireYear": "2030", "cvc": "123"},
        }),
        "1000",
    )
}

/// A checkout intent body with the order details (the buyer IP left out).
fn checkout_request() -> Value {
    let mut body = with_order_details(
        json!({"amount": "1000", "currency": "TRY", "orderId": "o_1"}),
        "1000",
    );
    body["buyer"]
        .as_object_mut()
        .map(|buyer| buyer.remove("ip"));
    body
}

#[test]
fn route_table_and_onboarding_url() {
    assert_eq!(MARKETPLACE_ROUTES.len(), 26);
    let signed: Vec<&str> = MARKETPLACE_ROUTES
        .iter()
        .filter(|route| route.step_up)
        .map(|route| route.name)
        .collect();
    assert_eq!(
        signed,
        [
            "createCheckoutIntent",
            "chargeMarketplaceCard2d",
            "chargeMarketplaceCard3d"
        ]
    );
    assert_eq!(
        marketplace_onboarding_url(&json!({"url": "https://x.example/o"}), None),
        "https://x.example/o"
    );
    assert_eq!(
        marketplace_onboarding_url(&json!({"token": "t 1"}), None),
        "https://altbayi.lunixi.com/m/t%201"
    );
}

#[tokio::test]
async fn checkout_intent_sends_the_split_with_the_key() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let response = client
        .marketplace()
        .create_checkout_intent(&checkout_request(), &legs(), None, KEY)
        .await
        .unwrap();
    assert_eq!(response.data.unwrap()["token"], "tok_1");
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/api/v1/payments/marketplace/intents");
    assert_eq!(seen[0].headers["idempotency-key"], KEY);
    let body = seen[0].body();
    assert_eq!(body["multiSellerPolicy"], "ALL_OR_NOTHING");
    assert_eq!(body["sellers"][1]["scenario"]["commissionRuleId"], "r_1");
}

#[tokio::test]
async fn checkout_intent_without_a_token_is_an_error() {
    let (_server, client, _) = recording_gateway(
        |_, _| {
            Reply::json(
                200,
                r#"{"status":"success","code":"SELLER_NOT_ELIGIBLE","data":{"paymentId":"pay_1"}}"#,
            )
        },
        0,
    )
    .await;
    let result = client
        .marketplace()
        .create_checkout_intent(&checkout_request(), &legs(), None, KEY)
        .await;
    match result {
        Err(Error::Api { code, .. }) => assert_eq!(code, "SELLER_NOT_ELIGIBLE"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn seller_legs_are_checked_before_sending() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let marketplace = client.marketplace();
    let intent = checkout_request();
    for (sellers, policy) in [
        (json!([]), None),
        (json!([{"subDealerId": "d_1", "amountMinor": "0"}]), None),
        (json!([{"subDealerId": "d_1", "amountMinor": 1000}]), None),
        (json!([{"subDealerId": "d_1", "amountMinor": "999"}]), None),
        (json!([{"subDealerId": " ", "amountMinor": "1000"}]), None),
        (
            json!([{"subDealerId": "d_1", "amountMinor": "1000", "iban": "TR00"}]),
            None,
        ),
        (
            json!([{"subDealerId": "d_1", "amountMinor": "1000", "scenario": "x"}]),
            None,
        ),
        (
            json!([{"subDealerId": "d_1", "amountMinor": "1000"}]),
            Some("SOME"),
        ),
    ] {
        let result = marketplace
            .create_checkout_intent(&intent, &sellers, policy, KEY)
            .await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{sellers}");
    }
    let mut inline = checkout_request();
    inline["sellers"] = json!([]);
    let result = marketplace
        .create_checkout_intent(&inline, &legs(), None, KEY)
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = marketplace
        .create_checkout_intent(&intent, &legs(), None, "short")
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn card_charges_go_to_their_route_with_the_legs() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let marketplace = client.marketplace();
    marketplace
        .charge_marketplace_card2d(
            &charge_request(),
            &legs(),
            Some(marketplace_multi_seller_policy::PARTIAL_ALLOWED),
            KEY,
        )
        .await
        .unwrap();
    let mut three_ds = charge_request();
    three_ds["callbackUrl"] = json!("https://shop.example/3ds");
    marketplace
        .charge_marketplace_card3d(&three_ds, &legs(), None, KEY)
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].path, "/api/v1/payments/marketplace/2d");
    assert_eq!(seen[1].path, "/api/v1/payments/marketplace/3d");
    assert_eq!(seen[0].headers["idempotency-key"], KEY);
    let body = seen[0].body();
    assert_eq!(body["multiSellerPolicy"], "PARTIAL_ALLOWED");
    assert_eq!(body["sellers"].as_array().unwrap().len(), 2);
    assert_eq!(body["card"]["cardNumber"], PAN);
}

#[tokio::test]
async fn a_bad_charge_is_refused_without_echoing_the_card() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let marketplace = client.marketplace();
    let mut no_buyer = charge_request();
    no_buyer.as_object_mut().unwrap().remove("buyer");
    let mut priced = charge_request();
    priced["price"] = json!("100000");
    let mut preauth = charge_request();
    preauth["preauth"] = json!("false");
    let mut empty_basket = charge_request();
    empty_basket["basketItems"] = json!([]);
    let mut errors = vec![];
    for request in [no_buyer, priced, preauth, empty_basket] {
        errors.push(
            marketplace
                .charge_marketplace_card2d(&request, &legs(), None, KEY)
                .await
                .unwrap_err(),
        );
    }
    let mut numeric = charge_request();
    numeric["amount"] = json!(1000);
    errors.push(
        marketplace
            .charge_marketplace_card(&numeric, false, KEY)
            .await
            .unwrap_err(),
    );
    for error in errors {
        assert!(matches!(error, Error::Invalid(_)), "{error:?}");
        let shown = format!("{error:?} {error}");
        assert!(!shown.contains(PAN) && !shown.contains("123"), "{shown}");
    }
    let mut physical = charge_request();
    physical["basketItems"][0]["itemType"] = json!("PHYSICAL");
    physical["buyer"]
        .as_object_mut()
        .unwrap()
        .remove("identityNumber");
    let error = marketplace
        .charge_marketplace_card2d(&physical, &legs(), None, KEY)
        .await
        .unwrap_err();
    assert_eq!(
        error,
        Error::MissingOrderDetails {
            operation: "A marketplace card charge",
            fields: vec!["buyer.identityNumber".into(), "shippingAddress".into()],
        }
    );
    let shown = format!("{error:?} {error}");
    assert!(!shown.contains(PAN) && !shown.contains("123"), "{shown}");
    let mut incomplete = checkout_request();
    incomplete["billingAddress"]
        .as_object_mut()
        .unwrap()
        .remove("zipCode");
    assert_eq!(
        marketplace
            .create_checkout_intent(&incomplete, &legs(), None, KEY)
            .await
            .unwrap_err(),
        Error::MissingOrderDetails {
            operation: "A marketplace payment intent",
            fields: vec!["billingAddress.zipCode".into()],
        }
    );
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn dealer_lifecycle_routes() {
    let (_server, client, seen) = recording_gateway(
        |_, _| {
            Reply::json(
                200,
                r#"{"status":"success","token":"onb_1","data":{"id":"d_1","url":"https://altbayi.lunixi.com/m/onb_1"}}"#,
            )
        },
        0,
    )
    .await;
    let marketplace = client.marketplace();
    marketplace
        .create_dealer(&json!({"legalName": "Shop Ltd", "dealerType": "LEGAL_ENTITY"}))
        .await
        .unwrap();
    marketplace
        .bulk_create_dealers(&[json!({"legalName": "A"}), json!({"legalName": "B"})])
        .await
        .unwrap();
    marketplace
        .update_dealer_status("d_1", "SUSPENDED")
        .await
        .unwrap();
    let link = marketplace.onboarding_token("d_1").await.unwrap();
    assert_eq!(link["token"], "onb_1");
    assert_eq!(
        marketplace_onboarding_url(&link, None),
        "https://altbayi.lunixi.com/m/onb_1"
    );
    marketplace
        .submit_dealer_application("d_1", &Value::Null)
        .await
        .unwrap();
    marketplace.activate_dealer("d_1").await.unwrap();
    for refused in [
        marketplace
            .create_dealer(&json!({"legalName": "X", "dealerType": "PARTNERSHIP"}))
            .await,
        marketplace.create_dealer(&json!({"taxId": "1"})).await,
        marketplace.update_dealer_status("d_1", "ARCHIVED").await,
        marketplace.bulk_create_dealers(&[]).await,
    ] {
        assert!(matches!(refused, Err(Error::Invalid(_))));
    }
    let seen = seen.lock().unwrap();
    let calls: Vec<(&str, &str)> = seen
        .iter()
        .map(|call| (call.method.as_str(), call.path.as_str()))
        .collect();
    assert_eq!(
        calls,
        [
            ("POST", "/api/v1/marketplace/dealers"),
            ("POST", "/api/v1/marketplace/dealers/bulk"),
            ("PUT", "/api/v1/marketplace/dealers/d_1/status"),
            ("POST", "/api/v1/marketplace/dealers/d_1/onboarding-token"),
            ("POST", "/api/v1/marketplace/dealers/d_1/submit"),
            ("POST", "/api/v1/marketplace/dealers/d_1/activate"),
        ]
    );
    assert_eq!(seen[1].body()["dealers"].as_array().unwrap().len(), 2);
    assert_eq!(seen[2].body(), json!({"status": "SUSPENDED"}));
    assert_eq!(seen[4].body(), json!({}));
    assert!(
        seen.iter()
            .all(|call| !call.headers.contains_key("idempotency-key"))
    );
}

#[tokio::test]
async fn list_dealers_reads_the_items_envelope() {
    let (_server, client, seen) = recording_gateway(
        |_, _| {
            Reply::json(
                200,
                r#"{"status":"success","data":{"items":[{"id":"d_1","status":"ACTIVE"}],"total":1}}"#,
            )
        },
        0,
    )
    .await;
    let page = client
        .marketplace()
        .list_dealers(&json!({"status": "ACTIVE", "limit": 20, "offset": 0}))
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.total_count, Some(1));
    assert!(!page.has_more);
    assert_eq!(
        seen.lock().unwrap()[0].path,
        "/api/v1/marketplace/dealers?status=ACTIVE&limit=20&offset=0"
    );
}

#[tokio::test]
async fn payouts_commissions_and_the_audit_log() {
    let (_server, client, seen) = recording_gateway(
        |_, _| {
            Reply::json(
                200,
                r#"{"status":"success","data":{"items":[{"id":"po_1","amountMinor":"5000"}]}}"#,
            )
        },
        0,
    )
    .await;
    let marketplace = client.marketplace();
    marketplace
        .create_payout(&json!({"subDealerId": "d_1", "amountMinor": "5000", "currency": "TRY"}))
        .await
        .unwrap();
    marketplace
        .bulk_mark_payouts_paid(&["po_1", "po_2"], &json!({"reference": "batch-1"}))
        .await
        .unwrap();
    marketplace
        .resolve_commission("d_1", Some("BOOKS"), Some("250000"))
        .await
        .unwrap();
    marketplace
        .payout_ledger(&json!({"days": 30}))
        .await
        .unwrap();
    marketplace.verify_audit_log().await.unwrap();
    for refused in [
        marketplace
            .create_payout(&json!({"subDealerId": "d_1", "amountMinor": "0"}))
            .await
            .map(|_| ()),
        marketplace
            .create_payout(&json!({"subDealerId": "d_1", "amountMinor": 5000}))
            .await
            .map(|_| ()),
        marketplace
            .resolve_commission("d_1", None, Some("12.5"))
            .await
            .map(|_| ()),
        marketplace
            .bulk_mark_payouts_paid(&[], &Value::Null)
            .await
            .map(|_| ()),
        marketplace
            .audit_log(&json!({"since": "2026-01-01"}))
            .await
            .map(|_| ()),
    ] {
        assert!(matches!(refused, Err(Error::Invalid(_))));
    }
    let seen = seen.lock().unwrap();
    let paths: Vec<&str> = seen.iter().map(|call| call.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/api/v1/marketplace/payouts",
            "/api/v1/marketplace/payouts/bulk-mark-paid",
            "/api/v1/marketplace/commission/resolve?subDealerId=d_1&categoryCode=BOOKS&mtdVolumeMinor=250000",
            "/api/v1/marketplace/payout-ledger?days=30",
            "/api/v1/marketplace/audit-log/verify",
        ]
    );
    assert_eq!(
        seen[1].body(),
        json!({"reference": "batch-1", "payoutIds": ["po_1", "po_2"]})
    );
}

#[tokio::test]
async fn typed_sellers_serialize_to_the_same_legs() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let scenario = json!({"commissionRuleId": "r_1"})
        .as_object()
        .cloned()
        .unwrap();
    let sellers = serde_json::to_value(vec![
        lunixi::MarketplaceSeller::new("d_1", "600"),
        lunixi::MarketplaceSeller::new("d_2", "400").with_scenario(scenario),
    ])
    .unwrap();
    client
        .marketplace()
        .create_checkout_intent(&checkout_request(), &sellers, None, KEY)
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].body()["sellers"], legs());
}
