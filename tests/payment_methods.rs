#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
mod common;
use common::{
    Incoming, Reply, Server, address, basket_item, buyer, order_details, validate_signature,
    with_order_details,
};
use lunixi::{
    BANK_TRANSFER_EXCEPTION_LIMIT_MAX, BANK_TRANSFER_OTHER_BANKS_CODE,
    BankTransferExceptionFilters, BankTransferInstruction, BasketItem, CARD_ONLY_FIELDS, Client,
    DIRECT_METHOD_PAYMENT_FIELDS, DirectMethodPaymentRequest, Error, LIST_SPLIT_PAYMENTS_FILTERS,
    METHOD_BLOCK_FIELDS, PAYMENT_ACTION_CONFIRM_FIELDS, PROMISSORY_NOTE_MAX_SCHEDULE_ITEMS,
    PromissoryNoteScheduleRow, SHOPPING_LOAN_MAX_BASKET_ITEMS, SPLIT_LEG_BLOCK_FIELDS,
    SPLIT_LEG_EVENTS_SUBSCRIPTION, bank_transfer_exception_kind, bank_transfer_exception_status,
    payment_action_type, payment_bank_transfer_events, payment_method,
    promissory_note_cancel_error_code, promissory_note_cancel_result,
    promissory_note_document_locale, promissory_note_document_type, promissory_note_events,
    promissory_note_status, provider_selection, provider_selection_mode, split_payment_events,
};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

const KEY: &str = "attempt-0000000001";
const TOKEN: &str = r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#;

struct Seen {
    method: String,
    path: String,
    accept: String,
    idempotency_key: Option<String>,
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
            accept: r.headers.get("accept").cloned().unwrap_or_default(),
            idempotency_key: r.headers.get("idempotency-key").cloned(),
            body,
        });
        answer
    })
    .await;
    (server, seen)
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

fn base(method: &str) -> serde_json::Map<String, Value> {
    json!({"paymentMethod":method,"amount":"10000","currency":"TRY","orderId":"order-1"})
        .as_object()
        .unwrap()
        .clone()
}
fn raw(method: &str, extra: Value) -> Value {
    let mut body = base(method);
    body.extend(extra.as_object().unwrap().clone());
    Value::Object(body)
}
/// `request` with complete order details of `amount`.
fn ordered(request: DirectMethodPaymentRequest, amount: &str) -> DirectMethodPaymentRequest {
    request
        .with_buyer(buyer())
        .with_billing_address(address())
        .with_basket_items(vec![basket_item(amount)])
}
/// A sent body without its order details.
fn order_free(body: &Value) -> Value {
    let mut body = body.clone();
    if let Some(object) = body.as_object_mut() {
        for key in ["buyer", "billingAddress", "shippingAddress", "basketItems"] {
            object.remove(key);
        }
    }
    body
}
fn rows(amounts: &[&str], dates: &[&str]) -> Value {
    Value::Array(
        amounts
            .iter()
            .zip(dates)
            .map(|(a, d)| json!({"amount":a,"dueDate":d}))
            .collect(),
    )
}
fn invalid<T: std::fmt::Debug>(result: lunixi::Result<T>, case: &str) {
    assert!(
        matches!(result, Err(Error::Invalid(_))),
        "{case}: {result:?}"
    );
}

