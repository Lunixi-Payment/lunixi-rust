//! Payment links (`/api/v1/payments/links`).
//!
//! Every gateway call here is signed, reads included; the merchant and the
//! environment come from the signing key (a TEST key sees only TEST links).
//! The key needs the matching scope (`payment-link:create`, `:view`,
//! `:manage`, `:send`). The one request that is not sent to the gateway is
//! the image PUT to the presigned storage URL: it carries only the
//! `uploadHeaders` of the upload target, never the access token or the
//! request signature.
//!
//! Amounts are canonical minor-unit strings (`"150000"` is 1,500.00 TRY).
//! The gateway refuses a body field outside its schema with HTTP 400, so the
//! field lists below stop such a body locally.

use crate::{
    Client, Error, MAX_SAFE_INTEGER, Method, Request, Response, Result,
    bank_transfer::is_uuid,
    client::{require_key, upload_header_allowed, valid_id},
    payments::{currency, enabled_installments},
};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::fmt;

const BASE: &str = "/api/v1/payments/links";
const IMAGES: &str = "/api/v1/payments/links/images";
const QR_ACCEPT: &str = "image/svg+xml, image/png, application/json";

/// `usage`: how often a link can be paid. Fixed at creation.
pub mod payment_link_usage {
    /// Can be paid once (an invoice, a personal payment request).
    pub const SINGLE_USE: &str = "SINGLE_USE";
    /// Can be paid many times (donations, tickets, a product page).
    pub const MULTI_USE: &str = "MULTI_USE";
    /// Every value.
    pub const ALL: &[&str] = &[SINGLE_USE, MULTI_USE];
}
/// `amountMode`: who sets the amount. Fixed at creation.
pub mod payment_link_amount_mode {
    /// The merchant sets the amount (`amountMinor`).
    pub const FIXED: &str = "FIXED";
    /// The buyer enters the amount, bounded by `minAmountMinor` / `maxAmountMinor`.
    pub const OPEN: &str = "OPEN";
    /// The buyer picks quantities from `items`; the amount is the sum of the lines.
    pub const ITEMIZED: &str = "ITEMIZED";
    /// Every value.
    pub const ALL: &[&str] = &[FIXED, OPEN, ITEMIZED];
}
/// `state` of a link.
pub mod payment_link_state {
    /// `ACTIVE`
    pub const ACTIVE: &str = "ACTIVE";
    /// `PAUSED`
    pub const PAUSED: &str = "PAUSED";
    /// `COMPLETED`
    pub const COMPLETED: &str = "COMPLETED";
    /// `EXPIRED`
    pub const EXPIRED: &str = "EXPIRED";
    /// `DEACTIVATED`
    pub const DEACTIVATED: &str = "DEACTIVATED";
    /// Removed by Lunixi; every write answers 409 `payment_link.link.taken_down`.
    pub const TAKEN_DOWN: &str = "TAKEN_DOWN";
    /// Every value.
    pub const ALL: &[&str] = &[ACTIVE, PAUSED, COMPLETED, EXPIRED, DEACTIVATED, TAKEN_DOWN];
}
/// `state` of a payment attempt on a link.
pub mod payment_link_attempt_state {
    /// `OPEN`
    pub const OPEN: &str = "OPEN";
    /// `BOUND`
    pub const BOUND: &str = "BOUND";
    /// `SUCCEEDED`
    pub const SUCCEEDED: &str = "SUCCEEDED";
    /// `FAILED`
    pub const FAILED: &str = "FAILED";
    /// `EXPIRED`
    pub const EXPIRED: &str = "EXPIRED";
    /// `ABANDONED`
    pub const ABANDONED: &str = "ABANDONED";
    /// `NEEDS_REVIEW`
    pub const NEEDS_REVIEW: &str = "NEEDS_REVIEW";
    /// Every value.
    pub const ALL: &[&str] = &[
        OPEN,
        BOUND,
        SUCCEEDED,
        FAILED,
        EXPIRED,
        ABANDONED,
        NEEDS_REVIEW,
    ];
}
/// `environment` of a link; when sent it must equal the signing key's.
pub mod payment_link_environment {
    /// `TEST`
    pub const TEST: &str = "TEST";
    /// `LIVE`
    pub const LIVE: &str = "LIVE";
    /// Every value.
    pub const ALL: &[&str] = &[TEST, LIVE];
}
/// Requirement of a buyer field (`buyerFields.*`). Payment providers require
/// the buyer, the identity number and the billing address, so the link page
/// always asks all five fields: `REQUIRED` is the only value (the gateway
/// answers any other with 400; leaving a field out means `REQUIRED`).
pub mod buyer_field_requirement {
    /// `REQUIRED`
    pub const REQUIRED: &str = "REQUIRED";
    /// Every value.
    pub const ALL: &[&str] = &[REQUIRED];
}
/// `fieldType` of a custom field (no file uploads).
pub mod custom_field_type {
    /// `TEXT`
    pub const TEXT: &str = "TEXT";
    /// `NUMBER`
    pub const NUMBER: &str = "NUMBER";
    /// `EMAIL`
    pub const EMAIL: &str = "EMAIL";
    /// `PHONE`
    pub const PHONE: &str = "PHONE";
    /// `DATE`
    pub const DATE: &str = "DATE";
    /// `SELECT` (with `options`)
    pub const SELECT: &str = "SELECT";
    /// `CHECKBOX`
    pub const CHECKBOX: &str = "CHECKBOX";
    /// `TCKN`
    pub const TCKN: &str = "TCKN";
    /// `VKN`
    pub const VKN: &str = "VKN";
    /// Every value.
    pub const ALL: &[&str] = &[
        TEXT, NUMBER, EMAIL, PHONE, DATE, SELECT, CHECKBOX, TCKN, VKN,
    ];
}
/// `buttonLabelKey` of the payment page.
pub mod payment_link_button_label {
    /// `PAY`
    pub const PAY: &str = "PAY";
    /// `DONATE`
    pub const DONATE: &str = "DONATE";
    /// `BUY`
    pub const BUY: &str = "BUY";
    /// `BOOK`
    pub const BOOK: &str = "BOOK";
    /// Every value.
    pub const ALL: &[&str] = &[PAY, DONATE, BUY, BOOK];
}
/// Delivery channel of `send` and of the verification code.
pub mod payment_link_delivery_channel {
    /// `EMAIL`
    pub const EMAIL: &str = "EMAIL";
    /// `SMS`
    pub const SMS: &str = "SMS";
    /// Every value.
    pub const ALL: &[&str] = &[EMAIL, SMS];
}
/// Image format of `qr`.
pub mod payment_link_qr_format {
    /// `svg` (default)
    pub const SVG: &str = "svg";
    /// `png`
    pub const PNG: &str = "png";
    /// Every value.
    pub const ALL: &[&str] = &[SVG, PNG];
}
/// Language of the payment page and of `send`.
pub mod payment_link_locale {
    /// `tr`
    pub const TR: &str = "tr";
    /// `en`
    pub const EN: &str = "en";
    /// Every value.
    pub const ALL: &[&str] = &[TR, EN];
}
/// Declared type of a link image; SVG is never accepted.
pub mod payment_link_image_content_type {
    /// `image/png`
    pub const PNG: &str = "image/png";
    /// `image/jpeg`
    pub const JPEG: &str = "image/jpeg";
    /// `image/webp`
    pub const WEBP: &str = "image/webp";
    /// Every value.
    pub const ALL: &[&str] = &[PNG, JPEG, WEBP];
}

