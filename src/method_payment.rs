//! Payment-method endpoints and their request rules:
//!
//! - `POST /api/v1/payments/direct/payment` (start a payment with any method)
//! - `POST /api/v1/payments/{paymentId}/actions/{actionId}/confirm`
//! - `POST /api/v1/payments/{paymentId}/actions/{actionId}/resend`
//! - `GET  /api/v1/payments/methods[?paymentMethod=]`
//!
//! The gateway refuses a body field outside its schema with HTTP 400; the
//! field lists below are the gateway's, so the SDK stops such a body locally.

use crate::{
    Address, BasketItem, Buyer, Error, Method, Request, Response, Result,
    client::{require_key, valid_id},
    payment_method,
    payments::Payments,
    promissory_note::{PromissoryNoteScheduleRow, normalize_note_count, validate_plan},
};
use serde_json::{Map, Value, json};

/// What the buyer must do when a payment response carries `action` (`action.type`).
pub mod payment_action_type {
    /// Send the one-time code with `confirm_payment_action`.
    pub const OTP: &str = "OTP";
    /// Send the buyer to `action.redirect.url` (for cards: the bank's 3D Secure page).
    pub const REDIRECT: &str = "REDIRECT";
    /// Show the buyer something to act on outside the page (transfer details, a code).
    pub const DISPLAY: &str = "DISPLAY";
    /// The buyer approves in the provider's app; the result arrives by webhook.
    pub const APP_APPROVAL: &str = "APP_APPROVAL";
    /// Send the fields listed in `action.input.fields` with `confirm_payment_action`.
    pub const INPUT: &str = "INPUT";
}

/// `providerSelection` of a method in `list_payment_methods`.
pub mod provider_selection {
    /// Card: routing picks the provider; `providerCode` is not sent.
    pub const ROUTED: &str = "ROUTED";
    /// `providerCode` may be omitted.
    pub const OPTIONAL: &str = "OPTIONAL";
    /// More than one provider is connected and the buyer picks; `providerCode` must be sent.
    pub const REQUIRED: &str = "REQUIRED";
}

/// `providerSelectionMode` of a method in `list_payment_methods`: who picks the provider.
pub mod provider_selection_mode {
    /// The buyer picks the brand; with more than one provider `providerCode` is required.
    pub const BUYER: &str = "BUYER";
    /// The platform picks one of the connected providers when `providerCode` is omitted.
    pub const PLATFORM: &str = "PLATFORM";
}

/// Method blocks `direct/payment` accepts besides `card`. The block of the
/// chosen method is required; a block of another method is refused.
pub const METHOD_BLOCK_FIELDS: &[&str] = &[
    "mobileBilling",
    "wallet",
    "promissoryNote",
    "mealCard",
    "openBanking",
    "shoppingLoan",
    "bankTransfer",
];
/// Body fields of `POST /payments/direct/payment`.
pub const DIRECT_METHOD_PAYMENT_FIELDS: &[&str] = &[
    "paymentMethod",
    "amount",
    "currency",
    "orderId",
    "paymentChannel",
    "paymentGroup",
    "callbackUrl",
    "locale",
    "description",
    "customerId",
    "preauth",
    "buyer",
    "shippingAddress",
    "billingAddress",
    "basketItems",
    "card",
    "installment",
    "installmentQuoteToken",
    "force3D",
    "metadata",
    "mobileBilling",
    "wallet",
    "promissoryNote",
    "mealCard",
    "openBanking",
    "shoppingLoan",
    "bankTransfer",
];
/// Fields that belong to a card payment only (`paymentMethod: CARD`).
pub const CARD_ONLY_FIELDS: &[&str] = &["card", "installment", "installmentQuoteToken", "force3D"];
/// Body fields of `confirm_payment_action`. The hosted form's token fields
/// (`token`, `checkoutSessionProof`) are never sent by a merchant server.
pub const PAYMENT_ACTION_CONFIRM_FIELDS: &[&str] = &["otp", "consentAccepted", "fields"];
/// Most basket items of a shopping-loan payment (`SHOPPING_LOAN`); the basket is required.
pub const SHOPPING_LOAN_MAX_BASKET_ITEMS: usize = 100;

const METHOD_FIELDS_MAX_KEYS: usize = 32;
const METHOD_FIELD_MAX_VALUE_LENGTH: usize = 1024;
const METHOD_FIELD_MAX_GROUP_ITEMS: usize = 10;
const METHOD_FIELD_MAX_NESTED_KEYS: usize = 16;

