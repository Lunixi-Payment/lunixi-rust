<p align="center">
  <a href="https://lunixi.com"><picture>
    <source media="(prefers-color-scheme: dark)" srcset=".github/assets/lunixi-logo-dark.svg">
    <img src=".github/assets/lunixi-logo.svg" alt="Lunixi" height="64">
  </picture></a>
</p>

# Lunixi Rust SDK

**Version 1.0.2 uses canonical minor-unit strings for money.** This is a breaking change from 0.x; update request and response handling together. See [migration](MIGRATION-1.0.md).

Historical release record: Verified crates.io release: **0.1.2**. The published archive was downloaded and its checksum verified on 2026-11-01.

The source patch adds a concrete merchant-site presentation example. **Embedded checkout is the default**: load only the official Lunixi bundle and mount the existing form; methods and multi-card choices stay in the Lunixi panel. Optional hosted mode uses the backend `paymentPageUrl` after exact HTTPS `pay.lunixi.com/payment/` validation. Its token may differ from the embedded token. Missing URLs keep embedded available and hosted on hold/reconciliation with the original attempt/key. Never derive a URL from the browser token. Authorize the customer/order and native browser CSRF before rendering, send `Cache-Control: private, no-store`, preserve the merchant CSP nonce and keep tokens/HTML/redirect URLs out of logs. Generic response data preserves `paymentPageUrl`; no new typed product parity is claimed.

