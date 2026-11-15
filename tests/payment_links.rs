#![allow(clippy::unwrap_used, clippy::expect_used)]
#[allow(dead_code)]
mod common;
use common::{Incoming, Reply, Server, validate_signature};
use lunixi::{
    Client, CreatePaymentLinkRequest, Error, PAYMENT_LINK_IMMUTABLE_FIELDS,
    PAYMENT_LINK_SPEC_FIELDS, PaymentLinkActionOptions, PaymentLinkCustomField, PaymentLinkItem,
    PaymentLinkSendRequest, buyer_field_requirement, custom_field_type, payment_link_amount_mode,
    payment_link_image_content_type, payment_link_qr_format, payment_link_usage,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

const KEY: &str = "link-attempt-0000001";
const TOKEN: &str = r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#;
const ASSET: &str = "6f1c2b9e-4d3a-4e8f-9b1a-2c3d4e5f6a7b";
const LINK: &str = "plink_01";

struct Seen {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    raw: Vec<u8>,
}
impl Seen {
    fn body(&self) -> Value {
        if self.raw.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&self.raw).unwrap()
        }
    }
    fn text(&self) -> String {
        String::from_utf8(self.raw.clone()).unwrap()
    }
}

/// Records every request. Gateway calls must be signed and carry the access
/// token; the storage PUT (`/storage/…`) is recorded as it arrived.
async fn recording(
    reply: impl Fn(&Incoming) -> Reply + Send + Sync + 'static,
) -> (Server, Arc<Mutex<Vec<Seen>>>) {
    let seen = Arc::new(Mutex::new(vec![]));
    let log = seen.clone();
    let server = Server::start(move |r| {
        if !r.path.starts_with("/storage/") {
            validate_signature(&r);
            if r.path == "/api/v1/auth/token" {
                return Reply::json(200, TOKEN);
            }
            assert_eq!(r.headers["authorization"], "Bearer at_TEST_fixture");
        }
        let answer = reply(&r);
        log.lock().unwrap().push(Seen {
            method: r.method.clone(),
            path: r.path.clone(),
            headers: r.headers.clone(),
            raw: r.body.clone(),
        });
        answer
    })
    .await;
    (server, seen)
}
fn link(_: &Incoming) -> Reply {
    Reply::json(
        200,
        r#"{"data":{"id":"plink_01","rowVersion":3,"amountMinor":"150000"},"pageInfo":{"hasMore":false}}"#,
    )
}
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

#[test]
fn field_lists_match_the_reference_sdks() {
    assert_eq!(PAYMENT_LINK_SPEC_FIELDS.len(), 39);
    assert_eq!(PAYMENT_LINK_SPEC_FIELDS.first(), Some(&"usage"));
    assert_eq!(PAYMENT_LINK_SPEC_FIELDS.last(), Some(&"metadata"));
    assert_eq!(
        PAYMENT_LINK_IMMUTABLE_FIELDS,
        ["usage", "amountMode", "currency"]
    );
    assert_eq!(payment_link_amount_mode::ALL, ["FIXED", "OPEN", "ITEMIZED"]);
    assert_eq!(payment_link_qr_format::ALL, ["svg", "png"]);
    assert_eq!(
        payment_link_image_content_type::ALL,
        ["image/png", "image/jpeg", "image/webp"]
    );
}

#[tokio::test]
async fn create_sends_the_typed_body_in_catalog_order_with_the_key() {
    let (server, seen) = recording(link).await;
    let client = server.client();
    let request = CreatePaymentLinkRequest::fixed(
        payment_link_usage::SINGLE_USE,
        "150000",
        "try",
        "Invoice 42",
    )
    .with_metadata(json!({"invoice":"42"}))
    .with_description("Consulting")
    .with_buyer_fields(json!({"email": buyer_field_requirement::REQUIRED}))
    .with_checkout(true, vec![], Some(vec![3, 1]))
    .with_environment("TEST");
    client.payment_links().create(&request, KEY).await.unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/api/v1/payments/links");
    assert_eq!(seen[0].headers["idempotency-key"], KEY);
    assert_eq!(
        seen[0].text(),
        r#"{"environment":"TEST","usage":"SINGLE_USE","amountMode":"FIXED","currency":"TRY","amountMinor":"150000","title":"Invoice 42","description":"Consulting","buyerFields":{"email":"REQUIRED"},"checkout":{"requireThreeDs":true,"paymentMethods":[],"enabledInstallments":[1,3]},"metadata":{"invoice":"42"}}"#
    );
}