fn given<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    object.get(key).filter(|v| !v.is_null())
}

fn valid_field_key(key: &str) -> bool {
    let b = key.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && b[0].is_ascii_alphabetic()
        && b.iter().all(u8::is_ascii_alphanumeric)
        && !["__proto__", "constructor", "prototype"].contains(&key)
}

pub(crate) fn valid_provider_code(code: &str) -> bool {
    (1..=64).contains(&code.len())
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn scalar(value: &Value) -> bool {
    match value {
        Value::String(s) => s.encode_utf16().count() <= METHOD_FIELD_MAX_VALUE_LENGTH,
        Value::Number(_) | Value::Bool(_) => true,
        _ => false,
    }
}

fn nested_object(object: &Map<String, Value>) -> Result<()> {
    if object.len() > METHOD_FIELD_MAX_NESTED_KEYS {
        return Err(Error::Invalid("method field object: at most 16 fields"));
    }
    for (key, value) in object {
        if !valid_field_key(key) {
            return Err(Error::Invalid("method field name"));
        }
        if !scalar(value) {
            return Err(Error::Invalid(
                "method field object value (string up to 1024, number or boolean)",
            ));
        }
    }
    Ok(())
}

fn field_value(value: &Value, allow_list: bool) -> Result<()> {
    if scalar(value) {
        return Ok(());
    }
    match value {
        Value::Object(object) => nested_object(object),
        Value::Array(items) if allow_list => {
            if items.len() > METHOD_FIELD_MAX_GROUP_ITEMS {
                return Err(Error::Invalid("method field list: at most 10 items"));
            }
            for item in items {
                let object = item
                    .as_object()
                    .ok_or(Error::Invalid("method field list item object"))?;
                if object.len() > METHOD_FIELDS_MAX_KEYS {
                    return Err(Error::Invalid("method field list item: at most 32 fields"));
                }
                for (key, value) in object {
                    if !valid_field_key(key) {
                        return Err(Error::Invalid("method field name"));
                    }
                    field_value(value, false)?;
                }
            }
            Ok(())
        }
        Value::String(_) => Err(Error::Invalid(
            "method field value: at most 1024 characters",
        )),
        _ => Err(Error::Invalid(
            "method field value (string, number, boolean, object or list of objects)",
        )),
    }
}

/// Structure of a method block or of confirm `fields`: a plain object of at
/// most 32 fields named `^[A-Za-z][A-Za-z0-9]{0,63}$`; a value is a string (at
/// most 1024 characters), a number, a boolean, an object of such values
/// (`ADDRESS`, at most 16 fields) or a list of at most 10 objects (`GROUP`).
/// `providerCode` is never a field (it is its own argument); `skip` names a
/// key validated elsewhere (the promissory-note `schedule`).
pub(crate) fn validate_method_fields(value: &Value, skip: Option<&str>) -> Result<()> {
    let object = value.as_object().ok_or(Error::Invalid(
        "method fields: plain object of field values",
    ))?;
    if object.len() > METHOD_FIELDS_MAX_KEYS {
        return Err(Error::Invalid("method fields: at most 32 fields"));
    }
    for (key, field) in object {
        if key == "providerCode" {
            return Err(Error::Invalid(
                "method fields must not carry providerCode; pass it as its own argument",
            ));
        }
        if !valid_field_key(key) {
            return Err(Error::Invalid("method field name"));
        }
        if skip == Some(key.as_str()) {
            continue;
        }
        field_value(field, true)?;
    }
    Ok(())
}

fn shopping_loan_basket(method: &str, basket: Option<&Value>) -> Result<()> {
    if method != payment_method::SHOPPING_LOAN {
        return Ok(());
    }
    let items = basket
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .ok_or(Error::Invalid(
            "SHOPPING_LOAN requires basketItems (the bank sets the loan term from the basket)",
        ))?;
    if items.len() > SHOPPING_LOAN_MAX_BASKET_ITEMS {
        return Err(Error::Invalid(
            "SHOPPING_LOAN takes at most 100 basket items",
        ));
    }
    Ok(())
}

/// Checks a raw `direct/payment` body with the gateway's rules before sending.
pub(crate) fn validate_direct_method_payment(body: &Value) -> Result<()> {
    let object = body
        .as_object()
        .ok_or(Error::Invalid("direct method payment JSON object"))?;
    if ["price", "paidPrice"]
        .iter()
        .any(|k| object.contains_key(*k))
    {
        return Err(Error::Invalid("removed payment field"));
    }
    crate::money::validate_fields(body, false)?;
    if object
        .keys()
        .any(|k| !DIRECT_METHOD_PAYMENT_FIELDS.contains(&k.as_str()))
    {
        return Err(Error::Invalid("unknown direct method payment field"));
    }
    let text = |key: &str| {
        given(object, key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    let (Some(code), Some(amount), Some(currency), Some(order_id)) = (
        text("paymentMethod"),
        text("amount"),
        text("currency"),
        text("orderId"),
    ) else {
        return Err(Error::Invalid(
            "direct method payment requires paymentMethod, amount, currency and orderId",
        ));
    };
    crate::money::parse_minor(amount, true, false)?;
    if currency.len() != 3 || !currency.bytes().all(|c| c.is_ascii_uppercase()) {
        return Err(Error::Invalid("uppercase currency"));
    }
    if order_id.trim().is_empty() || order_id.len() > 200 {
        return Err(Error::Invalid("order ID"));
    }
    let method = payment_method::normalize(code)?;
    let own = payment_method::block_name(method)?;
    let card = method == payment_method::CARD;
    if !card && CARD_ONLY_FIELDS.iter().any(|f| given(object, f).is_some()) {
        return Err(Error::Invalid(
            "card, installment, installmentQuoteToken and force3D need paymentMethod CARD",
        ));
    }
    if METHOD_BLOCK_FIELDS
        .iter()
        .any(|f| *f != own && given(object, f).is_some())
    {
        return Err(Error::Invalid(
            "direct method payment carries only the chosen method's block",
        ));
    }
    if !card && !METHOD_BLOCK_FIELDS.contains(&own.as_str()) {
        return Err(Error::Invalid(
            "payment method cannot be paid through direct/payment (see list_payment_methods)",
        ));
    }
    let block = given(object, &own).ok_or(Error::Invalid(
        "direct method payment requires the chosen method's block",
    ))?;
    if card {
        if block.as_object().is_none_or(Map::is_empty) {
            return Err(Error::Invalid("card block: non-empty card details object"));
        }
    } else {
        let mut fields = block
            .as_object()
            .ok_or(Error::Invalid("method block object"))?
            .clone();
        if let Some(provider) = fields.remove("providerCode") {
            if method == payment_method::BANK_TRANSFER {
                return Err(Error::Invalid(
                    "BANK_TRANSFER takes no providerCode in the bankTransfer block",
                ));
            }
            if !provider.is_null() && !provider.as_str().is_some_and(valid_provider_code) {
                return Err(Error::Invalid(
                    "providerCode (letters, digits, _ and -; from list_payment_methods)",
                ));
            }
        }
        let promissory = method == payment_method::PROMISSORY_NOTE;
        validate_method_fields(&Value::Object(fields), promissory.then_some("schedule"))?;
        if promissory {
            validate_plan(block, amount)?;
        }
    }
    shopping_loan_basket(method, given(object, "basketItems"))?;
    for flag in ["preauth", "force3D"] {
        if object.get(flag).is_some_and(|v| !v.is_boolean()) {
            return Err(Error::Invalid("preauth and force3D must be booleans"));
        }
    }
    Ok(())
}

/// A body for [`Payments::direct_method_payment`]: the typed
/// [`DirectMethodPaymentRequest`] or a raw `serde_json::Value`. Both are
/// checked with the same rules before any request.
pub trait DirectMethodPaymentBody {
    /// The JSON body to send; fails with `Error::Invalid` before network access.
    fn to_body(&self) -> Result<Value>;
}
impl DirectMethodPaymentBody for Value {
    fn to_body(&self) -> Result<Value> {
        validate_direct_method_payment(self)?;
        Ok(self.clone())
    }
}

enum PromissoryNotePlan {
    Schedule(Vec<PromissoryNoteScheduleRow>),
    Count(u8),
}

/// Typed body of `POST /payments/direct/payment`. Choose the method with
/// exactly one of [`with_method_fields`](Self::with_method_fields) (any method
/// other than card) or [`with_card`](Self::with_card). Every rule is checked
/// when the body is built, before any request.
///
/// `amount` is the only amount field: the total in canonical minor units,
/// without any installment fee.
pub struct DirectMethodPaymentRequest {
    amount: String,
    currency: String,
    order_id: String,
    payment_method: Option<String>,
    provider_code: Option<String>,
    block: Option<Value>,
    card: bool,
    plan: Option<PromissoryNotePlan>,
    options: Map<String, Value>,
    buyer: Option<Buyer>,
    billing_address: Option<Address>,
    shipping_address: Option<Address>,
    basket_items: Option<Vec<BasketItem>>,
}
impl DirectMethodPaymentRequest {
    /// Creates a request from trusted server-side order values.
    pub fn new(
        amount: impl Into<String>,
        currency: impl Into<String>,
        order_id: impl Into<String>,
    ) -> Self {
        Self {
            amount: amount.into(),
            currency: currency.into(),
            order_id: order_id.into(),
            payment_method: None,
            provider_code: None,
            block: None,
            card: false,
            plan: None,
            options: Map::new(),
            buyer: None,
            billing_address: None,
            shipping_address: None,
            basket_items: None,
        }
    }
    /// Pays with a method other than card. `fields` are the provider's
    /// `inputFields` values from `list_payment_methods` (`json!({})` when the
    /// provider asks for nothing). `provider_code` is required when the
    /// method's `providerSelection` is `REQUIRED`; `None` lets the platform
    /// pick on a `PLATFORM` method. `BANK_TRANSFER` takes no provider code and
    /// the buyer's bank as `{ "bankCode": … }` ([`crate::BANK_TRANSFER_OTHER_BANKS_CODE`]
    /// for other banks).
    pub fn with_method_fields(
        mut self,
        payment_method: impl Into<String>,
        provider_code: Option<&str>,
        fields: Value,
    ) -> Self {
        self.payment_method = Some(payment_method.into());
        self.provider_code = provider_code.map(str::to_owned);
        self.block = Some(fields);
        self.card = false;
        self
    }
    /// Promissory-note plan: one note per row; amounts add up to `amount`,
    /// due dates ascending, at most 24 rows. Replaces a note count.
    pub fn with_promissory_note_schedule(mut self, rows: Vec<PromissoryNoteScheduleRow>) -> Self {
        self.plan = Some(PromissoryNotePlan::Schedule(rows));
        self
    }
    /// Promissory-note plan: the number of notes (1–24, within your provider
    /// connection limits). Replaces a schedule.
    pub fn with_promissory_note_count(mut self, count: u8) -> Self {
        self.plan = Some(PromissoryNotePlan::Count(count));
        self
    }
    /// Pays by card: new card fields, or `cardUserKey` + `cardToken` for a stored card.
    pub fn with_card(mut self, card: Value) -> Self {
        self.payment_method = Some(payment_method::CARD.into());
        self.provider_code = None;
        self.block = Some(card);
        self.card = true;
        self
    }
    /// Card only. `true` charges with 3D Secure (a `REDIRECT` action).
    pub fn with_force_3d(mut self, value: bool) -> Self {
        self.options.insert("force3D".into(), json!(value));
        self
    }
    /// Card only. `quote_token` binds the charge to an installment quote.
    pub fn with_installment(mut self, count: u8, quote_token: Option<&str>) -> Self {
        self.options.insert("installment".into(), json!(count));
        match quote_token {
            Some(token) => self
                .options
                .insert("installmentQuoteToken".into(), json!(token)),
            None => self.options.remove("installmentQuoteToken"),
        };
        self
    }
    /// Pre-authorization: `true` holds the amount (`AUTHORIZED`); capture or void it later.
    pub fn with_preauth(mut self, value: bool) -> Self {
        self.options.insert("preauth".into(), json!(value));
        self
    }
    /// Where the buyer's browser returns after a `REDIRECT` action.
    pub fn with_callback_url(self, value: impl Into<String>) -> Self {
        self.with_text("callbackUrl", value.into())
    }
    pub fn with_locale(self, value: impl Into<String>) -> Self {
        self.with_text("locale", value.into())
    }
    pub fn with_description(self, value: impl Into<String>) -> Self {
        self.with_text("description", value.into())
    }
    pub fn with_customer_id(self, value: impl Into<String>) -> Self {
        self.with_text("customerId", value.into())
    }
    pub fn with_payment_channel(self, value: impl Into<String>) -> Self {
        self.with_text("paymentChannel", value.into())
    }
    pub fn with_payment_group(self, value: impl Into<String>) -> Self {
        self.with_text("paymentGroup", value.into())
    }
    pub fn with_buyer(mut self, buyer: Buyer) -> Self {
        self.buyer = Some(buyer);
        self
    }
    pub fn with_billing_address(mut self, address: Address) -> Self {
        self.billing_address = Some(address);
        self
    }
    pub fn with_shipping_address(mut self, address: Address) -> Self {
        self.shipping_address = Some(address);
        self
    }
    /// Basket lines; required (1–100 items) for `SHOPPING_LOAN`.
    pub fn with_basket_items(mut self, items: Vec<BasketItem>) -> Self {
        self.basket_items = Some(items);
        self
    }
    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.options.insert("metadata".into(), metadata);
        self
    }
    fn with_text(mut self, key: &str, value: String) -> Self {
        self.options.insert(key.into(), Value::String(value));
        self
    }
    fn method_block(&self, method: &str) -> Result<Value> {
        let fields = self.block.as_ref().ok_or(Error::Invalid("method fields"))?;
        if method == payment_method::CARD || self.card {
            if !self.card {
                return Err(Error::Invalid("use with_card to pay by card"));
            }
            return Ok(fields.clone());
        }
        let name = payment_method::block_name(method)?;
        if !METHOD_BLOCK_FIELDS.contains(&name.as_str()) {
            return Err(Error::Invalid(
                "payment method cannot be paid through direct/payment (see list_payment_methods)",
            ));
        }
        validate_method_fields(fields, None)?;
        let mut block = fields.as_object().cloned().unwrap_or_default();
        let promissory = method == payment_method::PROMISSORY_NOTE;
        if promissory && block.contains_key("schedule") {
            return Err(Error::Invalid(
                "pass the promissory-note schedule with with_promissory_note_schedule",
            ));
        }
        if promissory && let Some(count) = block.get("noteCount") {
            let count = normalize_note_count(count)?;
            block.insert("noteCount".into(), Value::String(count));
        }
        if let Some(code) = &self.provider_code {
            if method == payment_method::BANK_TRANSFER {
                return Err(Error::Invalid(
                    "BANK_TRANSFER takes no providerCode: pass None and the buyer's bank as { bankCode }",
                ));
            }
            if !valid_provider_code(code) {
                return Err(Error::Invalid(
                    "providerCode (letters, digits, _ and -; from list_payment_methods)",
                ));
            }
            block.insert("providerCode".into(), Value::String(code.clone()));
        }
        match (&self.plan, promissory) {
            (Some(_), false) => {
                return Err(Error::Invalid(
                    "a promissory-note plan needs paymentMethod PROMISSORY_NOTE",
                ));
            }
            (Some(_), true) if block.contains_key("noteCount") => {
                return Err(Error::Invalid(
                    "the promissory-note plan is given twice (noteCount in the fields and a plan)",
                ));
            }
            (Some(PromissoryNotePlan::Schedule(rows)), true) => {
                block.insert("schedule".into(), to_value(rows)?);
            }
            (Some(PromissoryNotePlan::Count(count)), true) => {
                block.insert(
                    "noteCount".into(),
                    Value::String(normalize_note_count(&json!(count))?),
                );
            }
            (None, _) => {}
        }
        Ok(Value::Object(block))
    }
}
fn to_value<T: serde::Serialize>(value: &T) -> Result<Value> {
    serde_json::to_value(value).map_err(|_| Error::Invalid("direct method payment JSON"))
}
impl DirectMethodPaymentBody for DirectMethodPaymentRequest {
    fn to_body(&self) -> Result<Value> {
        let code = self.payment_method.as_deref().ok_or(Error::Invalid(
            "choose the payment method with with_method_fields or with_card",
        ))?;
        let method = payment_method::normalize(code)?;
        let mut body = self.options.clone();
        body.insert("paymentMethod".into(), json!(method));
        body.insert("amount".into(), json!(self.amount));
        body.insert("currency".into(), json!(self.currency));
        body.insert("orderId".into(), json!(self.order_id));
        if let Some(buyer) = &self.buyer {
            body.insert("buyer".into(), to_value(buyer)?);
        }
        if let Some(address) = &self.billing_address {
            body.insert("billingAddress".into(), to_value(address)?);
        }
        if let Some(address) = &self.shipping_address {
            body.insert("shippingAddress".into(), to_value(address)?);
        }
        if let Some(items) = &self.basket_items {
            body.insert("basketItems".into(), to_value(items)?);
        }
        body.insert(
            payment_method::block_name(method)?,
            self.method_block(method)?,
        );
        let body = Value::Object(body);
        validate_direct_method_payment(&body)?;
        Ok(body)
    }
}

fn confirm_body(input: &Value) -> Result<Value> {
    let object = input.as_object().ok_or(Error::Invalid(
        "confirm input object { otp, consentAccepted, fields }",
    ))?;
    if object
        .keys()
        .any(|k| !PAYMENT_ACTION_CONFIRM_FIELDS.contains(&k.as_str()))
    {
        return Err(Error::Invalid(
            "unknown confirm field (a merchant server sends only otp, consentAccepted, fields)",
        ));
    }
    let mut body = Map::new();
    if let Some(otp) = given(object, "otp") {
        let valid = otp
            .as_str()
            .is_some_and(|s| (1..=16).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit()));
        if !valid {
            return Err(Error::Invalid("otp: the code the buyer entered, as digits"));
        }
        body.insert("otp".into(), otp.clone());
    }
    if let Some(consent) = given(object, "consentAccepted") {
        if !consent.is_boolean() {
            return Err(Error::Invalid("consentAccepted must be a boolean"));
        }
        body.insert("consentAccepted".into(), consent.clone());
    }
    if let Some(fields) = given(object, "fields") {
        validate_method_fields(fields, None)?;
        body.insert("fields".into(), fields.clone());
    }
    if body.is_empty() {
        return Err(Error::Invalid(
            "confirm_payment_action needs otp, consentAccepted or fields",
        ));
    }
    Ok(Value::Object(body))
}

fn action_path(payment_id: &str, action_id: &str, operation: &str) -> Result<String> {
    Ok(format!(
        "/api/v1/payments/{}/actions/{}/{operation}",
        valid_id(payment_id)?,
        valid_id(action_id)?
    ))
}

impl Payments<'_> {
    /// The payment methods open for your account (`GET /payments/methods`):
    /// per method its `blockName`, `providerSelection`, `providerSelectionMode`,
    /// `preauth` support, limits, split behavior and providers with the
    /// `inputFields` to send in the method block. Read provider codes from this
    /// answer; never hard-code them. `Some(code)` lists that method only (`QR`
    /// is sent as `QR_CODE`; a code must match `^[A-Z][A-Z0-9_]{0,31}$`).
    /// Same as [`methods`](Self::methods).
    pub async fn list_payment_methods(&self, payment_method: Option<&str>) -> Result<Response> {
        self.methods(payment_method).await
    }
    /// Starts a payment with any payment method from the merchant server
    /// (`POST /payments/direct/payment`). `body` is a
    /// [`DirectMethodPaymentRequest`] or a raw `serde_json::Value`; both are
    /// checked before sending, the order details included (`buyer`,
    /// `billingAddress`, `basketItems`; `shippingAddress` for a `PHYSICAL`
    /// item). The persisted `key` is required; reuse it when
    /// retrying the same payment. `data` holds `paymentId`, `status`,
    /// `paymentMethod`, `providerCode` and, when the buyer must act, `action`
    /// (HTTP 202); see [`crate::BankTransferInstruction`] for a `DISPLAY` action.
    pub async fn direct_method_payment<B: DirectMethodPaymentBody + ?Sized>(
        &self,
        body: &B,
        key: &str,
    ) -> Result<Response> {
        let body = body.to_body()?;
        require_key(key)?;
        crate::order_details::assert_order_details(
            &body,
            "A direct method payment",
            crate::OrderDetailsScope::Payment,
        )?;
        self.client
            .send(
                Request::new(Method::POST, "/api/v1/payments/direct/payment")
                    .json(body)
                    .idempotency_key(key),
            )
            .await
    }
    /// Completes the open action of a payment
    /// (`POST /payments/{paymentId}/actions/{actionId}/confirm`) with
    /// `{ otp, consentAccepted, fields }` (at least one). It does not start a
    /// new charge. A rejected code is a 422 with `details.attemptsRemaining`.
    pub async fn confirm_payment_action(
        &self,
        payment_id: &str,
        action_id: &str,
        input: Value,
    ) -> Result<Response> {
        let path = action_path(payment_id, action_id, "confirm")?;
        let body = confirm_body(&input)?;
        self.client
            .send(Request::new(Method::POST, path).json(body))
            .await
    }
    /// Sends a new one-time code
    /// (`POST /payments/{paymentId}/actions/{actionId}/resend`). The previous
    /// action is closed; confirm with the new `data.action.id`.
    pub async fn resend_payment_action(
        &self,
        payment_id: &str,
        action_id: &str,
    ) -> Result<Response> {
        self.client
            .send(Request::new(
                Method::POST,
                action_path(payment_id, action_id, "resend")?,
            ))
            .await
    }
}