#[test]
fn constants_match_the_reference_sdks() {
    assert_eq!(
        payment_method::ALL,
        [
            "CARD",
            "BNPL",
            "SHOPPING_LOAN",
            "PROMISSORY_NOTE",
            "MEAL_CARD",
            "GIFT_CARD",
            "CASH",
            "CASH_ON_DELIVERY",
            "WALLET",
            "BANK_TRANSFER",
            "OPEN_BANKING",
            "MOBILE_BILLING",
            "QR_CODE",
            "CRYPTO",
            "OTHER"
        ]
    );
    assert_eq!(payment_method::normalize("QR").unwrap(), "QR_CODE");
    assert_eq!(
        payment_method::block_name("BANK_TRANSFER").unwrap(),
        "bankTransfer"
    );
    assert_eq!(
        payment_method::block_name("MOBILE_BILLING").unwrap(),
        "mobileBilling"
    );
    assert_eq!(payment_method::block_name("CARD").unwrap(), "card");
    assert_eq!(
        [
            payment_action_type::OTP,
            payment_action_type::REDIRECT,
            payment_action_type::DISPLAY,
            payment_action_type::APP_APPROVAL,
            payment_action_type::INPUT
        ],
        ["OTP", "REDIRECT", "DISPLAY", "APP_APPROVAL", "INPUT"]
    );
    assert_eq!(
        [
            provider_selection::ROUTED,
            provider_selection::OPTIONAL,
            provider_selection::REQUIRED,
            provider_selection_mode::BUYER,
            provider_selection_mode::PLATFORM
        ],
        ["ROUTED", "OPTIONAL", "REQUIRED", "BUYER", "PLATFORM"]
    );
    assert_eq!(
        METHOD_BLOCK_FIELDS,
        [
            "mobileBilling",
            "wallet",
            "promissoryNote",
            "mealCard",
            "openBanking",
            "shoppingLoan",
            "bankTransfer"
        ]
    );
    assert_eq!(DIRECT_METHOD_PAYMENT_FIELDS.len(), 27);
    assert!(DIRECT_METHOD_PAYMENT_FIELDS.ends_with(METHOD_BLOCK_FIELDS));
    assert_eq!(
        CARD_ONLY_FIELDS,
        ["card", "installment", "installmentQuoteToken", "force3D"]
    );
    assert_eq!(
        PAYMENT_ACTION_CONFIRM_FIELDS,
        ["otp", "consentAccepted", "fields"]
    );
    assert_eq!(
        SPLIT_LEG_BLOCK_FIELDS,
        ["mobileBilling", "wallet", "promissoryNote"]
    );
    assert_eq!(LIST_SPLIT_PAYMENTS_FILTERS.len(), 8);
    assert_eq!(SHOPPING_LOAN_MAX_BASKET_ITEMS, 100);
    assert_eq!(BANK_TRANSFER_OTHER_BANKS_CODE, "9999");
    assert_eq!(BANK_TRANSFER_EXCEPTION_LIMIT_MAX, 200);
    assert_eq!(
        [
            bank_transfer_exception_kind::LATE,
            bank_transfer_exception_kind::UNDERPAID,
            bank_transfer_exception_kind::OVERPAID
        ],
        ["LATE", "UNDERPAID", "OVERPAID"]
    );
    assert_eq!(
        bank_transfer_exception_status::ALL,
        ["PENDING_DECISION", "ACCEPTED", "REFUND_PENDING", "REFUNDED"]
    );
    assert_eq!(
        [
            payment_bank_transfer_events::PARTIALLY_RECEIVED,
            payment_bank_transfer_events::UNDERPAID,
            payment_bank_transfer_events::OVERPAID,
            payment_bank_transfer_events::LATE_ARRIVAL
        ],
        [
            "payment.bank_transfer.partially_received",
            "payment.bank_transfer.underpaid",
            "payment.bank_transfer.overpaid",
            "payment.bank_transfer.late_arrival"
        ]
    );
    assert_eq!(
        [
            promissory_note_events::PAID,
            promissory_note_events::PROTESTED,
            promissory_note_events::CANCELLED
        ],
        [
            "payment.promissory_note.paid",
            "payment.promissory_note.protested",
            "payment.promissory_note.cancelled"
        ]
    );
    assert_eq!(
        [
            split_payment_events::COMPLETED,
            split_payment_events::VOIDED,
            split_payment_events::COMPENSATING,
            split_payment_events::OUTCOME_PENDING,
            split_payment_events::RECONCILIATION_REQUIRED,
            split_payment_events::LEG_REFUND_PENDING
        ],
        [
            "payment.split.completed",
            "payment.split.voided",
            "payment.split.compensating",
            "payment.split.outcome_pending",
            "payment.split.reconciliation_required",
            "payment.split.leg_refund_pending"
        ]
    );
    assert_eq!(SPLIT_LEG_EVENTS_SUBSCRIPTION, "payment.split.leg_events");
    assert_eq!(PROMISSORY_NOTE_MAX_SCHEDULE_ITEMS, 24);
    assert_eq!(
        [
            promissory_note_status::ISSUED,
            promissory_note_status::PAID,
            promissory_note_status::PROTESTED,
            promissory_note_status::CANCELLED
        ],
        ["ISSUED", "PAID", "PROTESTED", "CANCELLED"]
    );
    assert_eq!(
        [
            promissory_note_document_type::PROMISSORY_NOTE,
            promissory_note_document_type::PAYMENT_ORDER
        ],
        ["PROMISSORY_NOTE", "PAYMENT_ORDER"]
    );
    assert_eq!(
        [
            promissory_note_cancel_result::CANCELLED,
            promissory_note_cancel_result::ALREADY_CLOSED,
            promissory_note_cancel_result::NOT_CANCELLABLE,
            promissory_note_cancel_result::FAILED
        ],
        ["CANCELLED", "ALREADY_CLOSED", "NOT_CANCELLABLE", "FAILED"]
    );
    assert_eq!(
        [
            promissory_note_cancel_error_code::NOT_CANCELLABLE,
            promissory_note_cancel_error_code::PROVIDER_UNAVAILABLE,
            promissory_note_cancel_error_code::CANCEL_REJECTED
        ],
        [
            "payment.promissory_note.not_cancellable",
            "payment.promissory_note.provider_unavailable",
            "payment.promissory_note.cancel_rejected"
        ]
    );
    assert_eq!(
        [
            promissory_note_document_locale::TR,
            promissory_note_document_locale::EN
        ],
        ["tr-TR", "en-GB"]
    );
}

