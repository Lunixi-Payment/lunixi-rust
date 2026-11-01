<p align="center">
  <a href="https://lunixi.com"><picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/assets/lunixi-logo-dark.svg">
    <img src=".github/assets/lunixi-logo.svg" alt="Lunixi" height="64">
  </picture></a>
</p>

# Lunixi Rust SDK

**Version 1.0.1 uses canonical minor-unit strings for money.** This is a breaking change from 0.x; update request and response handling together. See [migration](MIGRATION-1.0.md).

Historical release record: Verified crates.io release: **0.1.2**. The published archive was downloaded and its checksum verified on 2026-11-01.

The source patch adds a concrete merchant-site presentation example. **Embedded checkout is the default**: load only the official Lunixi bundle and mount the existing form; methods and multi-card choices stay in the Lunixi panel. Optional hosted mode uses the backend `paymentPageUrl` after exact HTTPS `pay.lunixi.com/payment/` validation. Its token may differ from the embedded token. Missing URLs keep embedded available and hosted on hold/reconciliation with the original attempt/key. Never derive a URL from the browser token. Authorize the customer/order and native browser CSRF before rendering, send `Cache-Control: private, no-store`, preserve the merchant CSP nonce and keep tokens/HTML/redirect URLs out of logs. Generic response data preserves `paymentPageUrl`; no new typed product parity is claimed.