/// Every field of a payment-link definition, in the gateway's order. Create
/// bodies carry only these (plus `environment`); update bodies only these
/// (minus [`PAYMENT_LINK_IMMUTABLE_FIELDS`]) plus `expectedRowVersion`.
pub const PAYMENT_LINK_SPEC_FIELDS: &[&str] = &[
    "usage",
    "amountMode",
    "currency",
    "amountMinor",
    "minAmountMinor",
    "maxAmountMinor",
    "suggestedAmountsMinor",
    "quantityEnabled",
    "minQuantity",
    "maxQuantity",
    "items",
    "capacity",
    "title",
    "description",
    "imageAssetId",
    "successMessage",
    "successRedirectUrl",
    "taxNote",
    "locale",
    "startsAt",
    "expiresAt",
    "eventAt",
    "buttonLabelKey",
    "requireTermsAcceptance",
    "buyerFields",
    "customFields",
    "recipient",
    "prefill",
    "verification",
    "reminders",
    "checkout",
    "reference",
    "tags",
    "branchCode",
    "salesChannel",
    "campaign",
    "agentCode",
    "merchantNotifyEmails",
    "metadata",
];
/// Fixed at creation; an update naming one is refused.
pub const PAYMENT_LINK_IMMUTABLE_FIELDS: &[&str] = &["usage", "amountMode", "currency"];
/// Keys of `buyerFields`.
pub const PAYMENT_LINK_BUYER_FIELDS: &[&str] =
    &["name", "email", "phone", "address", "identityNumber"];
/// Timestamp fields (ISO 8601 text or `null`).
pub const PAYMENT_LINK_TIME_FIELDS: &[&str] = &["startsAt", "expiresAt", "eventAt"];
/// Query filters of `list`.
pub const PAYMENT_LINK_LIST_FILTERS: &[&str] = &[
    "limit",
    "cursor",
    "includeTotal",
    "state",
    "usage",
    "amountMode",
    "environment",
    "tag",
    "branchCode",
    "createdById",
    "query",
    "createdFrom",
    "createdTo",
];
/// Query filters of `list_attempts`.
pub const PAYMENT_LINK_ATTEMPT_LIST_FILTERS: &[&str] =
    &["limit", "cursor", "includeTotal", "state"];
/// Query filters of `list_payments`.
pub const PAYMENT_LINK_PAYMENT_LIST_FILTERS: &[&str] = &["limit", "cursor", "includeTotal"];
/// Operations of `action`.
pub const PAYMENT_LINK_ACTIONS: &[&str] = &["pause", "resume", "deactivate"];
/// Smallest `qr` size in pixels.
pub const PAYMENT_LINK_QR_MIN_SIZE: u32 = 128;
/// Largest `qr` size in pixels.
pub const PAYMENT_LINK_QR_MAX_SIZE: u32 = 2048;

const RECIPIENT_KEYS: &[&str] = &["name", "email", "phone"];
const PREFILL_KEYS: &[&str] = &[
    "name",
    "surname",
    "email",
    "phone",
    "identityNumber",
    "address",
    "city",
    "country",
    "zipCode",
];
const AMOUNT_FIELDS: &[&str] = &["amountMinor", "minAmountMinor", "maxAmountMinor"];

