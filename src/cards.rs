//! Card lookups and stored cards:
//!
//! - `POST /api/v1/bin/info` (card, bank and installment eligibility of a BIN)
//! - `POST /api/v1/bin/installments` (installment table and quote token)
//! - `GET  /api/v1/bin/card-taxonomy` (brand, family and bank reference lists)
//! - `POST|GET /api/v1/payments/cards`, `GET|DELETE /api/v1/payments/cards/{token}`
//!
//! A card number or security code never appears in an error or in debug
//! output of these types.

use crate::{
    Error, Method, Request, Response, Result,
    client::{require_key, valid_id},
    payments::{
        Payments, ascending_installments, currency, enabled_installments, https_callback_url, money,
    },
};
use serde::Serialize;
use serde_json::Value;
use std::fmt;

/// Body of `installment_options` (`POST /bin/installments`).
///
/// `amount` is the basket total in canonical minor units, without any
/// installment fee. With `bin_or_pan` and an `installment` above 1 the answer
/// carries the `quoteToken` that `direct_method_payment` sends as
/// `installmentQuoteToken`. `enabled_installments` follows the rules of
/// [`IntentRequest::enabled_installments`](crate::IntentRequest): 1–36,
/// distinct, includes 1, sent in ascending order.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct InstallmentOptionsRequest {
    pub amount: String,
    pub currency: String,
    /// First 6–19 digits of the card. Kept out of debug output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_or_pan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installment: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_brand: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "ascending_installments"
    )]
    pub enabled_installments: Option<Vec<u8>>,
}
impl fmt::Debug for InstallmentOptionsRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InstallmentOptionsRequest")
            .field("amount", &self.amount)
            .field("currency", &self.currency)
            .field(
                "bin_or_pan",
                &self.bin_or_pan.as_ref().map(|_| "[REDACTED]"),
            )
            .field("installment", &self.installment)
            .field("card_brand", &self.card_brand)
            .field("enabled_installments", &self.enabled_installments)
            .finish()
    }
}
impl InstallmentOptionsRequest {
    /// Creates a request for the installment table of `amount`.
    pub fn new(amount: impl Into<String>, currency: impl Into<String>) -> Self {
        Self {
            amount: amount.into(),
            currency: currency.into(),
            bin_or_pan: None,
            installment: None,
            card_brand: None,
            enabled_installments: None,
        }
    }
    /// The card's BIN or number prefix (6–19 digits).
    pub fn with_bin_or_pan(mut self, value: impl Into<String>) -> Self {
        self.bin_or_pan = Some(value.into());
        self
    }
    /// The installment count to quote (1–36).
    pub fn with_installment(mut self, count: u8) -> Self {
        self.installment = Some(count);
        self
    }
    /// A card brand from `card_taxonomy` when no BIN is known yet.
    pub fn with_card_brand(mut self, value: impl Into<String>) -> Self {
        self.card_brand = Some(value.into());
        self
    }
    /// Limits the table to these counts, e.g. `vec![1, 2, 3]`.
    pub fn with_enabled_installments(mut self, counts: Vec<u8>) -> Self {
        self.enabled_installments = Some(counts);
        self
    }
    fn validate(&self) -> Result<()> {
        money(&self.amount)?;
        currency(&self.currency)?;
        if let Some(value) = &self.bin_or_pan {
            bin_or_pan(value)?;
        }
        if self.installment.is_some_and(|n| {
            !(crate::MIN_ENABLED_INSTALLMENT..=crate::MAX_ENABLED_INSTALLMENT).contains(&n)
        }) {
            return Err(Error::Invalid("installment (1-36)"));
        }
        if self
            .card_brand
            .as_ref()
            .is_some_and(|s| s.trim().is_empty() || s.len() > 64)
        {
            return Err(Error::Invalid("card brand"));
        }
        if let Some(counts) = &self.enabled_installments {
            enabled_installments(counts)?;
        }
        Ok(())
    }
}

/// Body of `store_card` (`POST /payments/cards`): the card, your stable key
/// for the card holder (`cardUserKey`) and the HTTPS URL the buyer returns to
/// after a verification step. Debug output never shows the card.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct StoreCardRequest {
    /// Card fields as the gateway names them (holder name, number, expiry, CVC).
    pub card: Value,
    pub card_user_key: String,
    pub callback_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}
