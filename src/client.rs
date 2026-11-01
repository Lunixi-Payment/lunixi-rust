use crate::{Error, Result, canonical_json, canonical_request, digest, parse_json_strict};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{SecondsFormat, Utc};
use ed25519_dalek::{Signer, SigningKey, pkcs8::DecodePrivateKey};
pub use reqwest::Method;
use reqwest::{Url, redirect::Policy};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Trusted deployment configuration. Debug output redacts credentials.
pub struct Config {
    pub base_url: String,
    pub key_id: String,
    private_key: Zeroizing<String>,
    pub timeout: Duration,
    pub max_retries: u8,
    pub max_response_bytes: usize,
    /// Explicit loopback-only HTTP option for local tests. Default false.
    pub allow_insecure_localhost: bool,
    /// Additional trusted DER CA certificates; certificate verification stays on.
    pub additional_root_certificates: Vec<Vec<u8>>,
}
impl Config {
    /// Configures one merchant. Construction does not make a network request.
    pub fn new(
        base_url: impl Into<String>,
        key_id: impl Into<String>,
        private_key: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            key_id: key_id.into(),
            private_key: Zeroizing::new(private_key.into()),
            timeout: Duration::from_secs(30),
            max_retries: 0,
            max_response_bytes: 8 << 20,
            allow_insecure_localhost: false,
            additional_root_certificates: vec![],
        }
    }
}
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("credentials", &"[REDACTED]")
            .field("timeout", &self.timeout)
            .field("max_retries", &self.max_retries)
            .finish_non_exhaustive()
    }
}

struct Token {
    value: Zeroizing<String>,
    expires: Instant,
}
struct Inner {
    base: Url,
    key_id: String,
    key: SigningKey,
    http: reqwest::Client,
    max_retries: u8,
    max_response: usize,
    token: Mutex<Option<Token>>,
}
/// Cloneable asynchronous client. Clones share the token cache and HTTP pool.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}
impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LunixiClient { credentials: [REDACTED] }")
    }
}