#[tokio::test]
async fn bank_transfer_body_and_display_instruction() {
    let (server, seen) = recording(|_| {
        Reply::json(
            202,
            r#"{"data":{"paymentId":"pay-1","status":"PENDING","paymentMethod":"BANK_TRANSFER","action":{"id":"act-1","type":"DISPLAY","bankTransfer":{"bankCode":"0062","bankName":"Example Bank","holderName":"Example Merchant","iban":"TR000000000000000000000000","amountMinor":"10000","currency":"TRY","paymentCode":"LX12345","expiresAt":"2026-11-16T12:00:00.000Z","otherBank":false}}}}"#,
        )
    })
    .await;
    let client = server.client();
    let request = ordered(
        DirectMethodPaymentRequest::new("10000", "TRY", "order-1").with_method_fields(
            payment_method::BANK_TRANSFER,
            None,
            json!({"bankCode":"0062"}),
        ),
        "10000",
    );
    let response = client
        .payments()
        .direct_method_payment(&request, KEY)
        .await
        .unwrap();
    assert_eq!(response.status, 202);
    let instruction = BankTransferInstruction::from_response(&response).unwrap();
    assert_eq!(instruction.bank_code, "0062");
    assert_eq!(instruction.amount_minor, "10000");
    assert_eq!(instruction.payment_code, "LX12345");
    assert!(!instruction.other_bank);
    let other_banks = with_order_details(
        raw(
            "BANK_TRANSFER",
            json!({"bankTransfer":{"bankCode":BANK_TRANSFER_OTHER_BANKS_CODE}}),
        ),
        "10000",
    );
    client
        .payments()
        .direct_method_payment(&other_banks, KEY)
        .await
        .unwrap();
    let calls = seen.lock().unwrap();
    assert_eq!(calls[0].method, "POST");
    assert_eq!(calls[0].path, "/api/v1/payments/direct/payment");
    assert_eq!(calls[0].idempotency_key.as_deref(), Some(KEY));
    assert_eq!(
        order_free(&calls[0].body),
        json!({"paymentMethod":"BANK_TRANSFER","amount":"10000","currency":"TRY","orderId":"order-1","bankTransfer":{"bankCode":"0062"}})
    );
    let details = order_details("10000");
    for key in ["buyer", "billingAddress", "basketItems"] {
        assert_eq!(calls[0].body[key], details[key], "{key}");
    }
    assert!(calls[0].body["bankTransfer"].get("providerCode").is_none());
    assert_eq!(calls[1].body["bankTransfer"], json!({"bankCode":"9999"}));
    assert!(
        BankTransferInstruction::from_data(
            &json!({"action":{"type":"OTP","bankTransfer":{"bankCode":"0062"}}})
        )
        .is_none()
    );
    assert!(BankTransferInstruction::from_data(&json!({"status":"CAPTURED"})).is_none());
}