impl fmt::Debug for StoreCardRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoreCardRequest")
            .field("card", &"[REDACTED]")
            .field("currency", &self.currency)
            .finish_non_exhaustive()
    }
}
impl StoreCardRequest {
    /// Creates a request from the card fields, the card holder key and the return URL.
    pub fn new(
        card: Value,
        card_user_key: impl Into<String>,
        callback_url: impl Into<String>,
    ) -> Self {
        Self {
            card,
            card_user_key: card_user_key.into(),
            callback_url: callback_url.into(),
            currency: None,
        }
    }
    /// Currency of the verification step (`TRY`, `USD`, …).
    pub fn with_currency(mut self, value: impl Into<String>) -> Self {
        self.currency = Some(value.into());
        self
    }
    fn validate(&self) -> Result<()> {
        if self.card.as_object().is_none_or(|card| card.is_empty()) {
            return Err(Error::Invalid("card object"));
        }
        card_user_key(&self.card_user_key)?;
        https_callback_url(&self.callback_url)?;
        if let Some(code) = &self.currency {
            currency(code)?;
        }
        Ok(())
    }
}

/// 6 to 19 ASCII digits. The value itself never enters the error.
fn bin_or_pan(value: &str) -> Result<()> {
    if (6..=19).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_digit()) {
        Ok(())
    } else {
        Err(Error::Invalid("BIN or card number prefix (6-19 digits)"))
    }
}
fn card_user_key(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 256 {
        Err(Error::Invalid("card user key"))
    } else {
        Ok(())
    }
}
fn stored_card_path(token: &str) -> Result<String> {
    Ok(format!("/api/v1/payments/cards/{}", valid_id(token)?))
}

impl Payments<'_> {
    /// Card, bank and installment eligibility of a BIN or card number prefix
    /// (`POST /bin/info`, 6–19 digits, checked locally).
    pub async fn bin_info(&self, bin_or_pan_prefix: &str) -> Result<Response> {
        bin_or_pan(bin_or_pan_prefix)?;
        self.client
            .send(
                Request::new(Method::POST, "/api/v1/bin/info")
                    .json(serde_json::json!({ "binOrPan": bin_or_pan_prefix })),
            )
            .await
    }
    /// Installment table of an amount (`POST /bin/installments`). With a BIN
    /// and an installment above 1, `data` carries the binding `quoteToken`.
    pub async fn installment_options(
        &self,
        request: &InstallmentOptionsRequest,
    ) -> Result<Response> {
        request.validate()?;
        let body = serde_json::to_value(request)
            .map_err(|_| Error::Invalid("installment options JSON"))?;
        self.client
            .send(Request::new(Method::POST, "/api/v1/bin/installments").json(body))
            .await
    }
    /// Card brand, family and bank reference lists (`GET /bin/card-taxonomy`).
    pub async fn card_taxonomy(&self) -> Result<Response> {
        self.client
            .send(Request::new(Method::GET, "/api/v1/bin/card-taxonomy"))
            .await
    }
    /// Stores a card for a card holder (`POST /payments/cards`). The persisted
    /// `key` is required; reuse it when retrying.
    pub async fn store_card(&self, request: &StoreCardRequest, key: &str) -> Result<Response> {
        request.validate()?;
        require_key(key)?;
        let body = serde_json::to_value(request).map_err(|_| Error::Invalid("stored card JSON"))?;
        self.client
            .send(
                Request::new(Method::POST, "/api/v1/payments/cards")
                    .json(body)
                    .idempotency_key(key),
            )
            .await
    }
    /// Stored cards of a card holder (`GET /payments/cards?cardUserKey=`).
    pub async fn list_stored_cards(&self, card_user_key_value: &str) -> Result<Response> {
        card_user_key(card_user_key_value)?;
        let mut request = Request::new(Method::GET, "/api/v1/payments/cards");
        request
            .query
            .push(("cardUserKey".into(), card_user_key_value.into()));
        self.client.send(request).await
    }
    /// One stored card by its token (`GET /payments/cards/{token}`).
    pub async fn get_stored_card(&self, stored_card_token: &str) -> Result<Response> {
        self.client
            .send(Request::new(
                Method::GET,
                stored_card_path(stored_card_token)?,
            ))
            .await
    }
    /// Deactivates a stored card (`DELETE /payments/cards/{token}`). The
    /// persisted `key` is required.
    pub async fn deactivate_stored_card(
        &self,
        stored_card_token: &str,
        key: &str,
    ) -> Result<Response> {
        let path = stored_card_path(stored_card_token)?;
        require_key(key)?;
        self.client
            .send(Request::new(Method::DELETE, path).idempotency_key(key))
            .await
    }
}
