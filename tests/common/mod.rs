#![allow(clippy::unwrap_used, clippy::expect_used)]
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, SigningKey, Verifier, pkcs8::DecodePrivateKey};
use lunixi::{Client, Config};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};
use tokio_rustls::{
    TlsAcceptor,
    rustls::{ServerConfig, pki_types::PrivateKeyDer},
};

pub struct Incoming {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}
pub struct Reply {
    pub status: u16,
    pub body: String,
    pub headers: Vec<(String, String)>,
}
impl Reply {
    pub fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.into(),
            headers: vec![("Content-Type".into(), "application/json".into())],
        }
    }
}
pub struct Server {
    pub base: String,
    pub cert: Vec<u8>,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    pub async fn start(handler: impl Fn(Incoming) -> Reply + Send + Sync + 'static) -> Self {
        let certified =
            rcgen::generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()])
                .unwrap();
        let cert = certified.cert.der().to_vec();
        let key = PrivateKeyDer::Pkcs8(certified.signing_key.serialize_der().into());
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certified.cert.der().clone()], key)
            .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!(
            "https://localhost:{}",
            listener.local_addr().unwrap().port()
        );
        let handler = Arc::new(handler);
        let task = tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let acceptor = acceptor.clone();
                let handler = handler.clone();
                tokio::spawn(async move {
                    let Ok(mut socket) = acceptor.accept(socket).await else {
                        return;
                    };
                    let mut raw = vec![];
                    let mut buf = [0u8; 4096];
                    let end;
                    loop {
                        let n = socket.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        raw.extend_from_slice(&buf[..n]);
                        if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                            end = i + 4;
                            break;
                        }
                        if raw.len() > 65536 {
                            return;
                        }
                    }
                    let head = String::from_utf8(raw[..end].to_vec()).unwrap();
                    let mut lines = head.split("\r\n");
                    let mut request = lines.next().unwrap().split(' ');
                    let method = request.next().unwrap().to_string();
                    let path = request.next().unwrap().to_string();
                    let mut headers = HashMap::new();
                    for line in lines {
                        if let Some((k, v)) = line.split_once(':') {
                            headers.insert(k.to_ascii_lowercase(), v.trim().to_owned());
                        }
                    }
                    let length = headers
                        .get("content-length")
                        .map_or(0, |s| s.parse::<usize>().unwrap());
                    while raw.len() < end + length {
                        let n = socket.read(&mut buf).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        raw.extend_from_slice(&buf[..n])
                    }
                    let reply = handler(Incoming {
                        method,
                        path,
                        headers,
                        body: raw[end..end + length].to_vec(),
                    });
                    let mut response = format!(
                        "HTTP/1.1 {} Response\r\nConnection: close\r\nContent-Length: {}\r\n",
                        reply.status,
                        reply.body.len()
                    );
                    for (k, v) in reply.headers {
                        response.push_str(&format!("{k}: {v}\r\n"))
                    }
                    response.push_str("\r\n");
                    response.push_str(&reply.body);
                    socket.write_all(response.as_bytes()).await.unwrap();
                    socket.shutdown().await.unwrap();
                });
            }
        });
        Self { base, cert, task }
    }
    pub fn config(&self) -> Config {
        let mut c = Config::new(&self.base, "mk_test_fixture", seed());
        c.additional_root_certificates.push(self.cert.clone());
        c
    }
    pub fn client(&self) -> Client {
        Client::new(self.config()).unwrap()
    }
}
pub fn seed() -> String {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/canonical-vectors.json")).unwrap();
    f["privateSeed"].as_str().unwrap().into()
}
pub fn validate_signature(r: &Incoming) {
    use sha2::{Digest, Sha256};
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/canonical-vectors.json")).unwrap();
    let key = SigningKey::from_pkcs8_pem(fixture["privateKeyPem"].as_str().unwrap()).unwrap();
    assert_eq!(r.headers["x-key-id"], "mk_test_fixture");
    let mut canonical = format!(
        "{}\n{}\nX-Date:{}\nX-Nonce:{}",
        r.method, r.path, r.headers["x-date"], r.headers["x-nonce"]
    );
    if !r.body.is_empty() {
        let expected = format!("SHA-256={}", STANDARD.encode(Sha256::digest(&r.body)));
        assert_eq!(r.headers["digest"], expected);
        canonical.push_str(&format!("\nDigest:{expected}"));
    } else {
        assert!(!r.headers.contains_key("digest"));
    }
    let bytes = STANDARD.decode(&r.headers["x-signature"]).unwrap();
    let sig = Signature::from_slice(&bytes).unwrap();
    key.verifying_key()
        .verify(canonical.as_bytes(), &sig)
        .unwrap();
    if r.path == "/api/v1/auth/token" {
        assert_eq!(r.method, "POST");
        assert!(r.body.is_empty());
        assert!(!r.headers.contains_key("authorization"));
    }
}

