use crate::{Error, Result, client::safe_header, parse_json_strict};
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::Sha256;
use std::{fmt, time::Duration};
use zeroize::Zeroizing;
/// Authenticated immutable event. Fields cannot be forged through struct literals.
pub struct WebhookEvent {
    id: String,
    event_type: String,
    timestamp: DateTime<Utc>,
    raw: Vec<u8>,
    data: Value,
}
impl fmt::Debug for WebhookEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerifiedWebhookEvent")
            .field("id", &self.id)
            .field("type", &self.event_type)
            .finish_non_exhaustive()
    }
}
impl WebhookEvent {
    /// Event ID bound to the signature.
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Event type from the signed body, never an unsigned header fallback.
    pub fn event_type(&self) -> &str {
        &self.event_type
    }
    /// Signature timestamp after freshness verification.
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
    /// Exact authenticated body. May contain customer data; never log it.
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    /// Signed data, immutable; caller still must match stored order totals/state.
    pub fn data(&self) -> &Value {
        &self.data
    }
    /// Decodes signed data into an application-owned schema.
    pub fn decode_data<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_value(self.data.clone()).map_err(|_| Error::Webhook)
    }
}
/// Strict v2 HMAC verifier. Timestamp validation cannot be disabled.
pub struct WebhookVerifier {
    secrets: Vec<Zeroizing<Vec<u8>>>,
    tolerance: chrono::Duration,
}
impl fmt::Debug for WebhookVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WebhookVerifier { secrets: [REDACTED] }")
    }
}
impl WebhookVerifier {
    /// Creates a verifier with a five-minute freshness window. Supply up to
    /// three current/previous secrets to support a controlled rotation grace.
    pub fn new(secrets: Vec<String>) -> Result<Self> {
        Self::with_tolerance(secrets, Duration::from_secs(300))
    }
    /// Configures freshness from one second to fifteen minutes.
    pub fn with_tolerance(secrets: Vec<String>, tolerance: Duration) -> Result<Self> {
        if !(1..=3).contains(&secrets.len())
            || secrets.iter().any(|s| !(16..=1024).contains(&s.len()))
            || tolerance < Duration::from_secs(1)
            || tolerance > Duration::from_secs(900)
        {
            return Err(Error::Invalid("webhook configuration"));
        }
        let tolerance = chrono::Duration::from_std(tolerance)
            .map_err(|_| Error::Invalid("webhook tolerance"))?;
        Ok(Self {
            secrets: secrets
                .into_iter()
                .map(|s| Zeroizing::new(s.into_bytes()))
                .collect(),
            tolerance,
        })
    }
    /// Verifies the exact bytes, headers, freshness and signed identity/type.
    /// This does not consume an event ID; use durable transactional processing.
    pub fn verify(&self, raw: &[u8], headers: &HeaderMap) -> Result<WebhookEvent> {
        self.verify_at(raw, headers, Utc::now())
    }
    /// Same verification with an explicit trusted clock for deterministic tests.
    /// Production callers should normally use verify, not a client timestamp.
    pub fn verify_at(
        &self,
        raw: &[u8],
        headers: &HeaderMap,
        now: DateTime<Utc>,
    ) -> Result<WebhookEvent> {
        if raw.is_empty() || raw.len() > 8 << 20 {
            return Err(Error::Webhook);
        }
        let id = one(headers, "x-lunixi-event-id")?
            .filter(|s| safe_header(s, 200))
            .ok_or(Error::Webhook)?;
        let timestamp = one(headers, "x-lunixi-signature-timestamp")?
            .filter(|s| s.len() <= 100)
            .ok_or(Error::Webhook)?;
        let signature = one(headers, "x-lunixi-signature")?
            .filter(|s| s.len() <= 1024)
            .ok_or(Error::Webhook)?;
        let time = DateTime::parse_from_rfc3339(timestamp)
            .map_err(|_| Error::Webhook)?
            .with_timezone(&Utc);
        if now.signed_duration_since(time).abs() > self.tolerance {
            return Err(Error::Webhook);
        }
        let signatures: Vec<_> = signature.split(',').collect();
        if signatures.len() > 8 {
            return Err(Error::Webhook);
        }
        let mut matched = 0u8;
        for secret in &self.secrets {
            for part in &signatures {
                let Some((scheme, value)) = part.trim().split_once('=') else {
                    continue;
                };
                if scheme.trim() != "v2" {
                    continue;
                }
                let Some(actual) = unhex32(value.trim()) else {
                    continue;
                };
                let mut mac = Hmac::<Sha256>::new_from_slice(secret).map_err(|_| Error::Webhook)?;
                mac.update(id.as_bytes());
                mac.update(b".");
                mac.update(timestamp.as_bytes());
                mac.update(b".");
                mac.update(raw);
                // RustCrypto verifies in constant time. Evaluate every candidate
                // and secret, including after a match, for rotation consistency.
                matched |= u8::from(mac.verify_slice(&actual).is_ok());
            }
        }
        if matched != 1 {
            return Err(Error::Webhook);
        }
        let value = parse_json_strict(raw).map_err(|_| Error::Webhook)?;
        let typ = value
            .get("type")
            .and_then(Value::as_str)
            .filter(|s| safe_header(s, 200))
            .ok_or(Error::Webhook)?;
        if value.get("id").is_some_and(|v| v.as_str() != Some(id)) {
            return Err(Error::Webhook);
        }
        if one(headers, "x-lunixi-event-type")?.is_some_and(|h| h != typ) {
            return Err(Error::Webhook);
        }
        Ok(WebhookEvent {
            id: id.into(),
            event_type: typ.into(),
            timestamp: time,
            raw: raw.to_vec(),
            data: value.get("data").cloned().unwrap_or(Value::Null),
        })
    }
}
fn one<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>> {
    let mut it = headers.get_all(name).iter();
    let first = it.next();
    if it.next().is_some() {
        return Err(Error::Webhook);
    }
    first
        .map(|h| h.to_str().map_err(|_| Error::Webhook))
        .transpose()
}
fn unhex32(s: &str) -> Option<[u8; 32]> {
    fn nibble(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (slot, pair) in out.iter_mut().zip(s.as_bytes().as_chunks::<2>().0) {
        *slot = (nibble(pair[0])? << 4) | nibble(pair[1])?
    }
    Some(out)
}
