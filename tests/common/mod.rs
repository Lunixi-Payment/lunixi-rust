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
