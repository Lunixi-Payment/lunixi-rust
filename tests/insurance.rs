//! Insurance: key rules, local checks, binary policy files and the
//! reconciliation CSV, helpers and webhook readers against a local TLS
//! gateway double.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
mod common;
use common::{Incoming, Reply, recording_gateway, verified_event};
use lunixi::{Error, Insurance, InsuranceLeadWebhookData, InsuranceWebhookData, insurance_events};
use serde_json::{Value, json};

const KEY: &str = "ins-order-0001";
const PAYMENT: &str = "7b0c6a1e-2f8d-4c55-9a51-3f0e8d1c2a90";
const PDF: &str = "%PDF-1.7\nsynthetic policy\n%%EOF\n";

fn order(_: usize, _: &Incoming) -> Reply {
    Reply::json(
        200,
        r#"{"data":{"orderId":"inso_1","premiumAmount":"4500","premiumCollectedBy":"MERCHANT"}}"#,
    )
}
fn file(content_type: &'static str, body: &'static str) -> impl Fn(usize, &Incoming) -> Reply {
    move |_, _| Reply {
        status: 200,
        body: body.into(),
        headers: vec![("Content-Type".into(), content_type.into())],
    }
}
fn quote() -> Value {
    json!({
        "insuranceType": "TRAVEL_HEALTH",
        "currency": "try",
        "paymentIntentId": PAYMENT,
        "merchantOrderId": "ORD-1",
        "trip": {"startDate": "2026-12-01"},
    })
}

#[tokio::test]
async fn create_quote_and_order_are_signed_with_their_key_rules() {
    let (_server, client, seen) = recording_gateway(order, 0).await;
    let insurance = client.insurance();
    insurance.create_quote(&quote(), None).await.unwrap();
    let option = json!({"quoteId": "insq_1", "terms": {"version": "v3", "sha256": "abc"}});
    let consent = Insurance::consent_from_quote(&option, "2026-11-15T10:00:00Z").unwrap();
    insurance
        .create_order(&json!({"quoteId": "insq_1", "consent": consent}), KEY)
        .await
        .unwrap();
    insurance.issue_policy("inso_1", KEY).await.unwrap();
    insurance
        .refund_premium("inso_1", &Value::Null, KEY)
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].path, "/api/v1/insurance/quotes");
    assert!(!seen[0].headers.contains_key("idempotency-key"));
    assert_eq!(seen[0].body()["currency"], "TRY");
    assert_eq!(seen[1].path, "/api/v1/insurance/orders");
    assert_eq!(seen[1].headers["idempotency-key"], KEY);
    assert_eq!(
        seen[1].body()["consent"],
        json!({"termsVersion": "v3", "termsSha256": "abc", "acceptedAt": "2026-11-15T10:00:00Z"})
    );
    assert_eq!(seen[2].path, "/api/v1/insurance/orders/inso_1/issue-policy");
    assert!(seen[2].raw.is_empty());
    assert_eq!(
        seen[3].path,
        "/api/v1/insurance/orders/inso_1/refund-premium"
    );
    assert_eq!(seen[3].body(), json!({}));
}

#[tokio::test]
async fn local_errors_send_nothing() {
    let (_server, client, seen) = recording_gateway(order, 0).await;
    let insurance = client.insurance();
    let mut both = quote();
    both["context"] = json!({"items": [{"id": "b1"}]});
    let mut unknown = quote();
    unknown["premium"] = json!("100");
    let mut bad_uuid = quote();
    bad_uuid["paymentIntentId"] = json!("pay_1");
    for body in [both, unknown, bad_uuid] {
        let result = insurance.create_quote(&body, None).await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{body}");
    }
    let result = insurance.create_quote(&quote(), Some("short")).await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    for key in ["", "short", "has space key"] {
        let result = insurance.issue_policy("inso_1", key).await;
        assert!(matches!(result, Err(Error::Invalid(_))), "{key:?}");
    }
    let result = insurance
        .confirm_payment(
            "inso_1",
            &json!({"externalPaymentId": "x", "paidAmount": "0", "paidCurrency": "TRY", "paidAt": "2026-11-15T10:00:00Z"}),
            KEY,
        )
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = insurance
        .search_products(&json!({"currency": "TRY", "context": {"items": []}}))
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = insurance
        .create_lead(
            &json!({"insuranceType": "CARGO_TRANSIT", "note": "x".repeat(1001)}),
            KEY,
        )
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = insurance
        .list_products(&json!({"insuranceType": "X"}))
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = insurance
        .get_reconciliation_summary(&json!({"from": "2026-11-01T00:00:00Z"}))
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = insurance.list_leads(&json!({"status": "DONE"})).await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = insurance.list_orders(&json!({"limit": 101})).await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    let result = insurance
        .list_reference_values("PLANET", &Value::Null)
        .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn policy_document_is_pdf_bytes() {
    let (_server, client, seen) = recording_gateway(file("application/pdf", PDF), 0).await;
    let document = client
        .insurance()
        .get_policy_certificate("insp_1")
        .await
        .unwrap();
    assert_eq!(document.bytes, PDF.as_bytes());
    assert_eq!(document.content_type.as_deref(), Some("application/pdf"));
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen[0].path,
        "/api/v1/insurance/policies/insp_1/certificate"
    );
    assert_eq!(
        seen[0].headers["accept"],
        "application/pdf, application/json"
    );
}