#[tokio::test]
async fn create_itemized_and_raw_bodies() {
    let (server, seen) = recording(link).await;
    let client = server.client();
    let request = CreatePaymentLinkRequest::itemized(
        payment_link_usage::MULTI_USE,
        "TRY",
        "Tickets",
        vec![PaymentLinkItem::new("Adult", "25000").with_quantity_range(0, 4)],
    )
    .add_item(PaymentLinkItem::new("Child", "10000").with_image_asset_id(ASSET))
    .with_custom_fields(vec![
        PaymentLinkCustomField::new("seat", "Seat", custom_field_type::TEXT).with_required(true),
    ]);
    client.payment_links().create(&request, KEY).await.unwrap();
    let raw = json!({"usage":"MULTI_USE","amountMode":"OPEN","currency":"TRY","title":"Donate","minAmountMinor":"1000"});
    client.payment_links().create(&raw, KEY).await.unwrap();
    let seen = seen.lock().unwrap();
    let body = seen[0].body();
    assert_eq!(
        body["items"],
        json!([
            {"name":"Adult","unitPriceMinor":"25000","minQuantity":0,"maxQuantity":4},
            {"name":"Child","unitPriceMinor":"10000","imageAssetId":ASSET}
        ])
    );
    assert_eq!(body["customFields"][0]["fieldType"], "TEXT");
    assert_eq!(seen[1].body(), raw);
}

