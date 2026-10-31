use crate::{Error, MAX_SAFE_INTEGER, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::fmt;

struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Strict;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Strict, E> {
                Ok(Strict(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Strict, E> {
                Number::from_f64(v)
                    .map(|n| Strict(Value::Number(n)))
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_none<E: de::Error>(self) -> std::result::Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut out = vec![];
                while let Some(v) = a.next_element::<Strict>()? {
                    out.push(v.0)
                }
                Ok(Strict(Value::Array(out)))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut out = Map::new();
                while let Some((k, v)) = a.next_entry::<String, Strict>()? {
                    if out.contains_key(&k) {
                        return Err(de::Error::custom("duplicate key"));
                    }
                    out.insert(k, v.0);
                }
                Ok(Strict(Value::Object(out)))
            }
        }
        deserializer.deserialize_any(V)
    }
}

/// Parses JSON with duplicate-key and depth checks. The original bytes must
/// still be used for webhook verification; parsing never replaces raw HMAC.
pub fn parse_json_strict(raw: &[u8]) -> Result<Value> {
    let value: Strict = serde_json::from_slice(raw).map_err(|_| Error::Invalid("JSON body"))?;
    validate_depth(&value.0, 0)?;
    Ok(value.0)
}
fn validate_depth(v: &Value, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(Error::Invalid("JSON nesting limit"));
    }
    match v {
        Value::Array(a) => {
            for x in a {
                validate_depth(x, depth + 1)?
            }
        }
        Value::Object(o) => {
            for x in o.values() {
                validate_depth(x, depth + 1)?
            }
        }
        _ => {}
    }
    Ok(())
}

/// Compact gateway JSON.stringify-compatible encoding. Numeric index keys
/// sort first; other keys preserve serialization order. Unsafe integers fail.
pub fn canonical_json<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    let serialized = serde_json::to_vec(value).map_err(|_| Error::Invalid("JSON body"))?;
    let v = parse_json_strict(&serialized)?;
    let mut out = Vec::with_capacity(serialized.len());
    write(&v, &mut out)?;
    Ok(out)
}
fn index(s: &str) -> Option<u32> {
    let n = s.parse::<u32>().ok()?;
    (n < u32::MAX && n.to_string() == s).then_some(n)
}
fn write(v: &Value, out: &mut Vec<u8>) -> Result<()> {
    match v {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Value::String(s) => {
            serde_json::to_writer(out, s).map_err(|_| Error::Invalid("JSON string"))?
        }
        Value::Number(n) => {
            let f = n.as_f64().ok_or(Error::Invalid("JSON number"))?;
            if !f.is_finite() || (f.fract() == 0.0 && f.abs() > MAX_SAFE_INTEGER as f64) {
                return Err(Error::Invalid("unsafe JSON integer"));
            }
            let mut buffer = ryu_js::Buffer::new();
            out.extend_from_slice(buffer.format(f).as_bytes());
        }
        Value::Array(a) => {
            out.push(b'[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(b',')
                }
                write(x, out)?
            }
            out.push(b']')
        }
        Value::Object(o) => {
            let mut keys: Vec<_> = o.keys().collect();
            keys.sort_by(|a, b| match (index(a), index(b)) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            });
            out.push(b'{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(b',')
                }
                serde_json::to_writer(&mut *out, k).map_err(|_| Error::Invalid("JSON key"))?;
                out.push(b':');
                write(&o[*k], out)?
            }
            out.push(b'}');
        }
    }
    Ok(())
}
/// SHA-256 digest header for canonical request bytes.
pub fn digest(body: &[u8]) -> String {
    format!("SHA-256={}", STANDARD.encode(Sha256::digest(body)))
}
/// Exact request-signing string; path includes the serialized query.
pub fn canonical_request(
    method: &str,
    path: &str,
    date: &str,
    nonce: &str,
    digest: Option<&str>,
) -> String {
    let mut value = format!(
        "{}\n{path}\nX-Date:{date}\nX-Nonce:{nonce}",
        method.to_ascii_uppercase()
    );
    if let Some(d) = digest {
        value.push_str("\nDigest:");
        value.push_str(d)
    }
    value
}