/// One line of an `ITEMIZED` link. `item_key` is the line's permanent
/// identity (stock counters hang off it); omitted, the server assigns one.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PaymentLinkItem {
    pub name: String,
    pub unit_price_minor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `0` makes the line optional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_quantity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_quantity: Option<u32>,
    /// Units available across all payments; `None` = unlimited.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<u32>,
    /// A published image asset id (from `upload_image`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_asset_id: Option<String>,
    /// A [`product_category`](crate::product_category) code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_category: Option<String>,
}
impl PaymentLinkItem {
    /// A line with its unit price in canonical minor units.
    pub fn new(name: impl Into<String>, unit_price_minor: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            unit_price_minor: unit_price_minor.into(),
            item_key: None,
            description: None,
            min_quantity: None,
            max_quantity: None,
            capacity: None,
            image_asset_id: None,
            product_category: None,
        }
    }
    /// The line's permanent key (cannot change once the link exists).
    pub fn with_item_key(mut self, value: impl Into<String>) -> Self {
        self.item_key = Some(value.into());
        self
    }
    /// Plain-text description.
    pub fn with_description(mut self, value: impl Into<String>) -> Self {
        self.description = Some(value.into());
        self
    }
    /// Buyer-selectable quantity range of this line.
    pub fn with_quantity_range(mut self, min: u32, max: u32) -> Self {
        self.min_quantity = Some(min);
        self.max_quantity = Some(max);
        self
    }
    /// Units available across all payments.
    pub fn with_capacity(mut self, value: u32) -> Self {
        self.capacity = Some(value);
        self
    }
    /// A published image asset id.
    pub fn with_image_asset_id(mut self, value: impl Into<String>) -> Self {
        self.image_asset_id = Some(value.into());
        self
    }
    /// A Lunixi product category code.
    pub fn with_product_category(mut self, value: impl Into<String>) -> Self {
        self.product_category = Some(value.into());
        self
    }
}

/// A typed question the buyer answers on the payment page.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PaymentLinkCustomField {
    pub key: String,
    pub label: String,
    /// One of [`custom_field_type`].
    pub field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Choices of a `SELECT` field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_text: Option<String>,
}
impl PaymentLinkCustomField {
    /// A custom field of a [`custom_field_type`].
    pub fn new(
        key: impl Into<String>,
        label: impl Into<String>,
        field_type: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            field_type: field_type.into(),
            required: None,
            options: None,
            max_length: None,
            help_text: None,
        }
    }
    /// Whether the buyer must answer.
    pub fn with_required(mut self, value: bool) -> Self {
        self.required = Some(value);
        self
    }
    /// Choices of a `SELECT` field.
    pub fn with_options(mut self, values: Vec<String>) -> Self {
        self.options = Some(values);
        self
    }
    /// Longest answer in characters.
    pub fn with_max_length(mut self, value: u32) -> Self {
        self.max_length = Some(value);
        self
    }
    /// Help text shown under the field.
    pub fn with_help_text(mut self, value: impl Into<String>) -> Self {
        self.help_text = Some(value.into());
        self
    }
}

