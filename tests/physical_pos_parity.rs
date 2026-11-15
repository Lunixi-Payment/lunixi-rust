//! Physical POS parity with the shared fixture (`fixtures/physical-pos-parity.json`,
//! byte-identical to the Node, PHP, Java and Python copies): card-data names
//! and paths, card-number values, idempotency keys, field and filter lists,
//! path identifiers, event types and the webhook readers.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
mod common;
use common::{Reply, Server};
use lunixi::{
    Error, PHYSICAL_POS_CARD_DATA_KEY_RULE, PHYSICAL_POS_EVENT_TYPES, PHYSICAL_POS_FILTERS,
    PHYSICAL_POS_NOT_STARTED_CODES, PHYSICAL_POS_PAYMENT_TARGET_FIELDS,
    PHYSICAL_POS_REFUND_ITEM_FIELDS, PHYSICAL_POS_REQUEST_FIELDS, PHYSICAL_POS_SALE_ITEM_FIELDS,
    PhysicalPosCashier, PhysicalPosOriginal, physical_pos_category_of,
    physical_pos_contains_pan_like_value, physical_pos_find_card_data_field,
    physical_pos_is_card_data_key, physical_pos_is_valid_idempotency_key,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

const TOKEN: &str = r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#;
const KEY: &str = "7b0c6a1e-2f8d-4c55-9a51-3f0e8d1c2a90";

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/physical-pos-parity.json")).unwrap()
}
fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}
fn sale() -> Value {
    json!({
        "operation": "SALE",
        "target": {"deviceId": "dev_1"},
        "amountMinor": "4500",
        "currency": "TRY",
    })
}