[![CI](https://github.com/Lunixi-Payment/lunixi-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/Lunixi-Payment/lunixi-rust/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**[Documentation](https://docs.lunixi.com/sdks/rust) · [Türkçe](docs/README.tr.md) · [Changelog](CHANGELOG.md) · [Security](SECURITY.md)**

Asynchronous server-side Rust client for Lunixi merchant APIs. Uses Tokio and reqwest with verified Rustls TLS, Ed25519 request signing and strict raw-body v2 webhooks. Compatible with Axum, Actix Web and other Tokio applications. Minimum supported Rust: 1.89 (edition 2024).

## Installation

After version 1.0.1 is available on crates.io, add:

```toml
[dependencies]
lunixi = "1.0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

For the optional transactional SQLite inbox, add `features = ["sqlite"]` to the `lunixi` dependency. SQLite is bundled and requires a C compiler; it is not built by the default SDK feature set.

## One client per merchant

```rust
use lunixi::{Client, Config};

let mut config = Config::new(
    "https://api-gateway.lunixi.com",
    std::env::var("LUNIXI_KEY_ID")?,
    std::env::var("LUNIXI_PRIVATE_KEY_PEM")?,
);
config.max_retries = 2;
let client = Client::new(config)?;
```

Register your Ed25519 public key in the dashboard. Keep the private key on your server or secret manager. PKCS8 PEM, base64 PKCS8 DER, base64 32-byte seed and validated base64 64-byte keypair encodings are accepted. The merchant key selects TEST/LIVE; there is no environment switch. Never put private credentials in a browser bundle.

Clone and share the client: clones share the connection pool and token cache. Authentication is lazy; concurrent token acquisition is coalesced and cached with an expiry margin. Configuration/client/verifier debug output redacts credentials; response debug output omits bodies. Private-key configuration, cached token strings and retained webhook secrets use zeroizing storage. Do not log body/data accessors or serialize credentials yourself.

| Configuration | Default | Behavior |
|---|---|---|
| `timeout` | 30 seconds | HTTP request limit; positive, up to 2 minutes; also use an overall `tokio::time::timeout` |
| `max_retries` | 0 | 0–5; GET or caller-provided stable idempotency key only |
| `max_response_bytes` | 8 MiB | Up to 32 MiB; request JSON limited to 8 MiB |
| `allow_insecure_localhost` | false | HTTP allowed only for explicit loopback/localhost test origins |
| `additional_root_certificates` | empty | Trusted DER CA certificates; certificate/hostname verification stays on |

BaseURL must be a trusted HTTPS origin with no credentials, path or query. Redirects, automatic reqwest retries, environment proxies and Referer propagation are disabled. Merchant request paths cannot change origin, traverse directories or enter the authentication namespace. Never derive BaseURL or API paths from an untrusted browser request.

## Checkout: embedded by default, optional hosted mode

Load the order and basket from your server-side records. Persist the attempt key before calling the API and reuse that key/body after an uncertain outcome.

```rust
use lunixi::{BasketItem, IntentRequest};

let mut intent = IntentRequest::new(
    "10050", "TRY", "ORD-1001", "https://merchant.example/payment/return",
);
intent.basket_items.push(BasketItem::new(
    "line-1", "Example product", "PHYSICAL", "10050",
));
let response = client.payments()
    .create_intent(&intent, &persisted_attempt_key).await?;
let checkout_token = response.data.as_ref()
    .and_then(|data| data.get("token"))
    .and_then(|token| token.as_str())
    .filter(|token| !token.is_empty())
    .ok_or(lunixi::Error::Response("missing checkout token"))?;
```

Return the token only to the authenticated customer for this order, with `Cache-Control: no-store`, and initialize the existing Lunixi checkout form. Keep tokens out of URLs, console/access logs, analytics and error reports. The SDK never builds another card form or collects PAN/CVV in its default examples.

Money uses canonical minor-unit strings: `10050` = 100.50 TRY. Positive values up to `9223372036854775807` are accepted. Basket `price` is the line total; line totals must equal intent `amount`. Removed root fields `price`, `paidPrice`, `autoCapture` are rejected locally, even through the generic transport. Installments/fees belong to the Lunixi form. Buyer/address and documented insurance basket fields are available on `IntentRequest`; optional method restrictions only narrow methods already enabled in the Lunixi panel. Optional booleans use `Option<bool>` so explicit false is preserved.

**A token, browser return, HTTP success or `AUTHORIZED` status is not proof of capture.** Fulfill only after authenticating gateway state and matching stored merchant/order/payment ID, expected amount/currency and final capture/reconciliation state. Apply refund, void and dispute transitions separately.

## Coverage and responses

| Facade | Methods |
|---|---|
| `payments()` | `create_intent`, `get`, `list`, `methods`, `capture`, `refund`, `void` |
| `payments()` split | `split`, `get_split`, `refund_split` |
| `fraud()` | `evaluate`, `record_event`, `decisions`, `decision` |
| `send(Request)` | Signed access to other documented `/api/v1/` merchant endpoints |
| `WebhookVerifier` | Strict v2 HMAC, freshness, ID/type binding and rotation |
| `SqliteInbox` (`sqlite` feature) | Durable transactional replay marker + application update |

```rust
let payment = client.payments().get(&payment_id).await?;
let refund = client.payments()
    .refund(&payment_id, Some("2500"), Some("Customer return"), &persisted_refund_key)
    .await?;
```

`None` amount requests the full capture/refund. `void` releases an authorization; it does not refund a captured payment. Split accepts the documented JSON schema, checks integer leg totals, 1–5 legs and exact balance, and leaves method-specific field validation to the gateway. Pending split states may return `Response { status: 400, .. }`; they remain unpaid. Reconciliation, partial refunds and failed compensation require explicit handling.

`Response` preserves `envelope`, `data`, `page_info`, binary `bytes`, content type and safe request ID. `decode_data::<T>()` supports application-owned typed schemas. `serde_json::Value` preserves integer numbers without converting them to floating point. Unknown fields and states remain available for your application to handle safely.

Wallet, bank-connect, payment links, insurance, marketplace, subscriptions, identity and other services currently use documented REST schemas through `send`; version 1.0.1 does not claim typed helper parity for them. For example:

```rust
use lunixi::{Method, Request};
let mut request = Request::new(Method::GET, "/api/v1/payments");
request.query.push(("limit".into(), "20".into()));
let response = client.send(request).await?;
```

Every merchant call is signed. Canonical JSON follows JavaScript numeric-index key ordering and ECMAScript number formatting; duplicate keys, excessive nesting and unsafe integer numbers are rejected. The token request has no body or Digest header. Query parameters are encoded separately and included in the signed path.

Known payment mutations and typed financial helpers require a caller-persisted key of 16–128 non-space printable ASCII characters. Retries preserve key/body, use a fresh nonce, and are bounded. GET/keyed requests may retry network/read failures or 429/500/502/503/504; safe operations may refresh a rejected token once. Unkeyed mutations never automatically retry. Reconcile an uncertain result before creating another payment.

`Error::Api` exposes HTTP status, safe code and request ID. Upstream messages, response bodies, credentials and URLs are omitted from errors. Returned data can still contain customer information or tokens; do not log it.

## Raw webhooks and durable replay processing

```rust
let verifier = lunixi::WebhookVerifier::new(vec![
    std::env::var("LUNIXI_WEBHOOK_SECRET")?,
])?;
let event = verifier.verify(raw_body, headers)?;
```

Read the bounded raw bytes **before** JSON middleware. Headers use `reqwest::header::HeaderMap` (HTTP 1.x header types); adapt your framework without combining duplicate identity headers. HMAC-SHA256 covers `eventId + "." + exactTimestamp + "." + rawBody`, using constant-time verification. Freshness defaults to five minutes; `with_tolerance` allows 1–900 seconds and cannot disable it. The signed JSON body must include `type`; signed ID/type and header mismatches fail closed. Unknown signature schemes do not authorize a request. Up to three current/previous secrets support rotation.

Verification authenticates the event but does not consume its ID. With feature `sqlite`:

```rust
let inbox = lunixi::SqliteInbox::new(merchant_scope)?;
lunixi::SqliteInbox::ensure_schema(&connection)?; // startup / migration only
let applied = inbox.process(&mut connection, &event, |transaction| {
    // Match the event to the stored order and verified final payment fields.
    // Update order state and outbox rows using THIS transaction.
    apply_verified_order_transition(transaction, &event)
})?;
```

`connection` is your `rusqlite::Connection`; `apply_verified_order_transition` is your application adapter. Run synchronous SQLite work in a blocking worker rather than a Tokio request executor. Marker and state commit together; duplicates skip the callback; an ID reused with a different payload fails closed; callback/storage failure rolls back the marker so delivery can be retried. Use separate merchant scopes, retain markers for your replay policy and use an outbox for network effects. For another database, implement the same unique event marker + payload hash + business update transaction contract.

The [durable receipt example](examples/durable_webhook.rs) verifies a raw delivery from stdin and stores only event IDs/types alongside the replay marker. It is a runnable receipt consumer, not an order fulfillment implementation. Request a fresh dashboard replay for old events; never bypass timestamp checks. Do not acknowledge a persistence failure with HTTP 2xx in your HTTP adapter.

## Examples and development

```sh
cargo run --example checkout
cargo run --features sqlite --example durable_webhook < raw-webhook.json
```

Checkout requires `LUNIXI_BASE_URL`, TEST-only `LUNIXI_KEY_ID`, `LUNIXI_PRIVATE_KEY_PEM` and persisted `LUNIXI_ORDER_ID` / `LUNIXI_ATTEMPT_KEY`. The receipt consumer requires `LUNIXI_WEBHOOK_SECRET`, `LUNIXI_EVENT_ID`, `LUNIXI_SIGNATURE_TIMESTAMP`, `LUNIXI_SIGNATURE` from an authentic delivery. Supply secrets securely; `.env` is not loaded automatically. No actual credentials ship in the repository.

```sh
cargo fmt --all --check
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --doc --all-features
cargo audit
cargo deny --all-features check advisories licenses sources
cargo package --locked --allow-dirty --all-features
```

CI uses pinned actions and installs pinned security tools. Tests use genuine local TLS, independently generated Node.js JSON/Ed25519/HMAC fixtures and persistent SQLite files for rollback/restart/concurrent replay cases. All fixture keys/secrets are synthetic. No live merchant calls are required. See [contributing](CONTRIBUTING.md) and [security](SECURITY.md).

## Merchant-page integration example

The source checkout example includes [checkout_presentation.rs](https://github.com/Lunixi-Payment/lunixi-rust/blob/main/examples/support/checkout_presentation.rs): `render_checkout_html(token, nonce)` mounts the official form, while `hosted_checkout_url(paymentPageUrl)` validates an optional redirect. Adapt them in an existing authorized, CSRF-protected merchant controller. `LUNIXI_CHECKOUT_MODE` defaults to `embedded`; `hosted` is explicit. The CLI never prints the generated HTML/token/URL. These are example helpers, not additional SDK product APIs.

Before upgrading from 0.x, follow [the money-string migration](MIGRATION-1.0.md).