#[tokio::test]
async fn a_json_error_on_a_document_route_is_an_api_error() {
    let (_server, client, _) = recording_gateway(
        |_, _| Reply::json(503, r#"{"code":"insurance.document.unavailable"}"#),
        0,
    )
    .await;
    let result = client.insurance().get_policy_document("insp_1").await;
    match result {
        Err(Error::Api { status, code, .. }) => {
            assert_eq!(status, 503);
            assert_eq!(code, "insurance.document.unavailable");
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn reconciliation_export_reads_the_trailer() {
    const CSV: &str = "\u{feff}orderId,premiumAmount\ninso_1,4500\n# truncated=true orderCap=20000 nextCursor=cur_2\n";
    let (_server, client, seen) = recording_gateway(file("text/csv", CSV), 0).await;
    let export = client
        .insurance()
        .export_reconciliation(
            &json!({"from": "2026-11-01T00:00:00Z", "to": "2026-12-01T00:00:00Z"}),
        )
        .await
        .unwrap();
    assert!(export.truncated);
    assert!(!export.aborted);
    assert_eq!(export.next_cursor.as_deref(), Some("cur_2"));
    assert_eq!(export.bytes, CSV.as_bytes());
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].headers["accept"], "text/csv, application/json");
}

#[test]
fn premium_basket_item_helper() {
    let order =
        json!({"orderId": "inso_1", "premiumAmount": "4500", "premiumCollectedBy": "MERCHANT"});
    let item =
        Insurance::premium_basket_item(&order, "Travel insurance", "Insurance", None).unwrap();
    assert_eq!(item.id, "inso_1");
    assert_eq!(item.price, "4500");
    assert_eq!(item.item_type, "VIRTUAL");
    let provider =
        json!({"orderId": "inso_1", "premiumAmount": "4500", "premiumCollectedBy": "PROVIDER"});
    assert!(Insurance::premium_basket_item(&provider, "x", "y", None).is_err());
    assert!(
        Insurance::consent_from_quote(&json!({"quoteId": "q"}), "2026-11-15T10:00:00Z").is_err()
    );
}

#[test]
fn webhook_readers() {
    let event = verified_event(&json!({
        "id": "evt_ins_1",
        "type": insurance_events::POLICY_ISSUED,
        "data": {
            "eventId": "evt_ins_1", "insuranceOrderId": "inso_1", "premiumAmount": "4500",
            "currency": "TRY", "status": "ACTIVE",
            "policies": [{"policyId": "insp_1", "policyStatus": "ISSUED", "documentUrl": ""}],
        },
    }));
    let data = InsuranceWebhookData::from_event(&event).unwrap();
    assert_eq!(data.insurance_order_id, "inso_1");
    assert_eq!(data.premium_amount.as_deref(), Some("4500"));
    assert_eq!(data.policies[0].policy_id, "insp_1");
    assert_eq!(data.policies[0].document_url, None);
    assert!(InsuranceLeadWebhookData::from_event(&event).is_err());

    let lead = verified_event(&json!({
        "id": "evt_ins_2",
        "type": insurance_events::LEAD_SUBMITTED,
        "data": {"eventId": "evt_ins_2", "lead": {"leadId": "insl_1", "status": "SUBMITTED", "schemaVersion": 1}},
    }));
    let data = InsuranceLeadWebhookData::from_event(&lead).unwrap();
    assert_eq!(data.lead.lead_id, "insl_1");
    assert_eq!(data.lead.schema_version, Some(1));
    assert!(InsuranceWebhookData::from_event(&lead).is_err());
}
