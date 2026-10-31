#![allow(clippy::unwrap_used, clippy::expect_used)]
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer, SigningKey, pkcs8::DecodePrivateKey};
use hmac::{Hmac, KeyInit, Mac};
use lunixi::{
    Client, Config, Error, MAX_SAFE_INTEGER, WebhookVerifier, canonical_json, canonical_request,
    digest, parse_json_strict,
};
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{Value, json};
use sha2::Sha256;
use std::time::Duration;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/canonical-vectors.json")).unwrap()
}
#[test]
fn node_json_digest_and_ed25519_vectors() {
    let f = fixture();
    let key = SigningKey::from_pkcs8_pem(f["privateKeyPem"].as_str().unwrap()).unwrap();
    for v in f["vectors"].as_array().unwrap() {
        let body =
            canonical_json(&parse_json_strict(v["input"].as_str().unwrap().as_bytes()).unwrap())
                .unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            v["body"].as_str().unwrap()
        );
        assert_eq!(digest(&body), v["digest"].as_str().unwrap());
        let canonical = canonical_request(
            "POST",
            "/api/v1/payments/intents?locale=tr",
            "2026-10-31T00:00:00Z",
            "0123456789abcdef0123456789abcdef",
            Some(&digest(&body)),
        );
        assert_eq!(canonical, v["canonical"].as_str().unwrap());
        assert_eq!(
            STANDARD.encode(key.sign(canonical.as_bytes()).to_bytes()),
            v["signature"].as_str().unwrap()
        );
    }
}
#[test]
fn strict_json_and_safe_integer_boundaries() {
    assert!(parse_json_strict(br#"{"a":1,"a":2}"#).is_err());
    assert!(parse_json_strict(br#"{} {}"#).is_err());
    assert!(canonical_json(&json!({"amount":MAX_SAFE_INTEGER+1})).is_err());
    assert!(canonical_json(&json!({"amount":-MAX_SAFE_INTEGER-1})).is_err());
    assert!(canonical_json(&json!({"amount":MAX_SAFE_INTEGER})).is_ok());
    let input =
        parse_json_strict(br#"{"z":0,"4294967295":1,"4294967294":2,"01":3,"0":4}"#).unwrap();
    assert_eq!(
        canonical_json(&input).unwrap(),
        br#"{"0":4,"4294967294":2,"z":0,"4294967295":1,"01":3}"#
    );
    let mut deep = Value::Null;
    for _ in 0..66 {
        deep = json!([deep]);
    }
    assert!(canonical_json(&deep).is_err());
}
#[test]
fn seed_pkcs8_pair_and_debug_redaction() {
    let f = fixture();
    for private in [
        f["privateSeed"].as_str().unwrap(),
        f["privateKeyPem"].as_str().unwrap(),
    ] {
        let config = Config::new("https://gateway.example", "mk_test_key", private);
        assert!(!format!("{config:?}").contains(private));
        assert!(Client::new(config).is_ok());
    }
    let key = SigningKey::from_pkcs8_pem(f["privateKeyPem"].as_str().unwrap()).unwrap();
    assert!(
        Client::new(Config::new(
            "https://gateway.example",
            "k",
            STANDARD.encode(key.to_keypair_bytes())
        ))
        .is_ok()
    );
    let mut bad = key.to_keypair_bytes();
    bad[63] ^= 1;
    assert!(
        Client::new(Config::new(
            "https://gateway.example",
            "k",
            STANDARD.encode(bad)
        ))
        .is_err()
    );
    assert!(
        Client::new(Config::new(
            "https://gateway.example",
            "k",
            "private-key-invalid"
        ))
        .is_err()
    );
}
#[test]
fn canonical_preserves_explicit_false() {
    let mut intent =
        lunixi::IntentRequest::new(100, "TRY", "order", "https://merchant.example/return");
    intent.force_3d = Some(false);
    intent.preauth = Some(false);
    let body = String::from_utf8(canonical_json(&intent).unwrap()).unwrap();
    assert!(body.contains("\"force3D\":false"));
    assert!(body.contains("\"preauth\":false"));
}
fn headers(id: &str, ts: &str, signature: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("x-lunixi-event-id", HeaderValue::from_str(id).unwrap());
    h.insert(
        "x-lunixi-signature-timestamp",
        HeaderValue::from_str(ts).unwrap(),
    );
    h.insert(
        "x-lunixi-signature",
        HeaderValue::from_str(signature).unwrap(),
    );
    h
}
fn signature(secret: &str, id: &str, ts: &str, raw: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{id}.{ts}.").as_bytes());
    mac.update(raw);
    format!(
        "v2={}",
        mac.finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}
#[test]
fn shared_v2_golden_vectors() {
    let vectors: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/webhook-v2-vectors.json")).unwrap();
    for v in vectors {
        let secret = v["secret"].as_str().unwrap();
        let id = v["eventId"].as_str().unwrap();
        let ts = v["timestamp"].as_str().unwrap();
        let raw = v["rawBody"].as_str().unwrap().as_bytes();
        let now = DateTime::parse_from_rfc3339(ts)
            .unwrap()
            .with_timezone(&Utc);
        let expected = format!("v2={}", v["expectedV2"].as_str().unwrap());
        assert_eq!(signature(secret, id, ts, raw), expected);
        let verifier = WebhookVerifier::new(vec![secret.into()]).unwrap();
        let event = verifier
            .verify_at(raw, &headers(id, ts, &expected), now)
            .unwrap();
        assert_eq!(event.id(), id);
        assert_eq!(event.raw(), raw);
    }
}
#[test]
fn webhook_tamper_binding_rotation_freshness_and_duplicate_guards() {
    let secret = "synthetic_webhook_secret_0123456789";
    let old = "synthetic_old_secret_0123456789";
    let ts = "2026-10-31T00:00:00.000Z";
    let id = "event-1";
    let raw = br#"{"id":"event-1","type":"payment.captured","data":{"amountMinor":"10050"}}"#;
    let now = DateTime::parse_from_rfc3339(ts)
        .unwrap()
        .with_timezone(&Utc);
    let v = WebhookVerifier::new(vec![secret.into(), old.into()]).unwrap();
    let sig = format!(
        "{},{}",
        signature(secret, id, ts, raw),
        signature(old, id, ts, raw)
    );
    let h = headers(id, ts, &sig);
    assert!(v.verify_at(raw, &h, now).is_ok());
    let mut tampered = raw.to_vec();
    tampered.push(b' ');
    assert_eq!(v.verify_at(&tampered, &h, now).unwrap_err(), Error::Webhook);
    assert!(
        v.verify_at(raw, &h, now + chrono::Duration::seconds(301))
            .is_err()
    );
    assert!(
        v.verify_at(raw, &h, now - chrono::Duration::seconds(301))
            .is_err()
    );
    let mut bad = h.clone();
    bad.insert("x-lunixi-event-id", HeaderValue::from_static("changed"));
    assert!(v.verify_at(raw, &bad, now).is_err());
    let mut bad = h.clone();
    bad.insert(
        "x-lunixi-event-type",
        HeaderValue::from_static("payment.failed"),
    );
    assert!(v.verify_at(raw, &bad, now).is_err());
    let mut bad = h.clone();
    bad.append("x-lunixi-event-id", HeaderValue::from_static("event-1"));
    assert!(v.verify_at(raw, &bad, now).is_err());
    for body in [
        br#"{"id":"changed","type":"payment.captured"}"#.as_slice(),
        br#"{"type":"payment.captured","type":"payment.failed"}"#,
        br#"{"data":{}}"#,
    ] {
        let signed = signature(secret, id, ts, body);
        assert!(v.verify_at(body, &headers(id, ts, &signed), now).is_err());
    }
    for sig in [
        "v1=unknown".into(),
        format!("v2={}", "a".repeat(63)),
        format!("v2={}", "ö".repeat(32)),
        "".into(),
    ] {
        assert!(v.verify_at(raw, &headers(id, ts, &sig), now).is_err());
    }
    assert!(WebhookVerifier::with_tolerance(vec![secret.into()], Duration::ZERO).is_err());
    assert!(WebhookVerifier::new(vec!["short".into()]).is_err());
    assert!(!format!("{v:?}").contains(secret));
}
