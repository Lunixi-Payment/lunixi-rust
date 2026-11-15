//! Wallet: route-table checks, signed requests, keys and binary QR images
//! against a local TLS gateway double.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
mod common;
use common::{Incoming, Reply, recording_gateway};
use lunixi::{Error, WALLET_ROUTES, wallet_route};
use serde_json::{Value, json};

const KEY: &str = "wallet-key-1";

fn ok(_: usize, _: &Incoming) -> Reply {
    Reply::json(200, r#"{"data":{"operationId":"op_1"}}"#)
}

fn transfer() -> Value {
    json!({
        "endUserId": "eu_1",
        "sourceWalletAccountId": "wa_1",
        "amount": "12550",
        "currency": "TRY",
        "destinationWalletNo": "1000000001",
    })
}

#[test]
fn route_table_lookup() {
    assert_eq!(WALLET_ROUTES.len(), 60);
    let route = wallet_route("completeW2W").unwrap();
    assert_eq!(route.method, "POST");
    assert!(route.idempotency_key);
    assert!(route.accepts("integrityHash"));
    assert!(wallet_route("getQrImage").unwrap().binary);
    assert!(wallet_route("nope").is_none());
}

#[tokio::test]
async fn initiate_sends_the_checked_body_signed_with_the_key() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let data = client
        .wallet()
        .transfers()
        .initiate_w2w(&transfer(), &format!("  {KEY} "))
        .await
        .unwrap();
    assert_eq!(data["operationId"], "op_1");
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/api/v1/wallet/transfers/w2w");
    assert_eq!(seen[0].headers["idempotency-key"], KEY);
    assert_eq!(seen[0].body()["amount"], "12550");
}

#[tokio::test]
async fn complete_puts_the_operation_id_in_the_body() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let request =
        json!({"endUserId": "eu_1", "integrityHash": "h", "nonce": "n", "otpCode": "123456"});
    client
        .wallet()
        .transfers()
        .complete_w2_iban("op_1", &request, KEY)
        .await
        .unwrap();
    let mismatch =
        json!({"endUserId": "eu_1", "operationId": "op_2", "integrityHash": "h", "nonce": "n"});
    let refused = client
        .wallet()
        .withdrawals()
        .complete("op_1", &mismatch, KEY)
        .await;
    assert!(matches!(refused, Err(Error::Invalid(_))));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].path,
        "/api/v1/wallet/transfers/w2iban/op_1/complete"
    );
    assert_eq!(seen[0].body()["operationId"], "op_1");
}

#[tokio::test]
async fn reads_are_signed_with_filters_and_encoded_ids() {
    let (_server, client, seen) = recording_gateway(
        |_, _| {
            Reply::json(
                200,
                r#"{"data":{"items":[{"id":"eu_1"}],"nextCursor":"c2"},"pageInfo":{"hasMore":true,"nextCursor":"c2"}}"#,
            )
        },
        0,
    )
    .await;
    let page = client
        .wallet()
        .end_users()
        .list(&json!({"programId": "pr_1", "limit": 20, "status": null}))
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.has_more);
    assert_eq!(page.next_cursor.as_deref(), Some("c2"));
    client
        .wallet()
        .end_users()
        .get_by_external_id("cust/1", Some("pr_1"))
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "GET");
    assert_eq!(
        seen[0].path,
        "/api/v1/wallet/end-users?programId=pr_1&limit=20"
    );
    assert!(!seen[0].headers.contains_key("idempotency-key"));
    assert_eq!(
        seen[1].path,
        "/api/v1/wallet/end-users/by-external/cust%2F1?programId=pr_1"
    );
}

#[tokio::test]
async fn local_errors_send_nothing() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let wallet = client.wallet();
    let mut unknown = transfer();
    unknown["businessAccountId"] = json!("ba_1");
    let mut missing = transfer();
    missing.as_object_mut().unwrap().remove("currency");
    let mut decimal = transfer();
    decimal["amount"] = json!("125.50");
    let mut number = transfer();
    number["amount"] = json!(12550);
    for body in [unknown, missing, decimal, number] {
        let result = wallet.transfers().initiate_w2w(&body, KEY).await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{body}");
    }
    for key in ["", "   ", &"k".repeat(129), "bad\nkey"] {
        let result = wallet.transfers().initiate_w2w(&transfer(), key).await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{key:?}");
    }
    let rows = json!([{"phone": "+905550000000", "salary": "1"}]);
    let result = wallet
        .corporate()
        .import_employees("ca_1", &rows, KEY)
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let items = json!([{"targetType": "IBAN", "targetRef": "TR00", "amount": "0"}]);
    let batch =
        json!({"programId": "p", "sourceWalletAccountId": "w", "currency": "TRY", "items": items});
    let result = wallet.bulk_payouts().create(&batch, KEY).await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = wallet.end_users().list(&json!({"q": "x"})).await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = wallet.qr().image("qr_1", Some("gif"), None).await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = wallet.qr().image("qr_1", None, Some(21)).await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = wallet.end_users().get("").await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn qr_image_is_returned_as_bytes() {
    const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
    let (_server, client, seen) = recording_gateway(
        |_, _| Reply {
            status: 200,
            body: SVG.into(),
            headers: vec![("Content-Type".into(), "image/svg+xml".into())],
        },
        0,
    )
    .await;
    let image = client
        .wallet()
        .qr()
        .image("qr_1", Some("svg"), Some(4))
        .await
        .unwrap();
    assert_eq!(image.bytes, SVG.as_bytes());
    assert_eq!(image.content_type.as_deref(), Some("image/svg+xml"));
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen[0].path,
        "/api/v1/wallet/qr/qr_1/image?format=svg&scale=4"
    );
    assert_eq!(
        seen[0].headers["accept"],
        "image/svg+xml, image/png, application/json"
    );
}

#[tokio::test]
async fn bulk_payout_approval_and_bodiless_posts() {
    let (_server, client, seen) = recording_gateway(ok, 0).await;
    let bulk = client.wallet().bulk_payouts();
    bulk.approve("b_1", false, Some("wrong total"))
        .await
        .unwrap();
    bulk.submit("b_1").await.unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].path, "/api/v1/wallet/bulk-payouts/b_1/approve");
    assert_eq!(
        seen[0].body(),
        json!({"approve": false, "reason": "wrong total"})
    );
    assert_eq!(seen[1].path, "/api/v1/wallet/bulk-payouts/b_1/submit");
    assert!(seen[1].raw.is_empty());
}