#[tokio::test]
async fn direct_method_payment_guards_precede_network() {
    let (_server, client, calls) = counting().await;
    let payments = client.payments();
    let send = |body: Value| {
        let payments = client.payments();
        async move { payments.direct_method_payment(&body, KEY).await }
    };
    let bank = DirectMethodPaymentRequest::new("10000", "TRY", "order-1").with_method_fields(
        payment_method::BANK_TRANSFER,
        Some("bank-provider"),
        json!({"bankCode":"0062"}),
    );
    invalid(
        payments.direct_method_payment(&bank, KEY).await,
        "builder providerCode",
    );
    invalid(
        send(raw(
            "BANK_TRANSFER",
            json!({"bankTransfer":{"bankCode":"0062","providerCode":"bank-provider"}}),
        ))
        .await,
        "raw providerCode",
    );
    invalid(
        send(raw(
            "MOBILE_BILLING",
            json!({"mobileBilling":{},"wallet":{"providerCode":"w"}}),
        ))
        .await,
        "other block",
    );
    invalid(
        send(raw(
            "MOBILE_BILLING",
            json!({"mobileBilling":{},"installment":2}),
        ))
        .await,
        "card field",
    );
    invalid(
        send(raw(
            "MOBILE_BILLING",
            json!({"mobileBilling":{},"unknown":"x"}),
        ))
        .await,
        "unknown field",
    );
    invalid(
        send(raw("MOBILE_BILLING", json!({}))).await,
        "missing block",
    );
    invalid(
        send(raw(
            "MOBILE_BILLING",
            json!({"mobileBilling":{},"price":"10000"}),
        ))
        .await,
        "price",
    );
    invalid(
        send(raw("GIFT_CARD", json!({"giftCard":{}}))).await,
        "method without block",
    );
    invalid(
        send(raw("mobile_billing", json!({"mobileBilling":{}}))).await,
        "lowercase code",
    );
    let card = DirectMethodPaymentRequest::new("10000", "TRY", "order-1").with_method_fields(
        payment_method::CARD,
        None,
        json!({}),
    );
    invalid(
        payments.direct_method_payment(&card, KEY).await,
        "card via method fields",
    );
    let valid = raw(
        "MOBILE_BILLING",
        json!({"mobileBilling":{"gsmNumber":"+905000000000"}}),
    );
    invalid(
        payments.direct_method_payment(&valid, "").await,
        "missing key",
    );
    invalid(
        payments.direct_method_payment(&valid, "short").await,
        "short key",
    );

    let item = || BasketItem::new("sku-1", "Example", "PHYSICAL", "100");
    let loan = |count: usize| {
        DirectMethodPaymentRequest::new((100 * count.max(1)).to_string(), "TRY", "order-1")
            .with_method_fields(
                payment_method::SHOPPING_LOAN,
                None,
                json!({"bankCode":"0062"}),
            )
            .with_basket_items((0..count).map(|_| item()).collect())
    };
    invalid(
        payments.direct_method_payment(&loan(0), KEY).await,
        "empty basket",
    );
    invalid(
        payments.direct_method_payment(&loan(101), KEY).await,
        "101 items",
    );

    let wide: serde_json::Map<String, Value> =
        (0..33).map(|i| (format!("field{i}"), json!("x"))).collect();
    let group: Vec<Value> = (0..11).map(|_| json!({"name":"x"})).collect();
    for (case, fields) in [
        ("33 keys", Value::Object(wide)),
        ("1025 characters", json!({"note":"x".repeat(1025)})),
        ("11 group items", json!({"items":group})),
        ("__proto__", json!({"__proto__":"x"})),
        ("constructor", json!({"constructor":"x"})),
        ("providerCode in fields", json!({"providerCode":"w"})),
        ("null value", json!({"gsm":null})),
    ] {
        let request = DirectMethodPaymentRequest::new("10000", "TRY", "order-1")
            .with_method_fields(
                payment_method::WALLET,
                Some("wallet-provider"),
                fields.clone(),
            );
        invalid(payments.direct_method_payment(&request, KEY).await, case);
        if case != "providerCode in fields" {
            invalid(
                send(raw("WALLET", json!({"wallet":fields}))).await,
                &format!("raw {case}"),
            );
        }
    }
    invalid(
        send(raw(
            "WALLET",
            json!({"wallet":{"providerCode":"bad code!"}}),
        ))
        .await,
        "raw providerCode pattern",
    );

    let dates: Vec<String> = (1..=25).map(|d| format!("2027-01-{d:02}")).collect();
    let dates: Vec<&str> = dates.iter().map(String::as_str).collect();
    for (case, block) in [
        (
            "schedule and noteCount",
            json!({"schedule":rows(&["10000"],&["2027-01-01"]),"noteCount":"1"}),
        ),
        ("no plan", json!({})),
        (
            "total mismatch",
            json!({"schedule":rows(&["5000","4000"],&["2027-01-01","2027-02-01"])}),
        ),
        (
            "invalid date",
            json!({"schedule":rows(&["5000","5000"],&["2027-01-30","2027-02-30"])}),
        ),
        (
            "descending dates",
            json!({"schedule":rows(&["5000","5000"],&["2027-02-01","2027-01-01"])}),
        ),
        ("25 rows", json!({"schedule":rows(&["400"; 25],&dates)})),
        (
            "unknown row field",
            json!({"schedule":[{"amount":"10000","dueDate":"2027-01-01","note":"x"}]}),
        ),
        ("noteCount 25", json!({"noteCount":"25"})),
        ("noteCount 0", json!({"noteCount":0})),
    ] {
        invalid(
            send(raw("PROMISSORY_NOTE", json!({"promissoryNote":block}))).await,
            case,
        );
    }
    let twice = DirectMethodPaymentRequest::new("10000", "TRY", "order-1")
        .with_method_fields(
            payment_method::PROMISSORY_NOTE,
            None,
            json!({"noteCount":"2"}),
        )
        .with_promissory_note_count(3);
    invalid(
        payments.direct_method_payment(&twice, KEY).await,
        "plan twice",
    );
    let in_fields = DirectMethodPaymentRequest::new("10000", "TRY", "order-1").with_method_fields(
        payment_method::PROMISSORY_NOTE,
        None,
        json!({"schedule":rows(&["10000"],&["2027-01-01"])}),
    );
    invalid(
        payments.direct_method_payment(&in_fields, KEY).await,
        "schedule in fields",
    );
    let misplaced = DirectMethodPaymentRequest::new("10000", "TRY", "order-1")
        .with_method_fields(payment_method::WALLET, None, json!({}))
        .with_promissory_note_count(2);
    invalid(
        payments.direct_method_payment(&misplaced, KEY).await,
        "plan on wallet",
    );
    let mismatch = DirectMethodPaymentRequest::new("10000", "TRY", "order-1")
        .with_method_fields(payment_method::PROMISSORY_NOTE, None, json!({}))
        .with_promissory_note_schedule(vec![PromissoryNoteScheduleRow::new("9000", "2027-01-01")]);
    invalid(
        payments.direct_method_payment(&mismatch, KEY).await,
        "builder mismatch",
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn action_exception_note_reference_and_split_guards_precede_network() {
    let (_server, client, calls) = counting().await;
    let payments = client.payments();
    for (case, input) in [
        ("unknown field", json!({"otp":"123456","extra":true})),
        ("token", json!({"otp":"123456","token":"checkout-token"})),
        (
            "checkoutSessionProof",
            json!({"checkoutSessionProof":"proof"}),
        ),
        ("letters in otp", json!({"otp":"12a456"})),
        ("17 digits", json!({"otp":"12345678901234567"})),
        ("consent not boolean", json!({"consentAccepted":"yes"})),
        ("bad fields", json!({"fields":{"__proto__":"x"}})),
        ("empty", json!({})),
    ] {
        invalid(
            payments
                .confirm_payment_action("pay-1", "act-1", input)
                .await,
            case,
        );
    }
    invalid(
        payments
            .confirm_payment_action("", "act-1", json!({"otp":"1"}))
            .await,
        "empty payment ID",
    );
    invalid(
        payments.resend_payment_action("pay-1", "").await,
        "empty action ID",
    );
    invalid(
        payments.resend_payment_action("pay/1", "act-1").await,
        "path segment",
    );

    let filters = BankTransferExceptionFilters::default();
    for (case, filters) in [
        ("PROD", filters.clone().with_environment("PROD")),
        ("limit 0", filters.clone().with_limit(0)),
        ("limit 201", filters.clone().with_limit(201)),
        ("status", filters.with_status("OPEN")),
    ] {
        invalid(payments.list_bank_transfer_exceptions(&filters).await, case);
    }
    for id in [
        "exc-1",
        "6f1c2b9e-1f0a-4c55-9a7e-2b8d6c4e1a3",
        "6f1c2b9e1f0a4c559a7e2b8d6c4e1a3f",
    ] {
        invalid(payments.accept_bank_transfer_exception(id).await, id);
        invalid(payments.mark_bank_transfer_exception_refunded(id).await, id);
    }

    invalid(payments.methods(Some("card")).await, "lowercase method");
    invalid(
        payments.list_payment_methods(Some("")).await,
        "empty method",
    );

    invalid(
        payments
            .download_promissory_note_document("pay-1", Some("de-DE"))
            .await,
        "locale",
    );
    let many: Vec<String> = (0..25).map(|i| format!("SN-{i:04}")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    for (case, serials) in [
        ("short serial", vec!["SN1"]),
        ("invalid character", vec!["SN_0001"]),
        ("empty list", vec![]),
        ("25 serials", many),
    ] {
        invalid(
            payments
                .cancel_promissory_notes("pay-1", Some(&serials), None)
                .await,
            case,
        );
    }
    invalid(payments.get_promissory_notes("").await, "empty payment ID");

    let reference = client.reference();
    for code in ["00", "82", "6", "6a"] {
        invalid(reference.list_districts(code).await, code);
    }
    for code in ["", "12a", "1234567890123"] {
        invalid(reference.list_neighborhoods(code).await, code);
    }

    for (case, leg) in [
        (
            "bank transfer leg",
            json!({"paymentMethod":"BANK_TRANSFER","amount":"100","bankTransfer":{"bankCode":"0062"}}),
        ),
        (
            "open banking leg",
            json!({"paymentMethod":"OPEN_BANKING","amount":"100","openBanking":{"bankCode":"0062"}}),
        ),
        (
            "shopping loan leg",
            json!({"paymentMethod":"SHOPPING_LOAN","amount":"100"}),
        ),
        (
            "meal card block",
            json!({"paymentMethod":"MEAL_CARD","amount":"100","mealCard":{}}),
        ),
        (
            "card leg with block",
            json!({"cardToken":"card-1","amount":"100","mobileBilling":{"providerCode":"m"}}),
        ),
        (
            "other block",
            json!({"paymentMethod":"WALLET","amount":"100","wallet":{"providerCode":"w"},"mobileBilling":{}}),
        ),
        (
            "missing own block",
            json!({"paymentMethod":"MOBILE_BILLING","amount":"100"}),
        ),
    ] {
        invalid(
            payments
                .split(
                    json!({"totalAmount":"100","currency":"TRY","ownerRef":"owner","legs":[leg]}),
                    KEY,
                )
                .await,
            case,
        );
    }
    for filters in [
        vec![("unknown".to_string(), "x".to_string())],
        vec![("limit".into(), "0".into())],
        vec![("limit".into(), "101".into())],
        vec![("status".into(), "DONE".into())],
        vec![("createdFrom".into(), "yesterday".into())],
    ] {
        invalid(payments.list_splits(filters).await, "split list filter");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn typed_routes_are_signed_with_the_documented_wire_shape() {
    let (server, seen) = recording(|_| Reply::json(200, r#"{"data":{}}"#)).await;
    let client = server.client();
    let payments = client.payments();
    let schedule = ordered(DirectMethodPaymentRequest::new("10000", "TRY", "order-1"), "10000")
        .with_method_fields(
            payment_method::PROMISSORY_NOTE,
            Some("note-provider"),
            json!({"debtorAddress":{"provinceCode":"06","districtCode":"1234","line":"Example street 1"}}),
        )
        .with_promissory_note_schedule(vec![
            PromissoryNoteScheduleRow::new("5000", "2027-01-15"),
            PromissoryNoteScheduleRow::new("5000", "2027-02-15"),
        ])
        .with_callback_url("https://merchant.example/return");
    payments
        .direct_method_payment(&schedule, KEY)
        .await
        .unwrap();
    let count = ordered(
        DirectMethodPaymentRequest::new("10000", "TRY", "order-2").with_method_fields(
            payment_method::PROMISSORY_NOTE,
            None,
            json!({"noteCount":6}),
        ),
        "10000",
    );
    payments.direct_method_payment(&count, KEY).await.unwrap();
    let card = ordered(
        DirectMethodPaymentRequest::new("10000", "TRY", "order-3"),
        "10000",
    )
    .with_card(json!({"cardUserKey":"user-key","cardToken":"card-token"}))
    .with_force_3d(true)
    .with_installment(1, None);
    payments.direct_method_payment(&card, KEY).await.unwrap();
    payments
        .confirm_payment_action("pay-1", "act-1", json!({"otp":"123456"}))
        .await
        .unwrap();
    payments
        .resend_payment_action("pay-1", "act-1")
        .await
        .unwrap();
    payments.list_payment_methods(Some("QR")).await.unwrap();
    payments.methods(None).await.unwrap();
    let filters = BankTransferExceptionFilters::default()
        .with_environment("LIVE")
        .with_status(bank_transfer_exception_status::PENDING_DECISION)
        .with_limit(50);
    payments
        .list_bank_transfer_exceptions(&filters)
        .await
        .unwrap();
    payments
        .list_bank_transfer_exceptions(&BankTransferExceptionFilters::default())
        .await
        .unwrap();
    let id = "6F1C2B9E-1F0A-4C55-9A7E-2B8D6C4E1A3F";
    payments.accept_bank_transfer_exception(id).await.unwrap();
    payments
        .mark_bank_transfer_exception_refunded(id)
        .await
        .unwrap();
    payments.get_promissory_notes("pay-1").await.unwrap();
    payments
        .cancel_promissory_notes("pay-1", None, None)
        .await
        .unwrap();
    payments
        .cancel_promissory_notes("pay-1", Some(&["SN-0001", "SN-0002"]), Some(KEY))
        .await
        .unwrap();
    client.reference().list_provinces().await.unwrap();
    client.reference().list_districts("06").await.unwrap();
    client.reference().list_neighborhoods("1234").await.unwrap();
    payments
        .list_splits(vec![
            ("status".into(), "COMPLETED".into()),
            ("status".into(), "VOIDED".into()),
            ("limit".into(), "25".into()),
            ("orderId".into(), String::new()),
        ])
        .await
        .unwrap();
    payments
        .split(
            with_order_details(
                json!({"totalAmount":"300","currency":"TRY","ownerRef":"owner","legs":[
                    {"cardToken":"card-1","amount":"100"},
                    {"paymentMethod":"MOBILE_BILLING","amount":"100","mobileBilling":{"providerCode":"mobile-provider"}},
                    {"paymentMethod":"PROMISSORY_NOTE","amount":"100","promissoryNote":{"providerCode":"note-provider","noteCount":"2"}}
                ]}),
                "300",
            ),
            KEY,
        )
        .await
        .unwrap();

    let calls = seen.lock().unwrap();
    let route = |i: usize| (calls[i].method.as_str(), calls[i].path.as_str());
    assert_eq!(route(0), ("POST", "/api/v1/payments/direct/payment"));
    assert_eq!(
        order_free(&calls[0].body),
        json!({"paymentMethod":"PROMISSORY_NOTE","amount":"10000","currency":"TRY","orderId":"order-1",
            "callbackUrl":"https://merchant.example/return",
            "promissoryNote":{"providerCode":"note-provider",
                "debtorAddress":{"provinceCode":"06","districtCode":"1234","line":"Example street 1"},
                "schedule":[{"amount":"5000","dueDate":"2027-01-15"},{"amount":"5000","dueDate":"2027-02-15"}]}})
    );
    assert_eq!(calls[1].body["promissoryNote"], json!({"noteCount":"6"}));
    assert_eq!(
        order_free(&calls[2].body),
        json!({"paymentMethod":"CARD","amount":"10000","currency":"TRY","orderId":"order-3",
            "card":{"cardUserKey":"user-key","cardToken":"card-token"},"force3D":true,"installment":1})
    );
    assert_eq!(
        route(3),
        ("POST", "/api/v1/payments/pay-1/actions/act-1/confirm")
    );
    assert_eq!(calls[3].body, json!({"otp":"123456"}));
    assert_eq!(calls[3].idempotency_key, None);
    assert_eq!(
        route(4),
        ("POST", "/api/v1/payments/pay-1/actions/act-1/resend")
    );
    assert!(calls[4].body.is_null());
    assert_eq!(
        route(5),
        ("GET", "/api/v1/payments/methods?paymentMethod=QR_CODE")
    );
    assert_eq!(route(6), ("GET", "/api/v1/payments/methods"));
    assert_eq!(
        route(7),
        (
            "GET",
            "/api/v1/payments/bank-transfer/exceptions?environment=LIVE&status=PENDING_DECISION&limit=50"
        )
    );
    assert_eq!(
        route(8),
        ("GET", "/api/v1/payments/bank-transfer/exceptions")
    );
    assert_eq!(
        route(9),
        (
            "POST",
            "/api/v1/payments/bank-transfer/exceptions/6F1C2B9E-1F0A-4C55-9A7E-2B8D6C4E1A3F/accept"
        )
    );
    assert!(calls[9].body.is_null());
    assert_eq!(
        route(10),
        (
            "POST",
            "/api/v1/payments/bank-transfer/exceptions/6F1C2B9E-1F0A-4C55-9A7E-2B8D6C4E1A3F/refunded"
        )
    );
    assert!(calls[10].body.is_null());
    assert_eq!(
        route(11),
        ("GET", "/api/v1/payments/pay-1/promissory-notes")
    );
    assert_eq!(
        route(12),
        ("POST", "/api/v1/payments/pay-1/promissory-notes/cancel")
    );
    assert!(calls[12].body.is_null());
    assert_eq!(
        calls[13].body,
        json!({"serialNumbers":["SN-0001","SN-0002"]})
    );
    assert_eq!(calls[13].idempotency_key.as_deref(), Some(KEY));
    assert_eq!(route(14), ("GET", "/api/v1/reference/tr-address/provinces"));
    assert_eq!(
        route(15),
        (
            "GET",
            "/api/v1/reference/tr-address/districts?provinceCode=06"
        )
    );
    assert_eq!(
        route(16),
        (
            "GET",
            "/api/v1/reference/tr-address/neighborhoods?districtCode=1234"
        )
    );
    assert_eq!(
        route(17),
        (
            "GET",
            "/api/v1/payments/split?status=COMPLETED&status=VOIDED&limit=25"
        )
    );
    assert_eq!(route(18), ("POST", "/api/v1/payments/split"));
    assert_eq!(calls[18].body["buyer"], order_details("300")["buyer"]);
    assert_eq!(calls[18].body["basketItems"][0]["price"], "300");
    assert!(calls.iter().all(|c| c.accept == "application/json"));
}

#[tokio::test]
async fn promissory_note_document_returns_pdf_bytes() {
    const PDF: &str = "%PDF-1.7\nsynthetic promissory note document\n%%EOF\n";
    let (server, seen) = recording(|r| {
        if r.path.contains("pay-missing") {
            Reply::json(
                503,
                r#"{"code":"payment.promissory_note.document_unavailable","message":"provider detail"}"#,
            )
        } else {
            Reply {
                status: 200,
                body: PDF.into(),
                headers: vec![("Content-Type".into(), "application/pdf".into())],
            }
        }
    })
    .await;
    let client = server.client();
    let document = client
        .payments()
        .download_promissory_note_document("pay-1", Some(promissory_note_document_locale::EN))
        .await
        .unwrap();
    assert_eq!(document.status, 200);
    assert_eq!(document.bytes, PDF.as_bytes());
    assert_eq!(document.content_type.as_deref(), Some("application/pdf"));
    assert!(document.envelope.is_none());
    let error = client
        .payments()
        .download_promissory_note_document("pay-missing", None)
        .await
        .unwrap_err();
    assert!(matches!(
        &error,
        Error::Api { status: 503, code, .. } if code == "payment.promissory_note.document_unavailable"
    ));
    assert!(!format!("{error:?} {error}").contains("provider detail"));
    let calls = seen.lock().unwrap();
    assert_eq!(
        calls[0].path,
        "/api/v1/payments/pay-1/promissory-notes/document?locale=en-GB"
    );
    assert_eq!(calls[0].accept, "application/pdf, application/json");
    assert_eq!(
        calls[1].path,
        "/api/v1/payments/pay-missing/promissory-notes/document"
    );
}

#[tokio::test]
async fn direct_and_split_payments_name_the_missing_order_details() {
    let (_server, client, calls) = counting().await;
    let payments = client.payments();
    let mut body = with_order_details(
        raw("BANK_TRANSFER", json!({"bankTransfer":{"bankCode":"0062"}})),
        "10000",
    );
    body["buyer"]
        .as_object_mut()
        .map(|buyer| buyer.remove("ip"));
    body["basketItems"][0]["itemType"] = json!("PHYSICAL");
    assert_eq!(
        payments.direct_method_payment(&body, KEY).await.err(),
        Some(Error::MissingOrderDetails {
            operation: "A direct method payment",
            fields: vec!["buyer.ip".into(), "shippingAddress".into()],
        })
    );
    let split = json!({"totalAmount":"100","currency":"TRY","ownerRef":"owner","legs":[
        {"cardToken":"card-1","amount":"100"}
    ]});
    assert_eq!(
        payments.split(split, KEY).await.err(),
        Some(Error::MissingOrderDetails {
            operation: "A split payment",
            fields: vec![
                "buyer".into(),
                "billingAddress".into(),
                "basketItems".into()
            ],
        })
    );
    assert_eq!(
        lunixi::SPLIT_PAYMENT_FIELDS[7..],
        ["buyer", "billingAddress", "shippingAddress", "basketItems"]
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
