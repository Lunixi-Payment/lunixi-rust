//! Bank accounts, transfers and FX: filter rules, transfer keys and
//! retries, body checks and webhook readers against a local TLS gateway
//! double.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
mod common;
use common::{Incoming, Reply, recording_gateway, verified_event};
use lunixi::{
    BankAccountTransactionReceived, BankTransferWebhookData, Error, bank_connect_events,
    bank_transfer_events, payment_bank_transfer_events,
};
use serde_json::{Value, json};

const PAGE: &str = r#"{"data":{"items":[{"accountId":"a1"}]},"pageInfo":{"hasMore":true,"nextCursor":"c2","totalCount":null}}"#;
const CREATED: &str = r#"{"data":{"transfer":{"id":"t1","status":"SUBMITTED","amountMinor":"150075"},"stepUp":null,"replayed":false}}"#;

fn page(_: usize, _: &Incoming) -> Reply {
    Reply::json(200, PAGE)
}
fn created(_: usize, _: &Incoming) -> Reply {
    Reply::json(201, CREATED)
}
fn transfer() -> Value {
    json!({
        "accountId": "a1",
        "receiverIban": "TR330006100519786457841326",
        "receiverName": "Ada Lovelace",
        "amountMinor": "150075",
        "currency": "try",
        "paymentPurpose": "OTHER_01",
    })
}

#[tokio::test]
async fn list_transactions_sends_checked_filters_signed() {
    let (_server, client, seen) = recording_gateway(page, 0).await;
    let result = client
        .bank_connect()
        .list_transactions(&json!({
            "accountId": ["a1", "a2"],
            "bookedFrom": "2026-11-01T00:00:00Z",
            "bookedTo": "2026-11-15T00:00:00Z",
            "direction": "CREDIT",
            "canonicalType": ["TRANSFER_IN", "FEE"],
            "minAmountMinor": "0",
            "maxAmountMinor": "500000",
            "limit": 50,
        }))
        .await
        .unwrap();
    assert!(result.has_more);
    assert_eq!(result.next_cursor.as_deref(), Some("c2"));
    assert_eq!(result.total_count, None);
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "GET");
    assert_eq!(
        seen[0].path,
        "/api/v1/bank-connect/transactions?limit=50&accountId=a1%2Ca2&bookedFrom=2026-11-01T00%3A00%3A00Z&bookedTo=2026-11-15T00%3A00%3A00Z&direction=CREDIT&minAmountMinor=0&maxAmountMinor=500000&canonicalType=TRANSFER_IN%2CFEE"
    );
}

#[tokio::test]
async fn filter_rules_fail_before_sending() {
    let (_server, client, seen) = recording_gateway(page, 0).await;
    let bank = client.bank_connect();
    let ids: Vec<String> = (0..101).map(|i| format!("a{i}")).collect();
    let refused = [
        json!({"limit": 0}),
        json!({"limit": 101}),
        json!({"limit": "10"}),
        json!({"environment": "LIVE"}),
        json!({"direction": "IN"}),
        json!({"canonicalType": ["FEE", "BONUS"]}),
        json!({"accountId": ids}),
        json!({"accountId": "a1,a2"}),
        json!({"minAmountMinor": "10.5"}),
        json!({"minAmountMinor": 100}),
        json!({"minAmountMinor": "900", "maxAmountMinor": "100"}),
        json!({"bookedTo": "2026-11-15T00:00:00Z"}),
        json!({"bookedFrom": "2026-11-15T00:00:00Z", "bookedTo": "2026-11-01T00:00:00Z"}),
        json!({"bookedFrom": "2026-01-01T00:00:00Z", "bookedTo": "2026-06-01T00:00:00Z"}),
    ];
    for filters in refused {
        let result = bank.list_transactions(&filters).await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{filters}");
    }
    let result = bank.list_accounts(&json!({"status": "DEGRADED"})).await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = bank.list_accounts(&json!({"currency": "TL"})).await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    assert!(seen.lock().unwrap().is_empty());
    bank.list_connections(&json!({"status": ["ACTIVE", "DEGRADED"]}))
        .await
        .unwrap();
    bank.list_accounts(&json!({"currency": " try "}))
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen[0].path,
        "/api/v1/bank-connect/connections?status=ACTIVE%2CDEGRADED"
    );
    assert_eq!(seen[1].path, "/api/v1/bank-connect/accounts?currency=TRY");
}

#[tokio::test]
async fn request_sync_posts_without_a_body() {
    let (_server, client, seen) = recording_gateway(
        |_, _| Reply::json(202, r#"{"data":{"accountId":"a1","syncStatus":"QUEUED"}}"#),
        0,
    )
    .await;
    let data = client.bank_connect().request_sync("a1").await.unwrap();
    assert_eq!(data["syncStatus"], "QUEUED");
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/api/v1/bank-connect/accounts/a1/sync");
    assert!(seen[0].raw.is_empty());
}

#[tokio::test]
async fn create_transfer_generates_a_uuid_key_and_normalizes_the_body() {
    let (_server, client, seen) = recording_gateway(created, 0).await;
    let data = client
        .bank_connect()
        .transfers()
        .create(&transfer(), None)
        .await
        .unwrap();
    assert_eq!(data["transfer"]["id"], "t1");
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].path, "/api/v1/bank-connect/transfers");
    let key = &seen[0].headers["idempotency-key"];
    assert_eq!(key.len(), 36);
    assert_eq!(key.matches('-').count(), 4);
    let body = seen[0].body();
    assert_eq!(body["currency"], "TRY");
    assert_eq!(body["amountMinor"], "150075");
}