/// One recorded gateway call.
pub struct Seen {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub raw: Vec<u8>,
}
impl Seen {
    pub fn body(&self) -> serde_json::Value {
        serde_json::from_slice(&self.raw).unwrap()
    }
}
pub type Log = Arc<std::sync::Mutex<Vec<Seen>>>;

/// Answers the token and records every other call after checking its
/// signature and bearer token. `reply` gets the 0-based call index.
pub async fn recording_gateway(
    reply: impl Fn(usize, &Incoming) -> Reply + Send + Sync + 'static,
    max_retries: u8,
) -> (Server, Client, Log) {
    let seen: Log = Arc::new(std::sync::Mutex::new(vec![]));
    let log = seen.clone();
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let server = Server::start(move |r| {
        validate_signature(&r);
        if r.path == "/api/v1/auth/token" {
            return Reply::json(
                200,
                r#"{"access_token":"at_TEST_fixture","expires_in":1800}"#,
            );
        }
        assert_eq!(r.headers["authorization"], "Bearer at_TEST_fixture");
        let answer = reply(calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst), &r);
        log.lock().unwrap().push(Seen {
            method: r.method.clone(),
            path: r.path.clone(),
            headers: r.headers.clone(),
            raw: r.body.clone(),
        });
        answer
    })
    .await;
    let mut config = server.config();
    config.max_retries = max_retries;
    let client = Client::new(config).unwrap();
    (server, client, seen)
}

pub const WEBHOOK_SECRET: &str = "whsec_test_secret_0123456789";

/// A webhook envelope signed and verified like a delivery.
pub fn verified_event(envelope: &serde_json::Value) -> lunixi::WebhookEvent {
    use chrono::{SecondsFormat, Utc};
    use hmac::{Hmac, KeyInit, Mac};
    use reqwest::header::{HeaderMap, HeaderValue};
    let raw = serde_json::to_vec(envelope).unwrap();
    let id = envelope["id"].as_str().unwrap();
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(WEBHOOK_SECRET.as_bytes()).unwrap();
    mac.update(format!("{id}.{ts}.").as_bytes());
    mac.update(&raw);
    let hex: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut headers = HeaderMap::new();
    headers.insert("x-lunixi-event-id", HeaderValue::from_str(id).unwrap());
    headers.insert(
        "x-lunixi-signature-timestamp",
        HeaderValue::from_str(&ts).unwrap(),
    );
    headers.insert(
        "x-lunixi-signature",
        HeaderValue::from_str(&format!("v2={hex}")).unwrap(),
    );
    lunixi::WebhookVerifier::new(vec![WEBHOOK_SECRET.into()])
        .unwrap()
        .verify(&raw, &headers)
        .unwrap()
}

/// The buyer every payment requires (`ip` included).
pub fn buyer() -> lunixi::Buyer {
    lunixi::Buyer {
        name: "Ada".into(),
        surname: "Yilmaz".into(),
        identity_number: "11111111111".into(),
        email: "ada@example.com".into(),
        gsm_number: "+905350000000".into(),
        city: "Istanbul".into(),
        country: "Turkey".into(),
        zip_code: "34000".into(),
        ip: Some("203.0.113.7".into()),
        ..Default::default()
    }
}

/// A complete billing or shipping address.
pub fn address() -> lunixi::Address {
    lunixi::Address {
        contact_name: "Ada Yilmaz".into(),
        address: "Example Street 1".into(),
        city: "Istanbul".into(),
        country: "Turkey".into(),
        zip_code: "34000".into(),
    }
}

/// One virtual basket line of `price` with every required field.
pub fn basket_item(price: &str) -> lunixi::BasketItem {
    let mut item = lunixi::BasketItem::new("SKU-1", "Gift card", "VIRTUAL", price);
    item.category1 = Some("Digital".into());
    item
}

/// Order details of a payment of `amount` as JSON body fields.
pub fn order_details(amount: &str) -> serde_json::Value {
    serde_json::json!({
        "buyer": serde_json::to_value(buyer()).unwrap(),
        "billingAddress": serde_json::to_value(address()).unwrap(),
        "basketItems": [serde_json::to_value(basket_item(amount)).unwrap()],
    })
}

/// `body` with the order details of `amount` added.
pub fn with_order_details(mut body: serde_json::Value, amount: &str) -> serde_json::Value {
    if let (Some(target), serde_json::Value::Object(details)) =
        (body.as_object_mut(), order_details(amount))
    {
        target.extend(details);
    }
    body
}

/// An intent request of `amount` with complete order details.
pub fn intent_with_order_details(
    amount: &str,
    order_id: &str,
    callback_url: &str,
) -> lunixi::IntentRequest {
    let mut intent = lunixi::IntentRequest::new(amount, "TRY", order_id, callback_url);
    intent.buyer = Some(buyer());
    intent.billing_address = Some(address());
    intent.basket_items.push(basket_item(amount));
    intent
}