/// Records every path; nothing but the token is answered with data.
async fn recording() -> (Server, Arc<Mutex<Vec<String>>>) {
    let seen = Arc::new(Mutex::new(vec![]));
    let log = seen.clone();
    let server = Server::start(move |r| {
        log.lock().unwrap().push(r.path.clone());
        if r.path == "/api/v1/auth/token" {
            return Reply::json(200, TOKEN);
        }
        Reply::json(200, r#"{"data":{"payment":{"paymentId":"p1"}}}"#)
    })
    .await;
    (server, seen)
}

#[test]
fn card_data_key_rule_matches_the_fixture() {
    let rule = &fixture()["cardDataKeyRule"];
    assert_eq!(
        PHYSICAL_POS_CARD_DATA_KEY_RULE.number_terms,
        strings(&rule["numberTerms"])
    );
    assert_eq!(
        PHYSICAL_POS_CARD_DATA_KEY_RULE.secret_terms,
        strings(&rule["secretTerms"])
    );
    assert_eq!(
        PHYSICAL_POS_CARD_DATA_KEY_RULE.suffix_terms,
        strings(&rule["suffixTerms"])
    );
    assert_eq!(
        PHYSICAL_POS_CARD_DATA_KEY_RULE.whole_key_terms,
        strings(&rule["wholeKeyTerms"])
    );
    assert_eq!(
        PHYSICAL_POS_CARD_DATA_KEY_RULE.mask_tokens,
        strings(&rule["maskTokens"])
    );
}

#[test]
fn card_data_keys() {
    for case in fixture()["cardDataKeyCases"].as_array().unwrap() {
        let key = case["key"].as_str().unwrap();
        assert_eq!(
            physical_pos_is_card_data_key(key),
            case["cardData"].as_bool().unwrap(),
            "{key}"
        );
    }
}

#[test]
fn card_data_paths() {
    for case in fixture()["cardDataPathCases"].as_array().unwrap() {
        assert_eq!(
            physical_pos_find_card_data_field(&case["body"], "(body)").as_deref(),
            case["field"].as_str(),
            "{}",
            case["body"]
        );
    }
}

#[test]
fn card_number_values() {
    for case in fixture()["panValueCases"].as_array().unwrap() {
        let value = case["value"].as_str().unwrap();
        let item_text = case["mode"] == "ITEM_TEXT";
        assert_eq!(
            physical_pos_contains_pan_like_value(value, item_text),
            case["pan"].as_bool().unwrap(),
            "{value:?} {}",
            case["mode"]
        );
    }
}

#[test]
fn idempotency_keys() {
    for case in fixture()["idempotencyKeyCases"].as_array().unwrap() {
        let key = case["key"].as_str().unwrap();
        assert_eq!(
            physical_pos_is_valid_idempotency_key(key),
            case["valid"].as_bool().unwrap(),
            "{key:?}"
        );
    }
}

#[tokio::test]
async fn idempotency_keys_are_checked_before_sending() {
    let (server, seen) = recording().await;
    let client = server.client();
    for case in fixture()["idempotencyKeyCases"].as_array().unwrap() {
        if case["valid"] == false {
            let result = client
                .physical_pos()
                .create_payment(&sale(), case["key"].as_str().unwrap())
                .await;
            assert!(matches!(result, Err(Error::Invalid(_))), "{result:?}");
        }
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn forbidden_fields_are_refused_before_sending() {
    let (server, seen) = recording().await;
    let client = server.client();
    for field in strings(&fixture()["forbiddenFields"]) {
        let mut body = sale();
        body[field] = json!("x");
        let result = client.physical_pos().create_payment(&body, KEY).await;
        assert!(
            matches!(
                result,
                Err(Error::Invalid(_) | Error::PhysicalPosCardDataRejected { status: 0, .. })
            ),
            "{field}: {result:?}"
        );
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn not_started_codes() {
    assert_eq!(
        PHYSICAL_POS_NOT_STARTED_CODES,
        strings(&fixture()["notStartedCodes"])
    );
}

#[test]
fn request_field_lists() {
    let fields = &fixture()["requestFields"];
    let expected = fields.as_object().unwrap();
    for (name, list) in PHYSICAL_POS_REQUEST_FIELDS {
        let names: Vec<&str> = list.iter().map(|(field, _)| *field).collect();
        assert_eq!(names, strings(&expected[*name]), "{name}");
    }
    assert_eq!(
        PHYSICAL_POS_PAYMENT_TARGET_FIELDS,
        strings(&fields["paymentTarget"])
    );
    assert_eq!(PHYSICAL_POS_SALE_ITEM_FIELDS, strings(&fields["saleItem"]));
    assert_eq!(
        PHYSICAL_POS_REFUND_ITEM_FIELDS,
        strings(&fields["refundItem"])
    );
    let nested = ["paymentTarget", "saleItem", "refundItem"];
    assert_eq!(
        PHYSICAL_POS_REQUEST_FIELDS.len() + nested.len(),
        expected.len()
    );
}

#[test]
fn filter_lists() {
    let filters = fixture()["filters"].clone();
    let expected = filters.as_object().unwrap();
    assert_eq!(PHYSICAL_POS_FILTERS.len(), expected.len());
    for (route, list) in PHYSICAL_POS_FILTERS {
        assert_eq!(*list, strings(&expected[*route]), "{route}");
    }
}

#[tokio::test]
async fn path_identifiers() {
    let (server, seen) = recording().await;
    let client = server.client();
    for case in fixture()["pathIdCases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let result = client.physical_pos().get_payment(id).await;
        if case["valid"] == true {
            result.unwrap();
            let encoded = case["encoded"].as_str().unwrap_or(id);
            let last = seen.lock().unwrap().last().cloned().unwrap();
            assert_eq!(last, format!("/api/v1/physical-pos/payments/{encoded}"));
        } else {
            assert!(matches!(result, Err(Error::Invalid(_))), "{id:?}");
        }
    }
}

#[test]
fn event_types() {
    assert_eq!(PHYSICAL_POS_EVENT_TYPES, strings(&fixture()["eventTypes"]));
}

#[test]
fn webhook_original() {
    for case in fixture()["webhookOriginalCases"].as_array().unwrap() {
        let expected = case["original"].as_object().map(|o| {
            (
                o["paymentId"].as_str().unwrap().to_owned(),
                o["origin"].as_str().unwrap().to_owned(),
            )
        });
        let actual =
            PhysicalPosOriginal::from_data(&case["data"]).map(|o| (o.payment_id, o.origin));
        assert_eq!(actual, expected, "{}", case["name"]);
    }
}

#[test]
fn webhook_cashier() {
    for case in fixture()["webhookCashierCases"].as_array().unwrap() {
        let expected = case["cashier"].as_object().map(|o| {
            (
                o["cashierId"].as_str().unwrap().to_owned(),
                o["cashierCode"].as_str().unwrap().to_owned(),
            )
        });
        let actual =
            PhysicalPosCashier::from_data(&case["data"]).map(|c| (c.cashier_id, c.cashier_code));
        assert_eq!(actual, expected, "{}", case["name"]);
    }
}

#[test]
fn webhook_categories() {
    for case in fixture()["webhookCategoryCases"].as_array().unwrap() {
        let event_type = case["type"].as_str().unwrap();
        assert_eq!(
            physical_pos_category_of(event_type),
            case["category"].as_str().unwrap(),
            "{event_type}"
        );
    }
}