/// Body of [`PaymentLinks::create`]. `usage`, `amountMode` and `currency`
/// are fixed at creation. Every rule (value sets, money strings, asset ids,
/// nested keys) is checked when the body is built, before any request; the
/// business rules (title length, amount ranges, which fields a mode allows)
/// are the server's, which answers 422 `payment_link.validation.invalid`.
/// Fields are sent in the order of [`PAYMENT_LINK_SPEC_FIELDS`].
#[derive(Clone, Debug)]
pub struct CreatePaymentLinkRequest {
    fields: Map<String, Value>,
}
impl CreatePaymentLinkRequest {
    fn base(usage: &str, amount_mode: &str, currency: &str, title: &str) -> Self {
        let mut fields = Map::new();
        fields.insert("usage".into(), json!(usage));
        fields.insert("amountMode".into(), json!(amount_mode));
        fields.insert("currency".into(), json!(currency.to_ascii_uppercase()));
        fields.insert("title".into(), json!(title));
        Self { fields }
    }
    fn set(mut self, key: &str, value: Value) -> Self {
        self.fields.insert(key.into(), value);
        self
    }
    /// A link for a set amount (`FIXED`).
    pub fn fixed(usage: &str, amount_minor: &str, currency: &str, title: &str) -> Self {
        Self::base(usage, payment_link_amount_mode::FIXED, currency, title)
            .set("amountMinor", json!(amount_minor))
    }
    /// A link where the buyer enters the amount (`OPEN`), at least `min_amount_minor`.
    pub fn open(usage: &str, currency: &str, title: &str, min_amount_minor: &str) -> Self {
        Self::base(usage, payment_link_amount_mode::OPEN, currency, title)
            .set("minAmountMinor", json!(min_amount_minor))
    }
    /// A link where the buyer picks quantities of the given lines (`ITEMIZED`).
    pub fn itemized(usage: &str, currency: &str, title: &str, items: Vec<PaymentLinkItem>) -> Self {
        Self::base(usage, payment_link_amount_mode::ITEMIZED, currency, title)
            .set("items", to_value(&items))
    }
    /// Optional; must equal the signing key's environment ([`payment_link_environment`]).
    pub fn with_environment(self, value: &str) -> Self {
        self.set("environment", json!(value))
    }
    /// Upper bound of an `OPEN` amount.
    pub fn with_max_amount_minor(self, value: &str) -> Self {
        self.set("maxAmountMinor", json!(value))
    }
    /// Up to six ascending suggested amounts of an `OPEN` link.
    pub fn with_suggested_amounts_minor(self, values: Vec<String>) -> Self {
        self.set("suggestedAmountsMinor", json!(values))
    }
    /// Lets the buyer choose a quantity (`FIXED` + `MULTI_USE`), 1..999.
    pub fn with_quantity(self, min_quantity: u32, max_quantity: u32) -> Self {
        self.set("quantityEnabled", json!(true))
            .set("minQuantity", json!(min_quantity))
            .set("maxQuantity", json!(max_quantity))
    }
    /// Adds one `ITEMIZED` line.
    pub fn add_item(mut self, item: PaymentLinkItem) -> Self {
        let item = to_value(&item);
        match self.fields.get_mut("items").and_then(Value::as_array_mut) {
            Some(items) => items.push(item),
            None => {
                self.fields.insert("items".into(), json!([item]));
            }
        }
        self
    }
    /// Total units the link can sell (`MULTI_USE`).
    pub fn with_capacity(self, value: u32) -> Self {
        self.set("capacity", json!(value))
    }
    /// Plain text (no HTML or Markdown).
    pub fn with_description(self, value: impl Into<String>) -> Self {
        self.set("description", json!(value.into()))
    }
    /// A published image asset id (from `upload_image`).
    pub fn with_image_asset_id(self, value: &str) -> Self {
        self.set("imageAssetId", json!(value))
    }
    /// Text shown after a successful payment.
    pub fn with_success_message(self, value: impl Into<String>) -> Self {
        self.set("successMessage", json!(value.into()))
    }
    /// `https://` page the buyer goes to after paying.
    pub fn with_success_redirect_url(self, value: impl Into<String>) -> Self {
        self.set("successRedirectUrl", json!(value.into()))
    }
    /// Tax note shown on the page.
    pub fn with_tax_note(self, value: impl Into<String>) -> Self {
        self.set("taxNote", json!(value.into()))
    }
    /// Language of the payment page ([`payment_link_locale`]).
    pub fn with_locale(self, value: &str) -> Self {
        self.set("locale", json!(value))
    }
    /// ISO 8601 start time.
    pub fn with_starts_at(self, value: impl Into<String>) -> Self {
        self.set("startsAt", json!(value.into()))
    }
    /// ISO 8601 expiry time.
    pub fn with_expires_at(self, value: impl Into<String>) -> Self {
        self.set("expiresAt", json!(value.into()))
    }
    /// ISO 8601 event time (display only, e.g. the date of a ticket).
    pub fn with_event_at(self, value: impl Into<String>) -> Self {
        self.set("eventAt", json!(value.into()))
    }
    /// Button text ([`payment_link_button_label`]).
    pub fn with_button_label_key(self, value: &str) -> Self {
        self.set("buttonLabelKey", json!(value))
    }
    /// Whether the buyer must accept your terms.
    pub fn with_terms_acceptance_required(self, value: bool) -> Self {
        self.set("requireTermsAcceptance", json!(value))
    }
    /// `{ name, email, phone, address, identityNumber }` → [`buyer_field_requirement`].
    pub fn with_buyer_fields(self, value: Value) -> Self {
        self.set("buyerFields", value)
    }
    /// Questions the buyer answers.
    pub fn with_custom_fields(self, fields: Vec<PaymentLinkCustomField>) -> Self {
        self.set("customFields", to_value(&fields))
    }
    /// Who the link is for (`name`, `email`, `phone` in E.164); stored encrypted.
    pub fn with_recipient(self, value: Value) -> Self {
        self.set("recipient", value)
    }
    /// Buyer details filled in on the server side; never sent to the browser.
    pub fn with_prefill(self, value: Value) -> Self {
        self.set("prefill", value)
    }
    /// Requires a one-time code sent to `recipient.email` before paying.
    pub fn with_verification(self, required: bool) -> Self {
        self.set(
            "verification",
            json!({"required": required, "channel": payment_link_delivery_channel::EMAIL}),
        )
    }
    /// Reminder e-mails every `interval_hours` (24–168), at most `max_count` (1–3) times.
    pub fn with_reminders(self, interval_hours: u32, max_count: u32) -> Self {
        self.set(
            "reminders",
            json!({"enabled": true, "intervalHours": interval_hours, "maxCount": max_count}),
        )
    }
    /// Narrows the checkout: `require_three_ds` only tightens, `payment_methods`
    /// is a subset of your form settings (empty = your settings),
    /// `enabled_installments` the counts the form may offer (`None` = no
    /// restriction; otherwise 1–36, distinct, including 1).
    pub fn with_checkout(
        self,
        require_three_ds: bool,
        payment_methods: Vec<String>,
        enabled_installments: Option<Vec<u8>>,
    ) -> Self {
        let mut checkout =
            json!({"requireThreeDs": require_three_ds, "paymentMethods": payment_methods});
        if let Some(mut counts) = enabled_installments {
            counts.sort_unstable();
            checkout["enabledInstallments"] = json!(counts);
        }
        self.set("checkout", checkout)
    }
    /// Your reference.
    pub fn with_reference(self, value: impl Into<String>) -> Self {
        self.set("reference", json!(value.into()))
    }
    /// Tags for filtering.
    pub fn with_tags(self, values: Vec<String>) -> Self {
        self.set("tags", json!(values))
    }
    /// Branch code.
    pub fn with_branch_code(self, value: impl Into<String>) -> Self {
        self.set("branchCode", json!(value.into()))
    }
    /// Sales channel.
    pub fn with_sales_channel(self, value: impl Into<String>) -> Self {
        self.set("salesChannel", json!(value.into()))
    }
    /// Campaign.
    pub fn with_campaign(self, value: impl Into<String>) -> Self {
        self.set("campaign", json!(value.into()))
    }
    /// Agent code.
    pub fn with_agent_code(self, value: impl Into<String>) -> Self {
        self.set("agentCode", json!(value.into()))
    }
    /// Addresses notified of payments.
    pub fn with_merchant_notify_emails(self, values: Vec<String>) -> Self {
        self.set("merchantNotifyEmails", json!(values))
    }
    /// Up to 20 string key/value pairs returned to you in webhooks.
    pub fn with_metadata(self, value: Value) -> Self {
        self.set("metadata", value)
    }
}
fn to_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// A create body: [`CreatePaymentLinkRequest`] or a raw `serde_json::Value`
/// object. Both are checked before sending.
pub trait PaymentLinkBody {
    /// The checked JSON body.
    fn to_body(&self) -> Result<Value>;
}
impl PaymentLinkBody for Value {
    fn to_body(&self) -> Result<Value> {
        let fields = self
            .as_object()
            .ok_or(Error::Invalid("payment link body object"))?;
        validate_link_fields(fields, true)?;
        Ok(self.clone())
    }
}
impl PaymentLinkBody for CreatePaymentLinkRequest {
    fn to_body(&self) -> Result<Value> {
        validate_link_fields(&self.fields, true)?;
        let mut body = Map::new();
        for key in std::iter::once("environment").chain(PAYMENT_LINK_SPEC_FIELDS.iter().copied()) {
            if let Some(value) = self.fields.get(key) {
                body.insert(key.into(), value.clone());
            }
        }
        Ok(Value::Object(body))
    }
}