#[tokio::test]
async fn create_guards_send_nothing() {
    let (_server, client, calls) = counting().await;
    let valid = CreatePaymentLinkRequest::fixed("SINGLE_USE", "150000", "TRY", "Invoice");
    invalid(
        client.payment_links().create(&valid, "").await,
        "missing key",
    );
    invalid(
        client.payment_links().create(&valid, "short").await,
        "short key",
    );
    let typed = [
        CreatePaymentLinkRequest::fixed("ONCE", "150000", "TRY", "Invoice"),
        CreatePaymentLinkRequest::fixed("SINGLE_USE", "1500.00", "TRY", "Invoice"),
        CreatePaymentLinkRequest::fixed("SINGLE_USE", "150000", "TRY", " "),
        valid.clone().with_environment("PROD"),
        valid.clone().with_locale("de"),
        valid.clone().with_image_asset_id("not-a-uuid"),
        valid.clone().with_buyer_fields(json!({"age":"REQUIRED"})),
        valid.clone().with_buyer_fields(json!({"email":"MAYBE"})),
        valid.clone().with_recipient(json!({"email":1})),
        valid.clone().with_metadata(json!({"n":1})),
        valid.clone().with_checkout(false, vec![], Some(vec![2, 3])),
        CreatePaymentLinkRequest::itemized("MULTI_USE", "TRY", "Tickets", vec![]),
    ];
    for (i, request) in typed.iter().enumerate() {
        invalid(
            client.payment_links().create(request, KEY).await,
            &format!("typed case {i}"),
        );
    }
    for raw in [
        json!({"usage":"SINGLE_USE","amountMode":"FIXED","currency":"TRY","title":"x","price":"1"}),
        json!({"amountMode":"FIXED","currency":"TRY","title":"x"}),
        json!({"usage":"SINGLE_USE","amountMode":"FIXED","currency":"TRY","title":"x","amountMinor":150000}),
        json!([]),
    ] {
        invalid(client.payment_links().create(&raw, KEY).await, "raw");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn list_get_update_and_actions() {
    let (server, seen) = recording(link).await;
    let client = server.client();
    let links = client.payment_links();
    links
        .list(vec![
            ("state".into(), "ACTIVE,PAUSED".into()),
            ("limit".into(), "20".into()),
            ("tag".into(), String::new()),
        ])
        .await
        .unwrap();
    links.get(LINK).await.unwrap();
    links
        .update(
            LINK,
            3,
            &json!({"title":"New title","expiresAt":null}),
            Some(KEY),
        )
        .await
        .unwrap();
    links
        .pause(LINK, &PaymentLinkActionOptions::default(), None)
        .await
        .unwrap();
    links
        .resume(
            LINK,
            &PaymentLinkActionOptions::default().with_expected_row_version(4),
            None,
        )
        .await
        .unwrap();
    links
        .deactivate(
            LINK,
            &PaymentLinkActionOptions::default().with_reason("Sold out"),
            Some(KEY),
        )
        .await
        .unwrap();
    links
        .list_attempts(LINK, vec![("state".into(), "FAILED".into())])
        .await
        .unwrap();
    links
        .list_payments(LINK, vec![("cursor".into(), "c1".into())])
        .await
        .unwrap();
    let seen = seen.lock().unwrap();
    let routes: Vec<_> = seen
        .iter()
        .map(|s| (s.method.as_str(), s.path.as_str()))
        .collect();
    assert_eq!(
        routes,
        [
            (
                "GET",
                "/api/v1/payments/links?state=ACTIVE%2CPAUSED&limit=20"
            ),
            ("GET", "/api/v1/payments/links/plink_01"),
            ("PATCH", "/api/v1/payments/links/plink_01"),
            ("POST", "/api/v1/payments/links/plink_01/pause"),
            ("POST", "/api/v1/payments/links/plink_01/resume"),
            ("POST", "/api/v1/payments/links/plink_01/deactivate"),
            (
                "GET",
                "/api/v1/payments/links/plink_01/attempts?state=FAILED"
            ),
            ("GET", "/api/v1/payments/links/plink_01/payments?cursor=c1"),
        ]
    );
    assert_eq!(
        seen[2].text(),
        r#"{"expectedRowVersion":3,"title":"New title","expiresAt":null}"#
    );
    assert_eq!(seen[2].headers["idempotency-key"], KEY);
    assert_eq!(seen[3].text(), "{}");
    assert!(!seen[3].headers.contains_key("idempotency-key"));
    assert_eq!(seen[4].text(), r#"{"expectedRowVersion":4}"#);
    assert_eq!(seen[5].text(), r#"{"reason":"Sold out"}"#);
}

#[tokio::test]
async fn update_list_and_action_guards_send_nothing() {
    let (_server, client, calls) = counting().await;
    let links = client.payment_links();
    for field in PAYMENT_LINK_IMMUTABLE_FIELDS {
        let mut changes = serde_json::Map::new();
        changes.insert((*field).into(), json!("X"));
        invalid(
            links.update(LINK, 1, &Value::Object(changes), None).await,
            field,
        );
    }
    invalid(links.update(LINK, 1, &json!({}), None).await, "empty");
    invalid(
        links.update(LINK, 1, &json!({"colour":"red"}), None).await,
        "unknown",
    );
    invalid(
        links
            .update(LINK, 1u64 << 60, &json!({"title":"x"}), None)
            .await,
        "row version",
    );
    invalid(
        links.update("../x", 1, &json!({"title":"x"}), None).await,
        "link id",
    );
    invalid(
        links.list(vec![("page".into(), "2".into())]).await,
        "filter",
    );
    invalid(
        links
            .list_payments(LINK, vec![("state".into(), "OPEN".into())])
            .await,
        "payments filter",
    );
    invalid(
        links
            .action(LINK, "delete", &PaymentLinkActionOptions::default(), None)
            .await,
        "action",
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn send_requires_the_key_and_a_known_channel() {
    let (server, seen) = recording(|_| {
        Reply::json(
            200,
            r#"{"data":{"delivery":{"state":"QUEUED"},"replayed":false}}"#,
        )
    })
    .await;
    let client = server.client();
    let request = PaymentLinkSendRequest::new("SMS")
        .with_recipient_phone("+905321234567")
        .with_locale("en");
    client
        .payment_links()
        .send(LINK, &request, KEY)
        .await
        .unwrap();
    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].path, "/api/v1/payments/links/plink_01/send");
        assert_eq!(
            seen[0].text(),
            r#"{"channel":"SMS","recipientPhone":"+905321234567","locale":"en"}"#
        );
        assert_eq!(seen[0].headers["idempotency-key"], KEY);
    }
    let before = seen.lock().unwrap().len();
    invalid(client.payment_links().send(LINK, &request, "").await, "key");
    invalid(
        client
            .payment_links()
            .send(LINK, &PaymentLinkSendRequest::new("FAX"), KEY)
            .await,
        "channel",
    );
    invalid(
        client
            .payment_links()
            .send(
                LINK,
                &PaymentLinkSendRequest::new("EMAIL").with_locale("de"),
                KEY,
            )
            .await,
        "locale",
    );
    assert_eq!(seen.lock().unwrap().len(), before);
}

#[tokio::test]
async fn qr_returns_image_bytes() {
    let (server, seen) = recording(|_| Reply {
        status: 200,
        body: "<svg/>".into(),
        headers: vec![("Content-Type".into(), "image/svg+xml".into())],
    })
    .await;
    let client = server.client();
    let response = client
        .payment_links()
        .qr(LINK, Some("png"), Some(256))
        .await
        .unwrap();
    assert_eq!(response.bytes, b"<svg/>");
    assert_eq!(response.content_type.as_deref(), Some("image/svg+xml"));
    client.payment_links().qr(LINK, None, None).await.unwrap();
    {
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen[0].path,
            "/api/v1/payments/links/plink_01/qr?format=png&size=256"
        );
        assert_eq!(
            seen[0].headers["accept"],
            "image/svg+xml, image/png, application/json"
        );
        assert_eq!(
            seen[1].path,
            "/api/v1/payments/links/plink_01/qr?format=svg"
        );
    }
    let (_server, client, calls) = counting().await;
    invalid(
        client.payment_links().qr(LINK, Some("gif"), None).await,
        "format",
    );
    invalid(
        client.payment_links().qr(LINK, None, Some(64)).await,
        "small",
    );
    invalid(
        client.payment_links().qr(LINK, None, Some(4096)).await,
        "large",
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

/// Gateway + storage double: `target` builds the image upload answer from the
/// request's own host; `/storage/` answers with `storage`.
async fn image_server(
    target: impl Fn(&str) -> Value + Send + Sync + 'static,
    storage: Reply,
    confirm_state: &'static str,
) -> (Server, Arc<Mutex<Vec<Seen>>>) {
    let storage = Arc::new((storage.status, storage.body, storage.headers));
    recording(move |r| {
        if r.path.starts_with("/storage/") {
            return Reply {
                status: storage.0,
                body: storage.1.clone(),
                headers: storage.2.clone(),
            };
        }
        if r.path == "/api/v1/payments/links/images" {
            let data = target(&r.headers["host"]);
            return Reply::json(201, &json!({ "data": data }).to_string());
        }
        Reply::json(
            200,
            &json!({"data":{"assetId":ASSET,"url":null,"state":confirm_state}}).to_string(),
        )
    })
    .await
}
fn upload_target(host: &str) -> Value {
    json!({
        "assetId": ASSET,
        "uploadUrl": format!("https://{host}/storage/{ASSET}?X-Amz-Signature=fixture"),
        "uploadMethod": "PUT",
        "uploadHeaders": {"Content-Type": "image/png"},
        "expiresAt": "2026-11-15T10:15:00.000Z"
    })
}
fn stored() -> Reply {
    Reply {
        status: 200,
        body: String::new(),
        headers: vec![],
    }
}

#[tokio::test]
async fn upload_image_puts_to_storage_without_credentials() {
    let (server, seen) = image_server(upload_target, stored(), "PUBLISHED").await;
    let client = server.client();
    let content = b"\x89PNG\r\n\x1a\nfixture";
    let confirmed = client
        .payment_links()
        .upload_image(" logo.png ", payment_link_image_content_type::PNG, content)
        .await
        .unwrap();
    assert_eq!(confirmed.data.unwrap()["assetId"], ASSET);
    let seen = seen.lock().unwrap();
    let routes: Vec<_> = seen
        .iter()
        .map(|s| (s.method.as_str(), s.path.as_str()))
        .collect();
    assert_eq!(
        routes,
        [
            ("POST", "/api/v1/payments/links/images"),
            ("PUT", &*format!("/storage/{ASSET}?X-Amz-Signature=fixture")),
            (
                "POST",
                &*format!("/api/v1/payments/links/images/{ASSET}/confirm")
            ),
        ]
    );
    assert_eq!(
        seen[0].text(),
        format!(
            r#"{{"fileName":"logo.png","contentType":"image/png","sizeBytes":{}}}"#,
            content.len()
        )
    );
    let put = &seen[1];
    for header in [
        "authorization",
        "x-key-id",
        "x-date",
        "x-nonce",
        "x-signature",
        "digest",
        "idempotency-key",
    ] {
        assert!(!put.headers.contains_key(header), "PUT carried {header}");
    }
    assert_eq!(put.headers["content-type"], "image/png");
    assert_eq!(put.raw, content);
    assert_eq!(seen[2].text(), "{}");
}

#[tokio::test]
async fn image_steps_can_run_separately() {
    let (server, seen) = image_server(upload_target, stored(), "PUBLISHED").await;
    let client = server.client();
    let links = client.payment_links();
    let target = links
        .create_image_upload("logo.webp", "image/webp", 3)
        .await
        .unwrap();
    assert_eq!(target.asset_id, ASSET);
    assert_eq!(
        target.upload_headers,
        [("Content-Type".to_owned(), "image/png".to_owned())]
    );
    assert!(!format!("{target:?}").contains("X-Amz-Signature"));
    links.put_image_upload(&target, b"abc").await.unwrap();
    links.confirm_image_upload(ASSET).await.unwrap();
    assert_eq!(seen.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn unusable_upload_targets_stop_before_the_put() {
    let broken: [fn(&str) -> Value; 5] = [
        |host| {
            let mut t = upload_target(host);
            t["uploadHeaders"]["Authorization"] = json!("Bearer leaked");
            t
        },
        |host| {
            let mut t = upload_target(host);
            t["uploadUrl"] = json!(format!("http://{host}/storage/x"));
            t
        },
        |host| {
            let mut t = upload_target(host);
            t["uploadMethod"] = json!("POST");
            t
        },
        |host| {
            let mut t = upload_target(host);
            t["assetId"] = json!("asset-1");
            t
        },
        |host| {
            let mut t = upload_target(host);
            t["uploadHeaders"]["Content-Type"] = json!(1);
            t
        },
    ];
    for target in broken {
        let (server, seen) = image_server(target, stored(), "PUBLISHED").await;
        let result = server
            .client()
            .payment_links()
            .upload_image("logo.png", "image/png", b"abc")
            .await;
        assert!(matches!(result, Err(Error::Response(_))), "{result:?}");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "only the upload target was requested");
    }
}

#[tokio::test]
async fn storage_failure_is_reported_and_not_confirmed() {
    let denied = Reply {
        status: 403,
        body: "<?xml version=\"1.0\"?><Error><Code>AccessDenied</Code><Message>Request has expired</Message></Error>".into(),
        headers: vec![("Content-Type".into(), "application/xml".into())],
    };
    let (server, seen) = image_server(upload_target, denied, "PUBLISHED").await;
    let result = server
        .client()
        .payment_links()
        .upload_image("logo.png", "image/png", b"abc")
        .await;
    assert!(
        matches!(&result, Err(Error::Api { status: 403, code, request_id: None }) if code == "AccessDenied"),
        "{result:?}"
    );
    assert_eq!(
        seen.lock().unwrap().len(),
        2,
        "no confirmation after a failed PUT"
    );
}

#[tokio::test]
async fn confirmation_must_publish_the_image() {
    let (server, _seen) = image_server(upload_target, stored(), "PENDING").await;
    let result = server
        .client()
        .payment_links()
        .confirm_image_upload(ASSET)
        .await;
    assert!(matches!(result, Err(Error::Response(_))), "{result:?}");
}

#[tokio::test]
async fn image_guards_send_nothing() {
    let (_server, client, calls) = counting().await;
    let links = client.payment_links();
    invalid(
        links
            .create_image_upload("logo.svg", "image/svg+xml", 3)
            .await,
        "svg",
    );
    invalid(
        links.create_image_upload("logo.png", "image/png", 0).await,
        "size",
    );
    invalid(links.create_image_upload(" ", "image/png", 3).await, "name");
    invalid(
        links
            .create_image_upload(&"a".repeat(256), "image/png", 3)
            .await,
        "long name",
    );
    invalid(
        links.upload_image("logo.png", "image/png", b"").await,
        "empty",
    );
    invalid(links.confirm_image_upload("asset-1").await, "asset id");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