[![CI](https://github.com/Lunixi-Payment/lunixi-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/Lunixi-Payment/lunixi-rust/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**[Documentation](https://docs.lunixi.com/sdks/rust) · [Türkçe](docs/README.tr.md) · [Changelog](CHANGELOG.md) · [Security](SECURITY.md)**

Asynchronous server-side Rust client for Lunixi merchant APIs. Uses Tokio and reqwest with verified Rustls TLS, Ed25519 request signing and strict raw-body v2 webhooks. Compatible with Axum, Actix Web and other Tokio applications. Minimum supported Rust: 1.89 (edition 2024).

## Installation

Install the published [1.0.2 release](https://crates.io/crates/lunixi/1.0.2):

```toml
[dependencies]
lunixi = "2.0.0"
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
use lunixi::{Address, BasketItem, Buyer, IntentRequest};

let mut intent = IntentRequest::new(
    "10050", "TRY", "ORD-1001", "https://merchant.example/payment/return",
);
// Buyer, billing address and basket are required (take them from your order,
// never a placeholder); a PHYSICAL item also needs the shipping address. The
// buyer IP is optional on an intent: the Lunixi form takes it from the browser.
intent.buyer = Some(Buyer {
    name: "Ada".into(), surname: "Yılmaz".into(),
    identity_number: "11111111111".into(), email: "ada@example.com".into(),
    gsm_number: "+905000000000".into(), city: "Istanbul".into(),
    country: "Turkey".into(), zip_code: "34732".into(),
    ..Default::default()
});
intent.billing_address = Some(Address {
    contact_name: "Ada Yılmaz".into(), address: "Altunizade Mah. Örnek Sok. No: 1".into(),
    city: "Istanbul".into(), country: "Turkey".into(), zip_code: "34662".into(),
});
let mut line = BasketItem::new("line-1", "Consulting service", "VIRTUAL", "10050");
line.category1 = Some("Services".into());
intent.basket_items.push(line);
// Optional: the form offers at most 3 installments for this order.
intent.enabled_installments = Some(vec![1, 2, 3]);
let response = client.payments()
    .create_intent(&intent, &persisted_attempt_key).await?;
let checkout_token = response.data.as_ref()
    .and_then(|data| data.get("token"))
    .and_then(|token| token.as_str())
    .filter(|token| !token.is_empty())
    .ok_or(lunixi::Error::Response("missing checkout token"))?;
```

Every payment body carries the order details: `buyer` (`name`, `surname`, `identity_number` — Turkish ID or passport number, `email` — ASCII, `gsm_number`, `city`, `country`, `zip_code`, `ip` — the buyer's browser IP), `billing_address` (`contact_name`, `address`, `city`, `country`, `zip_code`) and at least one basket item (`id`, `name`, `category1`, `item_type`, `price`); a `PHYSICAL` item also needs `shipping_address`. On `create_intent`, `create_marketplace_intent` and marketplace `create_checkout_intent` the buyer `ip` may be left out (`CHECKOUT_INTENT_BUYER_OPTIONAL_FIELDS`). These calls, `direct_method_payment`, `split`, the marketplace card charges, `store_card` (buyer and billing address only) and the wallet card top-up return `Error::MissingOrderDetails` naming the missing paths (`buyer.email`, `basketItems.0.category1`, `shippingAddress`) before any request; `missing_order_details(&body, OrderDetailsScope::Payment)` returns the same list. The gateway answers a missing field with HTTP 400 `payment.order.field_required` and an invalid one with `payment.order.field_invalid` (`order_field_error_code`, `order_field_reason`; the lists are `ORDER_DETAILS_REQUIRED_FIELDS`, `BUYER_REQUIRED_FIELDS`, `ADDRESS_REQUIRED_FIELDS`, `BASKET_ITEM_REQUIRED_FIELDS`).

Return the token only to the authenticated customer for this order, with `Cache-Control: no-store`, and initialize the existing Lunixi checkout form. Keep tokens out of URLs, console/access logs, analytics and error reports. The SDK never builds another card form or collects PAN/CVV in its default examples.

Money uses canonical minor-unit strings: `10050` = 100.50 TRY. Positive values up to `9223372036854775807` are accepted. Basket `price` is the line total; line totals must equal intent `amount`. Removed root fields `price`, `paidPrice`, `autoCapture` are rejected locally, even through the generic transport. Installments/fees belong to the Lunixi form; `enabled_installments` only narrows the installment counts the form offers for this payment (`Some(vec![1])` = single payment only). The form shows only the counts your panel settings, the card and the platform maximum also allow; `None` means no restriction. Counts are 1–36, distinct and include 1, otherwise `create_intent` returns `Error::Invalid` before any request; the list is sent as `enabledInstallments` in ascending order. Documented insurance basket fields are available on `BasketItem`; optional method restrictions only narrow methods already enabled in the Lunixi panel. Optional `BasketItem::product_category` (sent as `productCategory`) is the Lunixi product category, used by payment methods with legal installment limits (e.g. shopping loan) and other platform features; use a `lunixi::product_category` constant (`product_category::ALL` lists the codes known to this version). `None` omits the field and keeps today's behaviour; an unknown code is rejected by the API with HTTP 400. The dictionary only grows, so the field stays an `Option<String>`. Optional booleans use `Option<bool>` so explicit false is preserved.

**A token, browser return, HTTP success or `AUTHORIZED` status is not proof of capture.** Fulfill only after authenticating gateway state and matching stored merchant/order/payment ID, expected amount/currency and final capture/reconciliation state. Apply refund, void and dispute transitions separately.

## Coverage and responses

| Facade | Methods |
|---|---|
| `payments()` | `create_intent`, `get`, `list`, `methods` / `list_payment_methods`, `capture`, `refund`, `void` |
| `payments()` methods | `direct_method_payment`, `confirm_payment_action`, `resend_payment_action` |
| `payments()` promissory notes | `get_promissory_notes`, `cancel_promissory_notes`, `download_promissory_note_document` |
| `payments()` bank transfer | `list_bank_transfer_exceptions`, `accept_bank_transfer_exception`, `mark_bank_transfer_exception_refunded` |
| `payments()` split | `split`, `get_split`, `list_splits`, `refund_split` |
| `payments()` cards | `bin_info`, `installment_options`, `card_taxonomy`, `store_card`, `list_stored_cards`, `get_stored_card`, `deactivate_stored_card` |
| `payments()` marketplace, analytics | `create_marketplace_intent`, `failover_recovery_analytics`, `provider_credential_analytics` |
| `payment_links()` | `create`, `list`, `get`, `update`, `pause`, `resume`, `deactivate`, `action`, `send`, `qr`, `list_attempts`, `list_payments`, `create_image_upload`, `put_image_upload`, `confirm_image_upload`, `upload_image` |
| `reference()` | `list_provinces`, `list_districts`, `list_neighborhoods` |
| `physical_pos()` | `create_payment`, `create_reversal`, `cancel_payment`, `get_payment`, `list_payments`, `list_transactions`, `get_transaction`, `list_ambiguous_payments`, `get_sales_summary`, `get_tax_summary`, `list_settlements`, `start_settlement`, `list_settlement_runs`, `get_settlement_run` |
| `physical_pos_management()` | overview and catalog, stores, registers, logical terminals with bank and provider rows and device links, devices with identity keys, end-of-day policy (account, store, device, effective) |
| `fraud()` | `evaluate`, `record_event`, `record_batch`, `decisions`, `decision`, `latest_for_payment` |
| `wallet()` | `ping` and the sub-facades `programs()`, `end_users()`, `wallets()`, `deposits()`, `corporate()`, `fees()`, `transfers()`, `withdrawals()`, `operations()`, `topups()`, `payments()`, `qr()`, `bulk_payouts()` |
| `bank_connect()` | `list_banks`, `list_connections`, `get_connection`, `list_accounts`, `get_account`, `request_sync`, `list_balances`, `list_transactions`, `get_transaction`; `transfers()` (`list_purposes`, `create`, `list`, `get`), `fx()` (`list_rates`, `create_quote`, `create_order`, `list_orders`, `get_order`) |
| `sanction()` | `screen_name`, `screen_wallet`, `screen_account`, `screen_transaction`, `screen_public_procurement_ban`, `screen_adverse_media`, `evaluate_behavioral_alert`, `list_screening_history`, monitoring, entity intelligence, `list_blacklist_entries` |
| `insurance()` | products, reference values, quotes, orders, premium payment and refund, policy files, reconciliation, leads |
| `subscriptions()` | `create`, `get`, `list`, `cancel`, `pause`, `resume`, `change_plan`, `audit_logs`; `plans()`, `customers()`, `cards()`, `invoices()`, `portal()`, `products()`, `features()`, `addons()`, `usage()` |
| `kyc()` | `create_session`, `get_session`, `list_sessions`, `set_jurisdiction`, `set_primary_document`, `upload_document`, `timeline`, `submit`, `manual_review`, `evidence_request`, `merchant_policy`, `update_merchant_policy`, `rotate_webhook_secret`, `webhook_outbox`, `retry_webhook_event`, `erasure_requests` |
| `identity()` | `analyze_sync`, `analyze`, `account_links`, `get_session`, `related_sessions`, `timeline`, `top_sources`, `list_sessions` |
| `marketplace()` | split charges (`create_checkout_intent`, `charge_marketplace_card`, `charge_marketplace_card2d`, `charge_marketplace_card3d`), sub-dealers, commissions, payouts, audit log |
| `send(Request)` | Signed access to other documented `/api/v1/` merchant endpoints |
| `WebhookVerifier` | Strict v2 HMAC, freshness, ID/type binding and rotation |
| `SqliteInbox` (`sqlite` feature) | Durable transactional replay marker + application update |

```rust
let payment = client.payments().get(&payment_id).await?;
let refund = client.payments()
    .refund(&payment_id, Some("2500"), Some("Customer return"), &persisted_refund_key)
    .await?;
```

`None` amount requests the full capture/refund. `void` releases an authorization; it does not refund a captured payment. Split accepts the documented JSON schema, checks integer leg totals, 1–5 legs and exact balance, and the leg blocks: a card leg carries no method block, a non-card leg carries exactly its own `mobileBilling`, `wallet` or `promissoryNote` block (`MEAL_CARD`, `OPEN_BANKING`, `SHOPPING_LOAN` and `BANK_TRANSFER` cannot be legs). The split body (`SPLIT_PAYMENT_FIELDS`) carries the order details too: `buyer`, `billingAddress`, `basketItems` and, for a `PHYSICAL` item, `shippingAddress`. Other method-specific fields are validated by the gateway. `list_splits` takes the filters in `LIST_SPLIT_PAYMENTS_FILTERS` as query pairs. Split webhook event names are in `split_payment_events`; add `SPLIT_LEG_EVENTS_SUBSCRIPTION` to an endpoint's event types to also receive leg events. Pending split states may return `Response { status: 400, .. }`; they remain unpaid. Reconciliation, partial refunds and failed compensation require explicit handling.

`Response` preserves `envelope`, `data`, `page_info`, binary `bytes`, content type and safe request ID. `decode_data::<T>()` supports application-owned typed schemas. `serde_json::Value` preserves integer numbers without converting them to floating point. Unknown fields and states remain available for your application to handle safely.

Other services use their documented REST schemas through `send`. For example:

```rust
use lunixi::{Method, Request};
let mut request = Request::new(Method::GET, "/api/v1/payments");
request.query.push(("limit".into(), "20".into()));
let response = client.send(request).await?;
```

Every merchant call is signed. Canonical JSON follows JavaScript numeric-index key ordering and ECMAScript number formatting; duplicate keys, excessive nesting and unsafe integer numbers are rejected. The token request has no body or Digest header. Query parameters are encoded separately and included in the signed path.

Known payment mutations and typed financial helpers require a caller-persisted key of 16–128 non-space printable ASCII characters. Retries preserve key/body, use a fresh nonce, and are bounded. GET/keyed requests may retry network/read failures or 429/500/502/503/504; safe operations may refresh a rejected token once. Unkeyed mutations never automatically retry. Reconcile an uncertain result before creating another payment.

`Error::Api` exposes HTTP status, safe code and request ID. Upstream messages, response bodies, credentials and URLs are omitted from errors. Returned data can still contain customer information or tokens; do not log it.

## Payment methods

`list_payment_methods(None)` (same as `methods`) returns the methods open for your account with their `blockName`, `providerSelection`, `providerSelectionMode` and providers with the `inputFields` to send. Always read the provider code from this answer; never hard-code one. `Some(code)` lists one method; codes are checked locally and `QR` is sent as `QR_CODE`. Codes and value sets are constants: `lunixi::payment_method`, `payment_action_type`, `provider_selection`, `provider_selection_mode`.

`direct_method_payment` starts a payment with any method from your server. Build the body with `DirectMethodPaymentRequest` or pass a raw `serde_json::Value`; both are checked before any request (known fields only, only the chosen method's block, card fields only with `CARD`, block structure, promissory-note plan, shopping-loan basket of 1–100 items, the order details). The persisted idempotency key is required.

```rust
use lunixi::{DirectMethodPaymentRequest, payment_method};
use serde_json::json;

let request = DirectMethodPaymentRequest::new("10000", "TRY", "ORD-1001")
    .with_method_fields(
        payment_method::MOBILE_BILLING,
        Some(&provider_code), // from list_payment_methods
        json!({ "gsmNumber": "+905321234567" }),
    )
    // Order details as above; here the buyer `ip` is required.
    .with_buyer(buyer)
    .with_billing_address(billing_address)
    .with_basket_items(basket_items);
let response = client.payments()
    .direct_method_payment(&request, &persisted_attempt_key).await?;
```

When `data.action` is present (HTTP 202) the buyer must act: `OTP` and `INPUT` are completed with `confirm_payment_action(payment_id, action_id, json!({ "otp": "123456" }))` (only `otp`, `consentAccepted` and `fields` are sent), `resend_payment_action` sends a new code, `REDIRECT` sends the buyer to `action.redirect.url`. Cards use `with_card(card)` and `with_force_3d(true)` for 3D Secure.

### Bank transfer

`BANK_TRANSFER` takes no provider code and the buyer's bank as `{ "bankCode": … }` (`BANK_TRANSFER_OTHER_BANKS_CODE`, `"9999"`, for other banks). The answer carries a `DISPLAY` action with the transfer instruction:

```rust
use lunixi::{BankTransferInstruction, DirectMethodPaymentRequest, payment_method};

let request = DirectMethodPaymentRequest::new("10000", "TRY", "ORD-1002")
    .with_method_fields(payment_method::BANK_TRANSFER, None, json!({ "bankCode": "0062" }))
    .with_buyer(buyer)
    .with_billing_address(billing_address)
    .with_basket_items(basket_items);
let response = client.payments()
    .direct_method_payment(&request, &persisted_attempt_key).await?;
if let Some(instruction) = BankTransferInstruction::from_response(&response) {
    // Show iban, holder_name, amount_minor and payment_code to the buyer.
}
```

The money is matched from your connected bank account; fulfill on `payment.captured`. Late, short and excess transfers become exceptions: `list_bank_transfer_exceptions(&BankTransferExceptionFilters::default().with_status("PENDING_DECISION"))`, then `accept_bank_transfer_exception(id)` or, after you refunded the sender yourself, `mark_bank_transfer_exception_refunded(id)`. Events: `payment_bank_transfer_events`.

### Promissory notes

`PROMISSORY_NOTE` needs a plan: `with_promissory_note_schedule(vec![PromissoryNoteScheduleRow::new("5000", "2027-01-15"), …])` (1–24 rows, amounts adding up to `amount`, ascending `YYYY-MM-DD` dates) or `with_promissory_note_count(n)` (1–24). After the payment, `get_promissory_notes(payment_id)` lists the notes, `cancel_promissory_notes(payment_id, None, None)` cancels every open note (or `Some(&["SN-…"])` for some), and `download_promissory_note_document(payment_id, Some("tr-TR"))` returns the printable PDF in `Response::bytes`. Debtor addresses use the codes of `client.reference()`: `list_provinces`, `list_districts("06")`, `list_neighborhoods(district_code)`. Events: `promissory_note_events`.

## Cards, installments and marketplace checkout

`bin_info("41111111")` returns card, bank and installment eligibility of a 6–19 digit prefix. `installment_options(&InstallmentOptionsRequest::new("10000", "TRY").with_bin_or_pan(bin).with_installment(3))` returns the installment table; with a BIN and an installment above 1 `data` carries the `quoteToken` that `direct_method_payment` sends as `installmentQuoteToken`. `with_enabled_installments` follows the same rule as the intent (1–36, distinct, includes 1). `card_taxonomy()` lists brands, families and banks.

`store_card(&StoreCardRequest::new(card, card_user_key, callback_url).with_buyer(buyer).with_billing_address(address), key)` stores a card for your card holder (the buyer and billing address are required); `list_stored_cards(card_user_key)`, `get_stored_card(token)` and `deactivate_stored_card(token, key)` manage them. The card number and security code never appear in debug output or SDK errors; do not log the request yourself.

`create_marketplace_intent` takes an `IntentRequest` (with its order details) and its sellers:

```rust
use lunixi::{MarketplaceIntentRequest, MarketplaceSeller};

let request = MarketplaceIntentRequest::new(intent, vec![
    MarketplaceSeller::new("dealer-1", "6000"),
    MarketplaceSeller::new("dealer-2", "4000"),
]);
let response = client.payments()
    .create_marketplace_intent(&request, &persisted_attempt_key).await?;
```

The sellers' `amountMinor` values must add up to `amount`; any other total fails locally before a request.

## Payment links

`client.payment_links()` creates and manages payment links. Every call is signed, reads included; the signing key decides the environment.

```rust
use lunixi::{CreatePaymentLinkRequest, PaymentLinkSendRequest, payment_link_usage};

let request = CreatePaymentLinkRequest::fixed(
    payment_link_usage::SINGLE_USE, "150000", "TRY", "Invoice 42",
)
.with_recipient(json!({ "email": "buyer@example.com" }));
let link = client.payment_links().create(&request, &persisted_link_key).await?;
client.payment_links()
    .send(&link_id, &PaymentLinkSendRequest::new("EMAIL"), &persisted_send_key)
    .await?;
```

`create` also accepts a raw `serde_json::Value`; both are checked against `PAYMENT_LINK_SPEC_FIELDS` and the value sets before sending. `update(link_id, expected_row_version, &changes, key)` changes only the given keys (`null` clears one; `usage`, `amountMode` and `currency` are fixed). `pause`, `resume` and `deactivate` take `PaymentLinkActionOptions`. `list`, `list_attempts` and `list_payments` keep `page_info` (`hasMore`, `nextCursor`). `qr(link_id, Some("png"), Some(512))` returns the image in `Response::bytes`.

A link image takes three steps, or one call to `upload_image(file_name, content_type, bytes)`: `create_image_upload` returns the upload target, `put_image_upload(&target, bytes)` sends the file straight to the storage host with only the target's `uploadHeaders` (never the access token or the request signature; not retried), and `confirm_image_upload(asset_id)` publishes it. Use the returned `assetId` as a link's or an item's `imageAssetId`. PNG, JPEG and WebP up to 2 MiB are accepted.

## Physical POS

`client.physical_pos()` starts and follows card payments on in-store devices; `client.physical_pos_management()` manages stores, registers, logical terminals, devices and the end-of-day policy. Requests and filters are JSON objects checked against `PHYSICAL_POS_REQUEST_FIELDS` and `PHYSICAL_POS_FILTERS` before sending: unknown fields, wrong types, missing required fields, a `rowVersion` outside 1–2147483647 and a `limit` outside 1–100 are `Error::Invalid`. Money is canonical minor-unit text (`"4500"`); counts and quantities are JSON numbers.

```rust
use lunixi::{Error, PhysicalPos};
use serde_json::json;

let key = PhysicalPos::new_idempotency_key(); // store it with the order first
let sale = json!({
    "operation": "SALE",
    "target": { "deviceId": device_id },
    "amountMinor": "4500",
    "currency": "TRY",
    "merchantReference": order_id,
    "allowDuplicateReference": false,
});
match client.physical_pos().create_payment(&sale, &key).await {
    Ok(_written) => { /* 202 accepted (not paid) or 200 replayed: follow the webhooks */ }
    Err(Error::PhysicalPosOutcomeUnknown { .. }) => { /* the payment may exist: retry later with the SAME key */ }
    Err(other) => return Err(other.into()),
}
```

A payment write means "accepted", never "paid": follow the outcome with the `physical_pos.payment.*` webhooks or `get_payment`. `create_payment` and `create_reversal` require your stored `Idempotency-Key` (8–200 printable ASCII characters without spaces, sent exactly as given); `cancel_payment`, `start_settlement` and the management creates take `Option<&str>` and use a new UUID for `None`. Every write returns a `PhysicalPosWriteResult` with `data`, `replayed`, `status_code` (202 or 201 new, 200 replay) and the `idempotency_key` sent. Payment writes run their own attempts (1 + `max_retries`) with the same key and body; no HTTP answer, a 2xx without a payment, or a 5xx other than the `PHYSICAL_POS_NOT_STARTED_CODES` is `Error::PhysicalPosOutcomeUnknown`, also when a later attempt answered 4xx. A new key for the same payment can charge the card twice.

Card data is never sent: the card is read on the device. A card-data field name (`pan`, `cardNumber`, `cvv`, `expiry` …) anywhere in a body or filter, or a card number in `merchantReference`, an item name or tax group, or the `merchantReference` / `q` filters, is `Error::PhysicalPosCardDataRejected` with the field path before anything is sent. Gateway answers map to `Error::PhysicalPosCardDataRejected`, `Error::PhysicalPosVoidAfterSettlement` (refund instead) and `Error::PhysicalPosRateLimit` (with `retry_after`). Lists return `PhysicalPosPage` (`items`, `has_more`, `next_cursor`, `total_count` with `includeTotal: true`); pass `next_cursor` as the `cursor` filter. Path identifiers are percent-encoded.

`PhysicalPosWebhookEvent::parse(&verifier, raw_body, &headers)` verifies the signature first, then reads the event: `category` (`payment`, `fiscal_document`, `settlement`, `other`), `history_seq`, `original` (a reversal's original payment), `cashier` (only with a device cashier session) and `is_device_sale`; amounts are checked after verification. End-of-day events carry no `historySeq`; deduplicate every event by `id`. Event names are in `physical_pos_events` and `PHYSICAL_POS_EVENT_TYPES`.

## Wallet, bank accounts, sanctions and insurance

These clients take request bodies and filters as JSON objects (`serde_json::Value`, `Value::Null` for no filters) and check them before anything is sent: an unknown field or filter, a missing required field, a value outside its set or a malformed amount is `Error::Invalid`, and nothing leaves the process. Answers are the envelope's `data`; lists return `CursorPage` (`items`, `has_more`, `next_cursor`, `total_count`, `raw`): pass `next_cursor` back unchanged as the `cursor` filter. Every call is signed, reads included. Money is canonical minor-unit text (`"12550"` = 125.50 TRY), never a JSON number.

**Wallet** (`client.wallet()`). Every route is described once in `WALLET_ROUTES` (method, path, `Idempotency-Key` rule, body and query fields; `wallet_route(name)` looks one up). Routes that require a key take `idempotency_key: &str` (1–128 printable ASCII characters, trimmed); the others take none. Money moves in two steps: `initiate_*` returns `{operationId, integrityHash, nonce, otpRequired, …}` and `complete_*` commits it; the complete calls put `operationId` in the body themselves. The card top-up (`topups().start_card`) requires `buyer` and `billingAddress` (`shippingAddress` optional) and returns `Error::MissingOrderDetails` naming the missing paths before sending.

```rust
use serde_json::json;

let wallet = client.wallet();
let started = wallet.transfers().initiate_w2w(&json!({
    "endUserId": end_user_id,
    "sourceWalletAccountId": account_id,
    "amount": "12550",
    "currency": "TRY",
    "destinationWalletNo": "1000000001",
}), &stored_key).await?;
let done = wallet.transfers().complete_w2w(started["operationId"].as_str().unwrap_or_default(), &json!({
    "endUserId": end_user_id,
    "integrityHash": started["integrityHash"],
    "nonce": started["nonce"],
}), &stored_complete_key).await?;
```

`qr().image(qr_id, format, scale)` returns a `WalletQrImage` (SVG or PNG bytes). Value sets: `wallet_account_type`, `wallet_end_user_status`, `wallet_kyc_level`, `wallet_otp_channel`, `wallet_qr_type`, `wallet_qr_image_format`, `wallet_bulk_target_type`, `wallet_corporate_account_status`.

**Bank accounts** (`client.bank_connect()`). Read filters follow `BANK_CONNECT_FILTERS`: `limit` 1–100, at most 100 account ids, list filters as one value or an array (sent comma-separated), minor-unit amount filters with `minAmountMinor` ≤ `maxAmountMinor`, `bookedTo` only with `bookedFrom`, at most 92 days apart. `transfers()` and `fx()` follow `BANK_TRANSFER_ROUTES` and need `bank-connect:transfer` and a key with an IP allowlist. `transfers().create(&body, key)` and `fx().create_order(&body, key)` check a key you pass (8–200 printable ASCII characters, sent as given) and use a new UUID for `None`; with `Config::max_retries` the request is retried after a network error or a 5xx with the same key and body. Store your own key (`BankConnectTransfers::new_idempotency_key()`) to retry safely across processes; the key used is also in `transfer.idempotencyKey`. `fx().create_quote` takes exactly one of `sourceAmountMinor` / `targetAmountMinor` and two different accounts.

**Sanctions** (`client.sanction()`). Bodies follow `SANCTION_ROUTES`, `SANCTION_NESTED_FIELDS` and `SANCTION_VALUE_SETS`. Screening returns matches, not verdicts: `is_clear_screening(&result)` is true only without any match or hit, sanction / match / case flag or blocking decision, and with `coverageStatus` `COMPLETE` when the answer reports coverage (`PARTIAL`, `DEGRADED` and `null` are never clear). A screening that could not run is `Error::Api` (HTTP 503, or a 2xx whose `status` is not `success`), never an empty result.

**Insurance** (`client.insurance()`). `create_order`, `confirm_payment`, `issue_policy`, `cancel_order`, `refund_premium` and `create_lead` require an `Idempotency-Key` (8–200 printable ASCII characters); `create_quote` and `bind_payment` take one optionally. A quote's basket is `paymentIntentId` or `context.items`, never both. The field rules of an insurance type's own object are left to the gateway (422 with every field error). `get_policy_document`, `get_policy_receipt` and `get_policy_certificate` return an `InsuranceDocument` (PDF bytes; errors stay JSON); `export_reconciliation` returns an `InsuranceReconciliationExport` with the CSV and its `truncated` / `next_cursor` / `aborted` trailer. `Insurance::consent_from_quote(&option, accepted_at)` builds the order's `consent`; `Insurance::premium_basket_item(&order, name, category1, category2)` builds the premium `BasketItem` of a `MERCHANT` order.

**Webhooks.** After `WebhookVerifier::verify`, read the data with `BankAccountTransactionReceived::from_event` (`bank_account.transaction.received`), `BankTransferWebhookData::from_event` (Bank Connect `bank_transfer.*`, not the payment `payment.bank_transfer.*` events), `InsuranceWebhookData::from_event` (`insurance_events::ORDER_EVENTS`) and `InsuranceLeadWebhookData::from_event` (`insurance.lead.submitted`). Event names: `bank_connect_events`, `bank_transfer_events`, `insurance_events`, `sanction_events`, with the `*_EVENT_TYPES` lists and subscription patterns.

## Subscriptions, KYC, identity and marketplace

These clients describe every route once in a table (`SUBSCRIPTION_ROUTES`, `KYC_ROUTES`, `IDENTITY_ROUTES`, `MARKETPLACE_ROUTES`: method, path, `Idempotency-Key` rule — `route.idempotency_key_rule()` is `"required"`, `"optional"` or `"none"` —, body and query fields) and check each call against it before anything is sent. Bodies and filters are JSON objects (`Value::Null` for none); an unknown field or filter, a missing required field, a value outside its set or a key where the route takes none is `Error::Invalid`, and the error never carries a value (card data and document images included). Answers are the envelope's `data`; lists return `CursorPage` (send `next_cursor` back as `cursor`, or as `pageToken` for subscription lists, which also report `page_size`). Every call is signed.

**Subscriptions** (`client.subscriptions()`). `create` needs an `Idempotency-Key` (the argument or the request's own `idempotencyKey`; both given, they must agree); it is sent in the header and the body, as for `change_plan`. Plan, customer, card and charge-retry writes accept one (1–200 printable ASCII characters without spaces). Renewals take the buyer from the customer record, so `customers().upsert` requires `externalId`, `name`, `surname`, `email`, `gsmNumber`, `identityNumber`, `billingAddress`, `billingCity`, `billingCountry` and `billingZipCode` (`billingContactName` and the `shipping*` fields are optional; `phone`, `locale` and `metadata` are not accepted); the gateway answers a missing or invalid field with `subscription_customer_error_code`. On subscription and marketplace routes every money member of the body, the filters and the answer must be canonical minor-unit text (`"9900"`; `"0"` for a free plan); a JSON number in an answer is `Error::Response`. `plans().create` checks `price`, the currency and the `billing` rule (`SUBSCRIPTION_BILLING_RULE_FIELDS`, `frequency` upper-cased, `interval` defaulting to 1). Cards are added from the checkout SDK's `paymentMethodToken`; a card-number field is refused. The portal-link routes live under `/api/panel/v1/subscriptions/portal` (the only panel paths the client reaches); `portal().create_sdk_session` returns the proof fields read at the envelope root, and `subscription_portal_url(&link, None)` gives the hosted URL. Helpers: `is_subscription_active`, `is_subscription_terminal`; values in `subscription_*` modules and `subscription_events`.

```rust
use serde_json::json;

let subscription = client.subscriptions().create(&json!({
    "customerId": customer_id,
    "planId": plan_id,
    "collectionMethod": "AUTO_CHARGE",
}), Some(&stored_key)).await?;
```

**KYC** (`client.kyc()`). `create_session` / `evidence_request` send `person` / `company` as `customer` with `customerType`, `metadata` as the `evidenceContextJson` text, check `sessionType` and `ttlMinutes` (1–1440), and put the optional key in the header and the body. `list_sessions` takes only `limit` and `cursor` (`pageSize` / `pageToken` are read as the same). The `kyc.*` webhook is the authority for the result; `kyc_assurance_meets(achieved, required)` and `is_kyc_session_terminal(status)` read it.

**Identity** (`client.identity()`). Device and account-linking signals, not verdicts: `is_identity_device_shared(&links, 2)` and `identity_link_reasons(&links)` read an `account_links` answer.

**Marketplace** (`client.marketplace()`). Split charges take the seller legs as a JSON array of `{subDealerId, amountMinor, scenario?}` (`scenario` a JSON object; `serde_json::to_value(&sellers)` of `MarketplaceSeller` values gives the same legs) and `multi_seller_policy` (`ALL_OR_NOTHING` by default); the legs must add up to `amount`, or nothing is sent. They require a persisted `Idempotency-Key` of 16–128 characters, like every payment mutation here. `charge_marketplace_card2d` / `3d` also require the fields of `MARKETPLACE_CHARGE_REQUIRED_FIELDS`, at least one basket item and complete order details; `price` / `paidPrice` and a non-boolean `preauth` are refused. Card data is sent as given and never put in an error. `create_checkout_intent` turns an answer without a token or form into `Error::Api`. `marketplace_onboarding_url(&link, None)` gives the dealer onboarding URL.

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