fn one_of(value: &Value, allowed: &[&str], field: &'static str) -> Result<()> {
    if value.as_str().is_some_and(|s| allowed.contains(&s)) {
        Ok(())
    } else {
        Err(Error::Invalid(field))
    }
}
fn text(value: &Value, field: &'static str) -> Result<()> {
    if value.as_str().is_some_and(|s| !s.trim().is_empty()) {
        Ok(())
    } else {
        Err(Error::Invalid(field))
    }
}
fn minor(value: &Value) -> Result<()> {
    crate::money::parse_minor(
        value
            .as_str()
            .ok_or(Error::Invalid("payment link amount (minor-unit string)"))?,
        false,
        false,
    )
    .map(|_| ())
}
fn asset_id(value: &Value) -> Result<()> {
    if value.is_null() || value.as_str().is_some_and(is_uuid) {
        Ok(())
    } else {
        Err(Error::Invalid("payment link image asset ID (UUID)"))
    }
}
fn string_object(value: &Value, keys: Option<&[&str]>, field: &'static str) -> Result<()> {
    if value.is_null() {
        return Ok(());
    }
    let object = value.as_object().ok_or(Error::Invalid(field))?;
    let valid = object
        .iter()
        .all(|(k, v)| v.is_string() && keys.is_none_or(|keys| keys.contains(&k.as_str())));
    if valid {
        Ok(())
    } else {
        Err(Error::Invalid(field))
    }
}
fn string_list(value: &Value, field: &'static str) -> Result<()> {
    if value
        .as_array()
        .is_some_and(|list| list.iter().all(Value::is_string))
    {
        Ok(())
    } else {
        Err(Error::Invalid(field))
    }
}
/// Field rules shared by create (`create`) and update bodies.
fn validate_link_fields(fields: &Map<String, Value>, create: bool) -> Result<()> {
    for key in fields.keys() {
        let known =
            PAYMENT_LINK_SPEC_FIELDS.contains(&key.as_str()) || (create && key == "environment");
        if !known {
            return Err(Error::Invalid("unknown payment link field"));
        }
        if !create && PAYMENT_LINK_IMMUTABLE_FIELDS.contains(&key.as_str()) {
            return Err(Error::Invalid(
                "usage, amountMode and currency are fixed at creation",
            ));
        }
    }
    if create {
        for key in ["usage", "amountMode", "currency", "title"] {
            text(
                fields.get(key).unwrap_or(&Value::Null),
                "payment link usage, amountMode, currency and title",
            )?;
        }
    }
    for (key, value) in fields {
        let key = key.as_str();
        let nullable = !["usage", "amountMode", "currency", "title", "environment"].contains(&key);
        if value.is_null() && nullable {
            continue;
        }
        match key {
            "usage" => one_of(value, payment_link_usage::ALL, "payment link usage")?,
            "amountMode" => one_of(
                value,
                payment_link_amount_mode::ALL,
                "payment link amountMode",
            )?,
            "environment" => one_of(
                value,
                payment_link_environment::ALL,
                "payment link environment",
            )?,
            "currency" => currency(value.as_str().unwrap_or_default())?,
            "title" => text(value, "payment link title")?,
            "locale" => one_of(value, payment_link_locale::ALL, "payment link locale")?,
            "buttonLabelKey" => one_of(
                value,
                payment_link_button_label::ALL,
                "payment link buttonLabelKey",
            )?,
            "imageAssetId" => asset_id(value)?,
            "startsAt" | "expiresAt" | "eventAt" => text(value, "payment link time (ISO 8601)")?,
            "suggestedAmountsMinor" => {
                let list = value
                    .as_array()
                    .ok_or(Error::Invalid("suggestedAmountsMinor"))?;
                list.iter().try_for_each(minor)?;
            }
            "items" => {
                let list = value
                    .as_array()
                    .filter(|l| !l.is_empty())
                    .ok_or(Error::Invalid("payment link items"))?;
                for item in list {
                    let item = item
                        .as_object()
                        .ok_or(Error::Invalid("payment link item"))?;
                    text(
                        item.get("name").unwrap_or(&Value::Null),
                        "payment link item name",
                    )?;
                    minor(item.get("unitPriceMinor").unwrap_or(&Value::Null))?;
                    if let Some(id) = item.get("imageAssetId") {
                        asset_id(id)?;
                    }
                }
            }
            "customFields" => {
                let list = value
                    .as_array()
                    .ok_or(Error::Invalid("payment link customFields"))?;
                for field in list {
                    one_of(
                        field.get("fieldType").unwrap_or(&Value::Null),
                        custom_field_type::ALL,
                        "payment link custom field type",
                    )?;
                }
            }
            "buyerFields" => {
                let object = value
                    .as_object()
                    .ok_or(Error::Invalid("payment link buyerFields"))?;
                for (name, requirement) in object {
                    if !PAYMENT_LINK_BUYER_FIELDS.contains(&name.as_str()) {
                        return Err(Error::Invalid("unknown payment link buyerFields key"));
                    }
                    one_of(
                        requirement,
                        buyer_field_requirement::ALL,
                        "payment link buyer field requirement",
                    )?;
                }
            }
            "recipient" => string_object(value, Some(RECIPIENT_KEYS), "payment link recipient")?,
            "prefill" => string_object(value, Some(PREFILL_KEYS), "payment link prefill")?,
            "metadata" => string_object(value, None, "payment link metadata (string values)")?,
            "verification" => one_of(
                value.get("channel").unwrap_or(&Value::Null),
                payment_link_delivery_channel::ALL,
                "payment link verification channel",
            )?,
            "checkout" => {
                if let Some(counts) = value.get("enabledInstallments") {
                    let counts: Vec<u8> = serde_json::from_value(counts.clone())
                        .map_err(|_| Error::Invalid("checkout enabledInstallments"))?;
                    enabled_installments(&counts)?;
                }
                if let Some(methods) = value.get("paymentMethods") {
                    string_list(methods, "checkout paymentMethods")?;
                }
            }
            "tags" | "merchantNotifyEmails" => string_list(value, "payment link string list")?,
            _ if AMOUNT_FIELDS.contains(&key) => minor(value)?,
            _ => {}
        }
    }
    Ok(())
}