/// A request to a documented merchant API. Do not accept Path/BaseURL from an
/// untrusted incoming request. Endpoint-specific schemas remain the caller's job.
pub struct Request {
    pub method: Method,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub body: Option<Value>,
    pub idempotency_key: Option<String>,
}
impl Request {
    /// Creates an unsigned request description; Client::send validates/signs it.
    pub fn new(method: Method, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            query: vec![],
            body: None,
            idempotency_key: None,
        }
    }
    /// Sets the JSON body. The transport canonicalizes the actual sent bytes.
    pub fn json(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }
    /// Sets a persisted key, reused unchanged on automatic retries.
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }
}
/// Preserved gateway response. Debug output excludes the body and data.
pub struct Response {
    pub status: u16,
    pub request_id: Option<String>,
    pub envelope: Option<Value>,
    pub data: Option<Value>,
    pub page_info: Option<Value>,
    pub bytes: Vec<u8>,
    pub content_type: Option<String>,
}
impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LunixiResponse")
            .field("status", &self.status)
            .field("request_id", &self.request_id)
            .finish_non_exhaustive()
    }
}
impl Response {
    /// Decodes the data member without discarding the original envelope.
    pub fn decode_data<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_value(self.data.clone().ok_or(Error::Response("missing data"))?)
            .map_err(|_| Error::Response("data schema"))
    }
}
impl Client {
    /// Builds a secure reusable client. HTTPS and no redirects are mandatory,
    /// except explicitly enabled loopback HTTP tests. No network is performed.
    pub fn new(config: Config) -> Result<Self> {
        let base = Url::parse(&config.base_url).map_err(|_| Error::Invalid("BaseURL"))?;
        let loopback = base.host_str().is_some_and(|h| {
            h == "localhost"
                || h.parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
                || h.strip_prefix('[')
                    .and_then(|v| v.strip_suffix(']'))
                    .and_then(|v| v.parse::<std::net::IpAddr>().ok())
                    .is_some_and(|ip| ip.is_loopback())
        });
        if base.host_str().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || base.path() != "/"
            || (base.scheme() != "https"
                && !(base.scheme() == "http" && config.allow_insecure_localhost && loopback))
        {
            return Err(Error::Invalid("HTTPS origin"));
        }
        if !safe_header(&config.key_id, 200)
            || config.timeout.is_zero()
            || config.timeout > Duration::from_secs(120)
            || config.max_retries > 5
            || config.max_response_bytes == 0
            || config.max_response_bytes > 32 << 20
        {
            return Err(Error::Invalid("client configuration"));
        }
        let key = parse_key(&config.private_key)?;
        let mut builder = reqwest::Client::builder()
            .redirect(Policy::none())
            .referer(false)
            .no_proxy()
            .retry(reqwest::retry::never())
            .timeout(config.timeout)
            .connect_timeout(config.timeout.min(Duration::from_secs(10)))
            .user_agent(concat!("lunixi-rust/", env!("CARGO_PKG_VERSION")));
        if config.additional_root_certificates.len() > 8 {
            return Err(Error::Invalid("CA certificate count"));
        }
        for cert in &config.additional_root_certificates {
            if cert.len() > 64 << 10 {
                return Err(Error::Invalid("CA certificate size"));
            }
            builder = builder.add_root_certificate(
                reqwest::Certificate::from_der(cert)
                    .map_err(|_| Error::Invalid("CA certificate"))?,
            );
        }
        let http = builder.build().map_err(|_| Error::Invalid("TLS client"))?;
        Ok(Self {
            inner: Arc::new(Inner {
                base,
                key_id: config.key_id,
                key,
                http,
                max_retries: config.max_retries,
                max_response: config.max_response_bytes,
                token: Mutex::new(None),
            }),
        })
    }
    /// Clears the token cache. Key rotation requires a new Client.
    pub async fn invalidate_token(&self) {
        *self.inner.token.lock().await = None;
    }
    /// Sends a validated signed public merchant request. Retry only occurs for
    /// GET or a caller-provided stable key; unkeyed mutations never retry.
    pub async fn send(&self, request: Request) -> Result<Response> {
        validate_path(&request.path)?;
        if ![
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ]
        .contains(&request.method)
        {
            return Err(Error::Invalid("HTTP method"));
        }
        if request
            .idempotency_key
            .as_ref()
            .is_some_and(|k| !valid_key(k))
        {
            return Err(Error::Invalid("idempotency key"));
        }
        if request.body.is_some() && request.method == Method::GET {
            return Err(Error::Invalid("GET body"));
        }
        validate_payment_body(&request.path, request.body.as_ref())?;
        if request.method == Method::POST && financial_mutation(&request.path) {
            require_key(request.idempotency_key.as_deref().unwrap_or(""))?;
        }

        if crate::money::fiat_route(&request.path)
            && let Some(value) = &request.body
        {
            crate::money::validate_fields(value, false)?;
        }
        let body = request.body.as_ref().map(canonical_json).transpose()?;
        if body.as_ref().is_some_and(|b| b.len() > 8 << 20) {
            return Err(Error::Invalid("request body size"));
        }
        let mut url = self.inner.base.clone();
        url.set_path(&request.path);
        if !request.query.is_empty() {
            url.query_pairs_mut().extend_pairs(request.query.iter());
        }
        if url.as_str().len() > 16384 {
            return Err(Error::Invalid("request URL length"));
        }
        let path = if let Some(query) = url.query() {
            format!("{}?{query}", url.path())
        } else {
            url.path().to_owned()
        };
        let safe = request.method == Method::GET || request.idempotency_key.is_some();
        let mut refreshed = false;
        let mut retries = 0;
        loop {
            let token = self.access_token().await?;
            let result = self
                .exchange(
                    &request.method,
                    &url,
                    &path,
                    body.as_deref(),
                    request.idempotency_key.as_deref(),
                    Some(&token),
                )
                .await;
            match result {
                Ok(response)
                    if (200..300).contains(&response.status)
                        || split_pending(&request.path, &response) =>
                {
                    if crate::money::fiat_route(&request.path)
                        && let Some(value) = &response.envelope
                    {
                        crate::money::validate_fields(value, true)?;
                    }
                    return Ok(response);
                }
                Ok(response) if response.status == 401 && safe && !refreshed => {
                    let mut cache = self.inner.token.lock().await;
                    if cache
                        .as_ref()
                        .is_some_and(|t| t.value.as_str() == token.as_str())
                    {
                        *cache = None
                    }
                    refreshed = true;
                }
                result => {
                    let retry = match &result {
                        Err(Error::Transport | Error::Response(_)) => true,
                        Ok(r) => [429, 500, 502, 503, 504].contains(&r.status),
                        _ => false,
                    };
                    if safe && retry && retries < self.inner.max_retries {
                        tokio::time::sleep(Duration::from_millis(100u64 << retries)).await;
                        retries += 1;
                        continue;
                    }
                    return result.and_then(|r| Err(api_error(&r)));
                }
            }
        }
    }
    async fn access_token(&self) -> Result<Zeroizing<String>> {
        // Holding the async mutex across one auth request gives single-flight
        // acquisition. Cancellation drops the lock without claiming a token.
        let mut cache = self.inner.token.lock().await;
        if let Some(t) = cache.as_ref().filter(|t| Instant::now() < t.expires) {
            return Ok(Zeroizing::new(t.value.to_string()));
        }
        let url = self
            .inner
            .base
            .join("api/v1/auth/token")
            .map_err(|_| Error::Invalid("auth URL"))?;
        let r = self
            .exchange(&Method::POST, &url, "/api/v1/auth/token", None, None, None)
            .await?;
        if !(200..300).contains(&r.status) {
            return Err(api_error(&r));
        }
        let auth = r.data.as_ref().ok_or(Error::Response("auth data"))?;
        let value = auth
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|v| safe_header(v, 8192))
            .ok_or(Error::Response("access token"))?;
        let ttl = auth
            .get("expires_in")
            .and_then(Value::as_u64)
            .filter(|v| *v > 0 && *v <= 86400)
            .ok_or(Error::Response("token expiry"))?;
        let duration = Duration::from_secs(ttl);
        let skew = if ttl <= 30 {
            duration / 2
        } else {
            Duration::from_secs(30)
        };
        *cache = Some(Token {
            value: Zeroizing::new(value.to_owned()),
            expires: Instant::now() + duration - skew,
        });
        Ok(Zeroizing::new(value.to_owned()))
    }
    async fn exchange(
        &self,
        method: &Method,
        url: &Url,
        path: &str,
        body: Option<&[u8]>,
        key: Option<&str>,
        token: Option<&str>,
    ) -> Result<Response> {
        let nonce = Uuid::new_v4().to_string();
        let date = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let digest = body.map(digest);
        let canonical = canonical_request(method.as_str(), path, &date, &nonce, digest.as_deref());
        let signature = STANDARD.encode(self.inner.key.sign(canonical.as_bytes()).to_bytes());
        let mut request = self
            .inner
            .http
            .request(method.clone(), url.clone())
            .header("Accept", "application/json")
            .header("X-Key-Id", &self.inner.key_id)
            .header("X-Date", date)
            .header("X-Nonce", nonce)
            .header("X-Signature", signature);
        if let Some(b) = body {
            request = request
                .header("Digest", digest.ok_or(Error::Invalid("digest"))?)
                .header("Content-Type", "application/json")
                .body(b.to_vec())
        }
        if let Some(k) = key {
            request = request.header("Idempotency-Key", k)
        }
        if let Some(t) = token {
            request = request.bearer_auth(t)
        }
        let mut response = request.send().await.map_err(|_| Error::Transport)?;
        let status = response.status().as_u16();
        let request_id = response
            .headers()
            .get("X-Request-Id")
            .and_then(|h| h.to_str().ok())
            .filter(|s| safe_header(s, 200))
            .map(str::to_owned);
        let content_type = response
            .headers()
            .get("Content-Type")
            .and_then(|h| h.to_str().ok())
            .map(str::to_owned);
        if response
            .content_length()
            .is_some_and(|n| n > self.inner.max_response as u64)
        {
            return Err(Error::Response("size limit"));
        }
        let mut raw = vec![];
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| Error::Response("read failure"))?
        {
            if chunk.len() > self.inner.max_response.saturating_sub(raw.len()) {
                return Err(Error::Response("size limit"));
            }
            raw.extend_from_slice(&chunk)
        }
        let looks_json = content_type
            .as_ref()
            .is_some_and(|s| s.to_ascii_lowercase().contains("json"))
            || raw
                .iter()
                .find(|b| !b.is_ascii_whitespace())
                .is_some_and(|b| *b == b'{' || *b == b'[');
        let mut r = Response {
            status,
            request_id,
            envelope: None,
            data: None,
            page_info: None,
            bytes: vec![],
            content_type,
        };
        if looks_json {
            let value = parse_json_strict(&raw).map_err(|_| Error::Response("JSON body"))?;
            r.data = Some(value.get("data").cloned().unwrap_or_else(|| value.clone()));
            r.page_info = value.get("pageInfo").cloned();
            r.envelope = Some(value)
        } else {
            r.bytes = raw
        }
        Ok(r)
    }
}
fn parse_key(value: &str) -> Result<SigningKey> {
    if value.contains("-----BEGIN") {
        return SigningKey::from_pkcs8_pem(value).map_err(|_| Error::Invalid("Ed25519 PKCS8 key"));
    }
    let raw = Zeroizing::new(
        STANDARD
            .decode(value.trim())
            .map_err(|_| Error::Invalid("base64 private key"))?,
    );
    if let Ok(key) = SigningKey::from_pkcs8_der(&raw) {
        return Ok(key);
    }
    if let Ok(seed) = <&[u8; 32]>::try_from(raw.as_slice()) {
        return Ok(SigningKey::from_bytes(seed));
    }
    if let Ok(pair) = <&[u8; 64]>::try_from(raw.as_slice()) {
        return SigningKey::from_keypair_bytes(pair)
            .map_err(|_| Error::Invalid("Ed25519 private/public pair"));
    }
    Err(Error::Invalid("Ed25519 PKCS8 key or seed"))
}
pub(crate) fn safe_header(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.bytes().all(|b| (0x21..=0x7e).contains(&b))
}
pub(crate) fn valid_key(s: &str) -> bool {
    s.len() >= 16 && safe_header(s, 128)
}
pub(crate) fn require_key(s: &str) -> Result<()> {
    if valid_key(s) {
        Ok(())
    } else {
        Err(Error::Invalid("stable idempotency key (16-128 ASCII)"))
    }
}
pub(crate) fn valid_id(s: &str) -> Result<&str> {
    if s.len() > 200
        || s.is_empty()
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
        || s == "."
        || s == ".."
    {
        Err(Error::Invalid("resource ID"))
    } else {
        Ok(s)
    }
}
fn validate_path(path: &str) -> Result<()> {
    if !path.starts_with("/api/v1/")
        || path.len() > 4096
        || path.contains("//")
        || path.split('/').any(|s| s == "." || s == "..")
        || path == "/api/v1/auth"
        || path.starts_with("/api/v1/auth/")
        || !path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~/:".contains(&b))
    {
        Err(Error::Invalid("merchant API path"))
    } else {
        Ok(())
    }
}
fn validate_payment_body(path: &str, body: Option<&Value>) -> Result<()> {
    let segments: Vec<_> = path.trim_start_matches('/').split('/').collect();
    let optional_operation = segments.len() == 5
        && segments[..3] == ["api", "v1", "payments"]
        && matches!(segments[4], "capture" | "refund")
        || segments.len() == 6
            && segments[..4] == ["api", "v1", "payments", "split"]
            && segments[5] == "refund";
    if optional_operation {
        if let Some(amount) = body.and_then(|value| value.get("amount")) {
            crate::money::parse_minor(
                amount
                    .as_str()
                    .ok_or(Error::Invalid("explicit operation amount"))?,
                true,
                false,
            )?;
        }
        return Ok(());
    }
    if ![
        "/api/v1/payments/intents",
        "/api/v1/payments/marketplace/intents",
        "/api/v1/payments/direct/payment",
    ]
    .contains(&path)
    {
        return Ok(());
    }
    let object = body
        .and_then(Value::as_object)
        .ok_or(Error::Invalid("payment JSON object"))?;
    if ["price", "paidPrice", "autoCapture"]
        .iter()
        .any(|k| object.contains_key(*k))
    {
        return Err(Error::Invalid("removed payment field"));
    }
    if !object
        .get("amount")
        .and_then(Value::as_str)
        .is_some_and(|v| crate::money::parse_minor(v, true, false).is_ok())
    {
        return Err(Error::Invalid("positive integer minor-unit amount"));
    }
    Ok(())
}
fn api_error(r: &Response) -> Error {
    let code = r
        .envelope
        .as_ref()
        .and_then(|v| v.get("code"))
        .and_then(Value::as_str)
        .filter(|s| {
            s.len() <= 128
                && !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
        })
        .unwrap_or("HTTP_ERROR")
        .to_owned();
    Error::Api {
        status: r.status,
        code,
        request_id: r.request_id.clone(),
    }
}
fn split_pending(path: &str, r: &Response) -> bool {
    if path != "/api/v1/payments/split" || r.status != 400 {
        return false;
    }
    let Some(data) = r.data.as_ref() else {
        return false;
    };
    data.get("splitPaymentId")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
        && data.get("status").and_then(Value::as_str).is_some_and(|s| {
            [
                "COMPOSING",
                "AUTHORIZING",
                "AWAITING_3DS",
                "AWAITING_ACTION",
                "AWAITING_CREDIT_APPROVAL",
                "AWAITING_REPLACEMENT_CARD",
                "AUTHORIZED",
                "CAPTURING",
            ]
            .contains(&s)
        })
}

fn financial_mutation(path: &str) -> bool {
    path == "/api/v1/payments/intents"
        || path == "/api/v1/payments/direct/payment"
        || path == "/api/v1/payments/marketplace/intents"
        || path == "/api/v1/payments/split"
        || (path.starts_with("/api/v1/payments/")
            && ["/capture", "/refund", "/void"]
                .iter()
                .any(|suffix| path.ends_with(suffix)))
}