#[tokio::test]
async fn a_5xx_is_retried_with_the_same_key_and_body() {
    let (_server, client, seen) = recording_gateway(
        |call, _| {
            if call == 0 {
                Reply::json(503, r#"{"code":"SERVICE_UNAVAILABLE"}"#)
            } else {
                Reply::json(201, CREATED)
            }
        },
        1,
    )
    .await;
    client
        .bank_connect()
        .transfers()
        .create(&transfer(), Some("payout-2026-000123"))
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].headers["idempotency-key"], "payout-2026-000123");
    assert_eq!(seen[1].headers["idempotency-key"], "payout-2026-000123");
    assert_eq!(seen[0].raw, seen[1].raw);
}

#[tokio::test]
async fn transfer_and_fx_bodies_are_checked_before_sending() {
    let (_server, client, seen) = recording_gateway(created, 0).await;
    let transfers = client.bank_connect().transfers();
    let mut bodies = vec![];
    for (field, value) in [
        ("environment", json!("LIVE")),
        ("amountMinor", json!("0")),
        ("amountMinor", json!(150075)),
        ("receiverName", json!("A")),
        ("receiverIban", json!("TR".repeat(22))),
        ("paymentPurpose", json!("bad purpose")),
        ("description", json!("x".repeat(141))),
        ("currency", json!("TURKISH")),
    ] {
        let mut body = transfer();
        body[field] = value;
        bodies.push(body);
    }
    let mut missing = transfer();
    missing.as_object_mut().unwrap().remove("accountId");
    bodies.push(missing);
    for body in bodies {
        let result = transfers.create(&body, None).await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{body}");
    }
    for key in ["short", "with space key", &"k".repeat(201)] {
        let result = transfers.create(&transfer(), Some(key)).await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{key:?}");
    }
    let fx = client.bank_connect().fx();
    for quote in [
        json!({"sourceAccountId": "a1", "targetAccountId": "a2"}),
        json!({"sourceAccountId": "a1", "targetAccountId": "a2", "sourceAmountMinor": "100", "targetAmountMinor": "100"}),
        json!({"sourceAccountId": "a1", "targetAccountId": "a1", "sourceAmountMinor": "100"}),
    ] {
        let result = fx.create_quote(&quote).await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{quote}");
    }
    let result = fx
        .create_order(
            &json!({"quoteId": "q1", "description": "x".repeat(141)}),
            None,
        )
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = transfers.list(&json!({"status": "DONE"})).await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = fx.list_orders(&json!({"status": "PROCESSING"})).await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn fx_flow_paths() {
    let (_server, client, seen) = recording_gateway(created, 0).await;
    let fx = client.bank_connect().fx();
    fx.list_rates("c1").await.unwrap();
    fx.create_quote(
        &json!({"sourceAccountId": "a1", "targetAccountId": "a2", "targetAmountMinor": "10000"}),
    )
    .await
    .unwrap();
    fx.create_order(&json!({"quoteId": "q1"}), Some("fx-order-0001"))
        .await
        .unwrap();
    fx.list_orders(&json!({"status": ["SUBMITTED", "FAILED"], "limit": 10}))
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen[0].path,
        "/api/v1/bank-connect/fx/rates?connectionId=c1"
    );
    assert_eq!(seen[1].path, "/api/v1/bank-connect/fx/quotes");
    assert!(!seen[1].headers.contains_key("idempotency-key"));
    assert_eq!(seen[2].path, "/api/v1/bank-connect/fx/orders");
    assert_eq!(seen[2].headers["idempotency-key"], "fx-order-0001");
    assert_eq!(
        seen[3].path,
        "/api/v1/bank-connect/fx/orders?limit=10&status=SUBMITTED%2CFAILED"
    );
}

#[test]
fn webhook_readers() {
    let event = verified_event(&json!({
        "id": "evt_bt_1",
        "type": bank_transfer_events::COMPLETED,
        "data": {
            "transferId": "t1", "status": "COMPLETED", "rail": "FAST",
            "amountMinor": "150075", "currency": "TRY", "failureCode": null,
        },
    }));
    let data = BankTransferWebhookData::from_event(&event).unwrap();
    assert_eq!(data.transfer_id, "t1");
    assert_eq!(data.rail.as_deref(), Some("FAST"));
    assert_eq!(data.failure_code, None);
    assert!(BankAccountTransactionReceived::from_event(&event).is_err());

    let event = verified_event(&json!({
        "id": "evt_tx_1",
        "type": bank_connect_events::TRANSACTION_RECEIVED,
        "data": {
            "transactionId": "tx1", "accountId": "a1", "direction": "CREDIT",
            "amountMinor": "2500", "currency": "TRY", "valueAt": null,
        },
    }));
    let data = BankAccountTransactionReceived::from_event(&event).unwrap();
    assert_eq!(data.amount_minor, "2500");
    assert_eq!(data.value_at, None);

    let payment = verified_event(&json!({
        "id": "evt_p_1",
        "type": payment_bank_transfer_events::UNDERPAID,
        "data": {"transferId": "t1", "status": "x", "amountMinor": "1", "currency": "TRY"},
    }));
    assert!(matches!(
        BankTransferWebhookData::from_event(&payment),
        Err(Error::Invalid(_))
    ));
    let bad_amount = verified_event(&json!({
        "id": "evt_bt_2",
        "type": bank_transfer_events::FAILED,
        "data": {"transferId": "t1", "status": "FAILED", "amountMinor": 1, "currency": "TRY"},
    }));
    assert!(BankTransferWebhookData::from_event(&bad_amount).is_err());
}