/// Options of `pause`, `resume`, `deactivate` and `action`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PaymentLinkActionOptions {
    /// Fail with 409 `payment_link.link.version_conflict` if the link changed.
    pub expected_row_version: Option<u64>,
    /// Recorded with the change.
    pub reason: Option<String>,
}
impl PaymentLinkActionOptions {
    /// The link's current `rowVersion`.
    pub fn with_expected_row_version(mut self, value: u64) -> Self {
        self.expected_row_version = Some(value);
        self
    }
    /// A reason recorded with the change.
    pub fn with_reason(mut self, value: impl Into<String>) -> Self {
        self.reason = Some(value.into());
        self
    }
}

/// Body of [`PaymentLinks::send`]. Without a recipient the link's own
/// `recipient` is used.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PaymentLinkSendRequest {
    /// [`payment_link_delivery_channel`]
    pub channel: String,
    pub recipient_email: Option<String>,
    pub recipient_phone: Option<String>,
    /// [`payment_link_locale`]
    pub locale: Option<String>,
}
impl PaymentLinkSendRequest {
    /// Sends by `EMAIL` or `SMS`.
    pub fn new(channel: impl Into<String>) -> Self {
        Self {
            channel: channel.into(),
            recipient_email: None,
            recipient_phone: None,
            locale: None,
        }
    }
    /// Another e-mail address than the link's recipient.
    pub fn with_recipient_email(mut self, value: impl Into<String>) -> Self {
        self.recipient_email = Some(value.into());
        self
    }
    /// Another phone number (E.164) than the link's recipient.
    pub fn with_recipient_phone(mut self, value: impl Into<String>) -> Self {
        self.recipient_phone = Some(value.into());
        self
    }
    /// Language of the message.
    pub fn with_locale(mut self, value: impl Into<String>) -> Self {
        self.locale = Some(value.into());
        self
    }
    fn body(&self) -> Result<Value> {
        one_of(
            &json!(self.channel),
            payment_link_delivery_channel::ALL,
            "payment link send channel (EMAIL or SMS)",
        )?;
        let mut body = Map::new();
        body.insert("channel".into(), json!(self.channel));
        if let Some(email) = &self.recipient_email {
            body.insert("recipientEmail".into(), json!(email));
        }
        if let Some(phone) = &self.recipient_phone {
            body.insert("recipientPhone".into(), json!(phone));
        }
        if let Some(locale) = &self.locale {
            one_of(
                &json!(locale),
                payment_link_locale::ALL,
                "payment link send locale",
            )?;
            body.insert("locale".into(), json!(locale));
        }
        Ok(Value::Object(body))
    }
}

/// Upload target of a link image (`create_image_upload`): PUT the file's
/// bytes to `upload_url` with exactly `upload_headers` before `expires_at`,
/// then `confirm_image_upload(asset_id)`. `upload_url` is a presigned storage
/// URL and works as a credential until it expires: debug output hides it.
#[derive(Clone)]
#[non_exhaustive]
pub struct PaymentLinkImageUpload {
    pub asset_id: String,
    pub upload_url: String,
    pub upload_headers: Vec<(String, String)>,
    pub expires_at: Option<String>,
}
impl fmt::Debug for PaymentLinkImageUpload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PaymentLinkImageUpload")
            .field("asset_id", &self.asset_id)
            .field("upload_url", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}
impl PaymentLinkImageUpload {
    /// Reads and checks the target: an asset id, an HTTPS URL, the PUT
    /// method and string headers without credentials. Anything else fails
    /// before a byte is sent.
    fn from_data(data: Option<&Value>, allow_insecure_localhost: bool) -> Result<Self> {
        let unusable = Error::Response("image upload target");
        let data = data.ok_or(unusable.clone())?;
        let asset_id = data
            .get("assetId")
            .and_then(Value::as_str)
            .filter(|s| is_uuid(s))
            .ok_or(unusable.clone())?;
        let upload_url = data
            .get("uploadUrl")
            .and_then(Value::as_str)
            .filter(|s| crate::client::upload_url(s, allow_insecure_localhost).is_ok())
            .ok_or(unusable.clone())?;
        if data.get("uploadMethod").and_then(Value::as_str) != Some("PUT") {
            return Err(unusable);
        }
        let mut upload_headers = vec![];
        for (name, value) in data
            .get("uploadHeaders")
            .and_then(Value::as_object)
            .ok_or(unusable.clone())?
        {
            let value = value.as_str().ok_or(unusable.clone())?;
            if !upload_header_allowed(name) {
                return Err(unusable);
            }
            upload_headers.push((name.clone(), value.to_owned()));
        }
        Ok(Self {
            asset_id: asset_id.to_owned(),
            upload_url: upload_url.to_owned(),
            upload_headers,
            expires_at: data
                .get("expiresAt")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }
}

fn link_path(link_id: &str) -> Result<String> {
    Ok(format!("{BASE}/{}", valid_id(link_id)?))
}
fn row_version(value: u64) -> Result<Value> {
    if value > MAX_SAFE_INTEGER as u64 {
        return Err(Error::Invalid("expectedRowVersion (the link's rowVersion)"));
    }
    Ok(json!(value))
}
fn list_query(filters: Vec<(String, String)>, allowed: &[&str]) -> Result<Vec<(String, String)>> {
    let mut query = vec![];
    for (name, value) in filters {
        if !allowed.contains(&name.as_str()) {
            return Err(Error::Invalid("unknown payment link list filter"));
        }
        if !value.is_empty() {
            query.push((name, value));
        }
    }
    Ok(query)
}
fn image_file(file_name: &str, content_type: &str, size_bytes: u64) -> Result<Value> {
    let file_name = file_name.trim();
    if file_name.is_empty() || file_name.chars().count() > 255 {
        return Err(Error::Invalid("image file name (1-255 characters)"));
    }
    one_of(
        &json!(content_type),
        payment_link_image_content_type::ALL,
        "image content type (PNG, JPEG or WebP; SVG is not accepted)",
    )?;
    if size_bytes == 0 || size_bytes > MAX_SAFE_INTEGER as u64 {
        return Err(Error::Invalid("image size in bytes"));
    }
    Ok(json!({"fileName": file_name, "contentType": content_type, "sizeBytes": size_bytes}))
}

/// Payment-link facade sharing the client's transport and token cache.
pub struct PaymentLinks<'a> {
    client: &'a Client,
}
impl Client {
    /// Returns the payment-link facade; no independent HTTP client is constructed.
    pub fn payment_links(&self) -> PaymentLinks<'_> {
        PaymentLinks { client: self }
    }
}
impl PaymentLinks<'_> {
    /// Creates a link (`POST /payments/links`). The persisted `key` is
    /// required: the same key and body return the stored link with
    /// `replayed: true`; the same key with another body is 409
    /// `payment_link.idempotency.conflict`.
    pub async fn create<B: PaymentLinkBody + ?Sized>(
        &self,
        body: &B,
        key: &str,
    ) -> Result<Response> {
        let body = body.to_body()?;
        require_key(key)?;
        self.client
            .send(
                Request::new(Method::POST, BASE)
                    .json(body)
                    .idempotency_key(key),
            )
            .await
    }
    /// One page of links, newest first, filters from
    /// [`PAYMENT_LINK_LIST_FILTERS`] (a list value as a comma list).
    /// `page_info` carries `hasMore` / `nextCursor`.
    pub async fn list(&self, filters: Vec<(String, String)>) -> Result<Response> {
        let mut request = Request::new(Method::GET, BASE);
        request.query = list_query(filters, PAYMENT_LINK_LIST_FILTERS)?;
        self.client.send(request).await
    }
    /// One link.
    pub async fn get(&self, link_id: &str) -> Result<Response> {
        self.client
            .send(Request::new(Method::GET, link_path(link_id)?))
            .await
    }
    /// Changes a link and creates a new version (`PATCH /payments/links/{id}`).
    /// Only the keys in `changes` are updated; `null` clears a field and a
    /// nested object is replaced whole. `expected_row_version` is the link's
    /// current `rowVersion` (409 `payment_link.link.version_conflict` if it
    /// changed). `usage`, `amountMode` and `currency` cannot be changed.
    pub async fn update(
        &self,
        link_id: &str,
        expected_row_version: u64,
        changes: &Value,
        key: Option<&str>,
    ) -> Result<Response> {
        let path = link_path(link_id)?;
        let changes = changes
            .as_object()
            .filter(|c| !c.is_empty())
            .ok_or(Error::Invalid(
                "payment link update needs at least one field",
            ))?;
        validate_link_fields(changes, false)?;
        let mut body = Map::new();
        body.insert(
            "expectedRowVersion".into(),
            row_version(expected_row_version)?,
        );
        body.extend(changes.clone());
        let mut request = Request::new(Method::PATCH, path).json(Value::Object(body));
        if let Some(key) = key {
            request = request.idempotency_key(key);
        }
        self.client.send(request).await
    }
    /// Stops accepting payments until `resume`.
    pub async fn pause(
        &self,
        link_id: &str,
        options: &PaymentLinkActionOptions,
        key: Option<&str>,
    ) -> Result<Response> {
        self.action(link_id, "pause", options, key).await
    }
    /// Accepts payments again after `pause`.
    pub async fn resume(
        &self,
        link_id: &str,
        options: &PaymentLinkActionOptions,
        key: Option<&str>,
    ) -> Result<Response> {
        self.action(link_id, "resume", options, key).await
    }
    /// Closes the link permanently; it cannot be reopened.
    pub async fn deactivate(
        &self,
        link_id: &str,
        options: &PaymentLinkActionOptions,
        key: Option<&str>,
    ) -> Result<Response> {
        self.action(link_id, "deactivate", options, key).await
    }
    /// `pause`, `resume` or `deactivate` (`POST /payments/links/{id}/{action}`).
    pub async fn action(
        &self,
        link_id: &str,
        action: &str,
        options: &PaymentLinkActionOptions,
        key: Option<&str>,
    ) -> Result<Response> {
        if !PAYMENT_LINK_ACTIONS.contains(&action) {
            return Err(Error::Invalid(
                "payment link action (pause, resume, deactivate)",
            ));
        }
        let path = format!("{}/{action}", link_path(link_id)?);
        let mut body = Map::new();
        if let Some(version) = options.expected_row_version {
            body.insert("expectedRowVersion".into(), row_version(version)?);
        }
        if let Some(reason) = &options.reason {
            body.insert("reason".into(), json!(reason));
        }
        let mut request = Request::new(Method::POST, path).json(Value::Object(body));
        if let Some(key) = key {
            request = request.idempotency_key(key);
        }
        self.client.send(request).await
    }
    /// Sends the link by e-mail or SMS (`POST /payments/links/{id}/send`).
    /// The persisted `key` is required so a retry cannot send twice. An
    /// accepted delivery can still fail: check `delivery.state`.
    pub async fn send(
        &self,
        link_id: &str,
        request: &PaymentLinkSendRequest,
        key: &str,
    ) -> Result<Response> {
        let path = format!("{}/send", link_path(link_id)?);
        let body = request.body()?;
        require_key(key)?;
        self.client
            .send(
                Request::new(Method::POST, path)
                    .json(body)
                    .idempotency_key(key),
            )
            .await
    }
    /// The link's QR code (`GET /payments/links/{id}/qr`) in
    /// `Response::bytes` with its content type. `format` is
    /// [`payment_link_qr_format`] (default `svg`), `size` 128–2048 pixels.
    pub async fn qr(
        &self,
        link_id: &str,
        format: Option<&str>,
        size: Option<u32>,
    ) -> Result<Response> {
        let format = format.unwrap_or(payment_link_qr_format::SVG);
        one_of(
            &json!(format),
            payment_link_qr_format::ALL,
            "QR format (svg or png)",
        )?;
        let mut request = Request::new(Method::GET, format!("{}/qr", link_path(link_id)?));
        request.query.push(("format".into(), format.into()));
        if let Some(size) = size {
            if !(PAYMENT_LINK_QR_MIN_SIZE..=PAYMENT_LINK_QR_MAX_SIZE).contains(&size) {
                return Err(Error::Invalid("QR size (128-2048 pixels)"));
            }
            request.query.push(("size".into(), size.to_string()));
        }
        self.client.send_binary(request, QR_ACCEPT).await
    }
    /// Payment attempts on the link, newest first
    /// ([`PAYMENT_LINK_ATTEMPT_LIST_FILTERS`]).
    pub async fn list_attempts(
        &self,
        link_id: &str,
        filters: Vec<(String, String)>,
    ) -> Result<Response> {
        let mut request = Request::new(Method::GET, format!("{}/attempts", link_path(link_id)?));
        request.query = list_query(filters, PAYMENT_LINK_ATTEMPT_LIST_FILTERS)?;
        self.client.send(request).await
    }
    /// Successful payments on the link with refund totals
    /// ([`PAYMENT_LINK_PAYMENT_LIST_FILTERS`]).
    pub async fn list_payments(
        &self,
        link_id: &str,
        filters: Vec<(String, String)>,
    ) -> Result<Response> {
        let mut request = Request::new(Method::GET, format!("{}/payments", link_path(link_id)?));
        request.query = list_query(filters, PAYMENT_LINK_PAYMENT_LIST_FILTERS)?;
        self.client.send(request).await
    }
    /// Link image, step 1 of 3 (`POST /payments/links/images`): an upload
    /// target for a PNG, JPEG or WebP file. `file_name` is recorded only
    /// (1–255 characters); a declared size over the image limit (2 MiB) is
    /// refused by the gateway before anything is uploaded.
    pub async fn create_image_upload(
        &self,
        file_name: &str,
        content_type: &str,
        size_bytes: u64,
    ) -> Result<PaymentLinkImageUpload> {
        let body = image_file(file_name, content_type, size_bytes)?;
        let response = self
            .client
            .send(Request::new(Method::POST, IMAGES).json(body))
            .await?;
        PaymentLinkImageUpload::from_data(
            response.data.as_ref(),
            self.client.allow_insecure_localhost(),
        )
    }
    /// Link image, step 2 of 3: PUT of `content` straight to the storage
    /// host with only `upload_headers`. It does not go through the gateway
    /// transport: the access token and the request signature are never sent
    /// there. Not retried; a failure is `Error::Api` with the storage status
    /// (e.g. code `AccessDenied` once the URL has expired) — start again from
    /// `create_image_upload`.
    pub async fn put_image_upload(
        &self,
        target: &PaymentLinkImageUpload,
        content: &[u8],
    ) -> Result<()> {
        if content.is_empty() {
            return Err(Error::Invalid("image content"));
        }
        self.client
            .put_presigned(&target.upload_url, &target.upload_headers, content)
            .await
    }
    /// Link image, step 3 of 3 (`POST /payments/links/images/{assetId}/confirm`):
    /// media checks the stored bytes (declared type, at most 2 MiB and
    /// 4096×4096 px) and publishes the image. `data` is
    /// `{ assetId, url, state: "PUBLISHED" }`; use `assetId` as a link's or
    /// an item's `imageAssetId`. Confirming before the PUT landed is 400
    /// `media.confirm.object_missing` and can be retried.
    pub async fn confirm_image_upload(&self, asset_id: &str) -> Result<Response> {
        let asset_id = asset_id.trim();
        if !is_uuid(asset_id) {
            return Err(Error::Invalid("image asset ID (UUID)"));
        }
        let response = self
            .client
            .send(
                Request::new(Method::POST, format!("{IMAGES}/{asset_id}/confirm")).json(json!({})),
            )
            .await?;
        let data = response.data.as_ref();
        let published = data
            .and_then(|d| d.get("assetId"))
            .and_then(Value::as_str)
            .is_some()
            && data.and_then(|d| d.get("state")).and_then(Value::as_str) == Some("PUBLISHED");
        if !published {
            return Err(Error::Response("image confirmation"));
        }
        Ok(response)
    }
    /// Uploads a link image in one call: `create_image_upload`, the PUT of
    /// `content` and `confirm_image_upload`. Returns the confirmation.
    pub async fn upload_image(
        &self,
        file_name: &str,
        content_type: &str,
        content: &[u8],
    ) -> Result<Response> {
        if content.is_empty() {
            return Err(Error::Invalid("image content"));
        }
        let target = self
            .create_image_upload(file_name, content_type, content.len() as u64)
            .await?;
        self.put_image_upload(&target, content).await?;
        self.confirm_image_upload(&target.asset_id).await
    }
}
