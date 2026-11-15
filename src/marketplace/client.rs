//! `client.marketplace()`: split charges across sub-dealers, the sub-dealer
//! lifecycle, commissions, payouts and the audit log.

use super::values::{
    MARKETPLACE_CHARGE_REQUIRED_FIELDS, MARKETPLACE_ROUTES, MARKETPLACE_SELLER_FIELDS,
    marketplace_dealer_status, marketplace_dealer_type, marketplace_multi_seller_policy,
};
use crate::{
    Client, Error, Response, Result,
    client::{error_code, require_key},
    route_request::{
        RouteCall, check_member, compact, data, list_page, merge_root, object_or_empty,
        optional_text, require_text, send_route,
    },
    route_table::{CursorPage, object_of, present, text},
};
use serde_json::{Map, Value, json};

/// Marketplace — `client.marketplace()`.
///
/// **Split charges** (`/api/v1/payments/marketplace/*`) take a persisted
/// `Idempotency-Key` (16–128 printable ASCII characters, like every payment
/// mutation of this SDK). A split is aggregate per seller: group the basket
/// by vendor and pass one leg per sub-dealer, `{subDealerId, amountMinor,
/// scenario?}`; the legs must add up to `amount`, or nothing is sent. Card
/// data (card number, expiry, security code) is sent as given and never
/// logged or put in an error; a direct card charge is PCI DSS SAQ-D scope,
/// so most marketplaces use `create_checkout_intent` (hosted form).
///
/// **Dealer, commission, payout and audit routes** (`/api/v1/marketplace/*`)
/// are checked against [`MARKETPLACE_ROUTES`](crate::MARKETPLACE_ROUTES).
///
/// Amounts are canonical minor-unit text. Entity calls return the
/// envelope's `data`; lists are [`CursorPage`]; split charges return the
/// [`Response`] as it came.
pub struct Marketplace<'a> {
    client: &'a Client,
}
impl Client {
    /// Returns the marketplace facade; no independent HTTP client is constructed.
    pub fn marketplace(&self) -> Marketplace<'_> {
        Marketplace { client: self }
    }
}

impl Marketplace<'_> {
    async fn call(&self, name: &str, call: RouteCall<'_>) -> Result<Response> {
        send_route(self.client, MARKETPLACE_ROUTES, name, call).await
    }
    async fn entity(&self, name: &str, call: RouteCall<'_>) -> Result<Value> {
        Ok(data(&self.call(name, call).await?))
    }
    async fn page(&self, name: &str, filters: &Value) -> Result<CursorPage> {
        let call = RouteCall {
            query: Some(filters),
            ..RouteCall::default()
        };
        Ok(list_page(&self.call(name, call).await?))
    }
    async fn on_dealer(&self, name: &str, dealer_id: &str, body: Option<&Value>) -> Result<Value> {
        let call = RouteCall {
            params: &[dealer_id],
            body,
            ..RouteCall::default()
        };
        self.entity(name, call).await
    }

    // Split charges

    /// Hosted-form split checkout (`POST /payments/marketplace/intents`):
    /// the checkout intent body (`amount`, `currency`, `orderId`, …) plus
    /// the seller legs; `multi_seller_policy` defaults to `ALL_OR_NOTHING`.
    /// Returns the answer as it came (`token`, `checkoutFormContent`,
    /// `paymentId` …); an answer without a token or form (a seller not
    /// eligible under `ALL_OR_NOTHING`) is `Error::Api` instead of leaving
    /// the buyer on an empty page. Same route as
    /// `payments().create_marketplace_intent`.
    pub async fn create_checkout_intent(
        &self,
        request: &Value,
        sellers: &Value,
        multi_seller_policy: Option<&str>,
        idempotency_key: &str,
    ) -> Result<Response> {
        let body = split_body(
            object_of(
                request,
                "marketplace checkout request must be a JSON object",
            )?,
            sellers,
            multi_seller_policy,
        )?;
        let response = self
            .charge("createCheckoutIntent", &body, idempotency_key)
            .await?;
        let intent = merge_root(&response);
        if text(intent.get("token")).is_none() && text(intent.get("checkoutFormContent")).is_none()
        {
            return Err(Error::Api {
                status: response.status,
                code: error_code(&response)
                    .unwrap_or_else(|| "MARKETPLACE_INTENT_WITHOUT_TOKEN".to_owned()),
                request_id: response.request_id.clone(),
            });
        }
        Ok(response)
    }
    /// Direct split card charge from a raw body (`amount`, `currency`,
    /// `orderId`, `card`, `buyer`, `billingAddress`, `basketItems`,
    /// `sellers`, `multiSellerPolicy`, `preauth?` …):
    /// `POST /payments/marketplace/3d` when `three_ds`, `/2d` otherwise.
    /// Prefer `charge_marketplace_card2d` / `charge_marketplace_card3d`,
    /// which check the body and the legs.
    pub async fn charge_marketplace_card(
        &self,
        payload: &Value,
        three_ds: bool,
        idempotency_key: &str,
    ) -> Result<Response> {
        let body = object_of(payload, "marketplace charge payload must be a JSON object")?;
        check_charge_amount(body)?;
        let name = if three_ds {
            "chargeMarketplaceCard3d"
        } else {
            "chargeMarketplaceCard2d"
        };
        self.charge(name, payload, idempotency_key).await
    }
    /// Split card charge without 3-D Secure: the card payment body (with
    /// every field of [`MARKETPLACE_CHARGE_REQUIRED_FIELDS`](crate::MARKETPLACE_CHARGE_REQUIRED_FIELDS)
    /// and at least one basket item) plus the seller legs.
    pub async fn charge_marketplace_card2d(
        &self,
        request: &Value,
        sellers: &Value,
        multi_seller_policy: Option<&str>,
        idempotency_key: &str,
    ) -> Result<Response> {
        let body = charge_body(request, sellers, multi_seller_policy)?;
        self.charge_marketplace_card(&body, false, idempotency_key)
            .await
    }
    /// Split card charge with 3-D Secure: the card payment body plus the
    /// seller legs (as `charge_marketplace_card2d`).
    pub async fn charge_marketplace_card3d(
        &self,
        request: &Value,
        sellers: &Value,
        multi_seller_policy: Option<&str>,
        idempotency_key: &str,
    ) -> Result<Response> {
        let body = charge_body(request, sellers, multi_seller_policy)?;
        self.charge_marketplace_card(&body, true, idempotency_key)
            .await
    }
    async fn charge(&self, name: &str, body: &Value, key: &str) -> Result<Response> {
        let key = key.trim();
        require_key(key)?;
        let call = RouteCall {
            body: Some(body),
            key: Some(key),
            ..RouteCall::default()
        };
        self.call(name, call).await
    }

    // Sub-dealers

    /// Creates a sub-dealer: `{legalName, taxId?, dealerType?, categoryCode?,
    /// email?, externalRef?}`. The dealer then onboards itself (KYC, IBAN,
    /// contract) before it is `ACTIVE`.
    pub async fn create_dealer(&self, request: &Value) -> Result<Value> {
        let body = dealer_body(request)?;
        let call = RouteCall {
            body: Some(&body),
            ..RouteCall::default()
        };
        self.entity("createDealer", call).await
    }
    /// Several dealers at once (an import); each row as in `create_dealer`.
    pub async fn bulk_create_dealers(&self, dealers: &[Value]) -> Result<Value> {
        if dealers.is_empty() {
            return Err(Error::Invalid(
                "bulkCreateDealers needs a non-empty list of dealers",
            ));
        }
        let rows = dealers
            .iter()
            .map(dealer_body)
            .collect::<Result<Vec<_>>>()?;
        let body = json!({ "dealers": rows });
        let call = RouteCall {
            body: Some(&body),
            ..RouteCall::default()
        };
        self.entity("bulkCreateDealers", call).await
    }
    /// Sub-dealers; filters `status`, `search`, `limit`, `offset`.
    pub async fn list_dealers(&self, filters: &Value) -> Result<CursorPage> {
        self.page("listDealers", filters).await
    }
    /// One sub-dealer.
    pub async fn get_dealer(&self, dealer_id: &str) -> Result<Value> {
        self.on_dealer("getDealer", dealer_id, None).await
    }
    /// Sets the dealer's status ([`marketplace_dealer_status`](crate::marketplace_dealer_status)).
    pub async fn update_dealer_status(&self, dealer_id: &str, status: &str) -> Result<Value> {
        if !marketplace_dealer_status::ALL.contains(&status) {
            return Err(Error::Invalid(
                "status must be DRAFT, PENDING_KYC, ACTIVE, SUSPENDED, HOLD or CLOSED",
            ));
        }
        let body = json!({ "status": status });
        self.on_dealer("updateDealerStatus", dealer_id, Some(&body))
            .await
    }
    /// Self-onboarding link: `{token, url, …}` (`token` also starts the
    /// embedded onboarding SDK); [`marketplace_onboarding_url`](crate::marketplace_onboarding_url)
    /// gives the URL. Fields at the envelope root and in `data` are merged.
    pub async fn onboarding_token(&self, dealer_id: &str) -> Result<Value> {
        let call = RouteCall {
            params: &[dealer_id],
            ..RouteCall::default()
        };
        Ok(merge_root(&self.call("onboardingToken", call).await?))
    }
    /// Onboarding / application status.
    pub async fn dealer_application(&self, dealer_id: &str) -> Result<Value> {
        self.on_dealer("dealerApplication", dealer_id, None).await
    }
    /// Submits the dealer's application (`Value::Null` for an empty body).
    pub async fn submit_dealer_application(
        &self,
        dealer_id: &str,
        payload: &Value,
    ) -> Result<Value> {
        let body = Value::Object(object_or_empty(payload, "payload must be a JSON object")?);
        self.on_dealer("submitDealerApplication", dealer_id, Some(&body))
            .await
    }
    /// Commission / terms definition.
    pub async fn set_dealer_terms(&self, dealer_id: &str, payload: &Value) -> Result<Value> {
        object_of(payload, "payload must be a JSON object")?;
        self.on_dealer("setDealerTerms", dealer_id, Some(payload))
            .await
    }
    /// Signature / contract acceptance (`Value::Null` for an empty body).
    pub async fn sign_dealer_contract(&self, dealer_id: &str, payload: &Value) -> Result<Value> {
        let body = Value::Object(object_or_empty(payload, "payload must be a JSON object")?);
        self.on_dealer("signDealerContract", dealer_id, Some(&body))
            .await
    }
    /// Activates an onboarded dealer.
    pub async fn activate_dealer(&self, dealer_id: &str) -> Result<Value> {
        self.on_dealer("activateDealer", dealer_id, None).await
    }
    /// A KYC outcome, when you run the dealer's KYC yourself.
    pub async fn record_dealer_kyc_result(
        &self,
        dealer_id: &str,
        payload: &Value,
    ) -> Result<Value> {
        object_of(payload, "payload must be a JSON object")?;
        self.on_dealer("recordDealerKycResult", dealer_id, Some(payload))
            .await
    }

    // Reporting and commissions

    /// Payout ledger; filters `status`, `subDealerId`, `days`, `limit`.
    pub async fn payout_ledger(&self, filters: &Value) -> Result<CursorPage> {
        self.page("payoutLedger", filters).await
    }
    /// Split transactions; filters `status`, `subDealerId`, `limit`.
    pub async fn transactions(&self, filters: &Value) -> Result<CursorPage> {
        self.page("transactions", filters).await
    }
    /// Commission rules; filter `limit`.
    pub async fn commission_rules(&self, filters: &Value) -> Result<CursorPage> {
        self.page("commissionRules", filters).await
    }
    /// Creates or replaces a commission rule (the gateway validates the body).
    pub async fn upsert_commission_rule(&self, rule: &Value) -> Result<Value> {
        object_of(rule, "commission rule must be a JSON object")?;
        let call = RouteCall {
            body: Some(rule),
            ..RouteCall::default()
        };
        self.entity("upsertCommissionRule", call).await
    }
    /// Dry run of a rule's effect.
    pub async fn preview_commission_rule(&self, rule: &Value) -> Result<Value> {
        object_of(rule, "commission rule must be a JSON object")?;
        let call = RouteCall {
            body: Some(rule),
            ..RouteCall::default()
        };
        self.entity("previewCommissionRule", call).await
    }
    /// Effective commission of a dealer (and category) at a month-to-date
    /// volume (`mtd_volume_minor`, minor-unit text).
    pub async fn resolve_commission(
        &self,
        sub_dealer_id: &str,
        category_code: Option<&str>,
        mtd_volume_minor: Option<&str>,
    ) -> Result<Value> {
        let query = json!({
            "subDealerId": require_text(sub_dealer_id, "subDealerId is required")?,
            "categoryCode": optional_text(category_code),
            "mtdVolumeMinor": mtd_volume_minor,
        });
        let call = RouteCall {
            query: Some(&query),
            ..RouteCall::default()
        };
        self.entity("resolveCommission", call).await
    }

    // Payouts

    /// A manual payout: `{subDealerId, amountMinor, currency, reference?, …}`
    /// (`amountMinor` positive minor-unit text).
    pub async fn create_payout(&self, payload: &Value) -> Result<Value> {
        let body = object_of(payload, "payout must be a JSON object")?;
        if let Some(amount) = body.get("amountMinor").filter(|value| !value.is_null()) {
            positive_minor(amount, "amountMinor must be positive minor-unit text")?;
        }
        let call = RouteCall {
            body: Some(payload),
            ..RouteCall::default()
        };
        self.entity("createPayout", call).await
    }
    /// Marks a payout paid: `{paidAt?, reference?, …}` (`Value::Null` for none).
    pub async fn mark_payout_paid(&self, payout_id: &str, payload: &Value) -> Result<Value> {
        let body = Value::Object(object_or_empty(payload, "payload must be a JSON object")?);
        let call = RouteCall {
            params: &[payout_id],
            body: Some(&body),
            ..RouteCall::default()
        };
        self.entity("markPayoutPaid", call).await
    }
    /// Marks several payouts paid; `extra` (`paidAt?`, `reference?` …) is
    /// sent beside `payoutIds` (`Value::Null` for none).
    pub async fn bulk_mark_payouts_paid(
        &self,
        payout_ids: &[&str],
        extra: &Value,
    ) -> Result<Value> {
        if payout_ids.is_empty() {
            return Err(Error::Invalid(
                "bulkMarkPayoutsPaid needs a non-empty list of payout ids",
            ));
        }
        let ids = payout_ids
            .iter()
            .map(|id| require_text(id, "payout ids must not be empty").map(Value::String))
            .collect::<Result<Vec<_>>>()?;
        let mut body = object_or_empty(extra, "extra must be a JSON object")?;
        body.insert("payoutIds".to_owned(), Value::Array(ids));
        let body = Value::Object(body);
        let call = RouteCall {
            body: Some(&body),
            ..RouteCall::default()
        };
        self.entity("bulkMarkPayoutsPaid", call).await
    }

    // Audit log (append-only, hash-chained)

    /// Audit log; filters `limit`, `type`.
    pub async fn audit_log(&self, filters: &Value) -> Result<CursorPage> {
        self.page("auditLog", filters).await
    }
    /// Verifies the integrity of the hash chain.
    pub async fn verify_audit_log(&self) -> Result<Value> {
        self.entity("verifyAuditLog", RouteCall::default()).await
    }
}

fn dealer_body(request: &Value) -> Result<Value> {
    let body = object_of(request, "dealer must be a JSON object")?;
    check_member(
        body,
        "dealerType",
        marketplace_dealer_type::ALL,
        "dealerType must be LEGAL_ENTITY or INDIVIDUAL",
    )?;
    Ok(request.clone())
}

fn positive_minor(value: &Value, message: &'static str) -> Result<i64> {
    value
        .as_str()
        .and_then(|text| crate::money::parse_minor(text, true, false).ok())
        .ok_or(Error::Invalid(message))
}

/// The body of `charge_marketplace_card2d` / `3d`: the commercial context is required.
fn charge_body(request: &Value, sellers: &Value, policy: Option<&str>) -> Result<Value> {
    let body = object_of(request, "marketplace charge request must be a JSON object")?;
    if MARKETPLACE_CHARGE_REQUIRED_FIELDS
        .iter()
        .any(|field| !body.get(*field).is_some_and(present))
    {
        return Err(Error::Invalid(
            "a marketplace card charge requires amount, currency, orderId, card, buyer, billingAddress and basketItems",
        ));
    }
    if !body
        .get("basketItems")
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty())
    {
        return Err(Error::Invalid(
            "a marketplace card charge requires at least one basket item",
        ));
    }
    split_body(body, sellers, policy)
}

/// Adds the seller legs and the policy; the legs must add up to `amount`.
fn split_body(body: &Map<String, Value>, sellers: &Value, policy: Option<&str>) -> Result<Value> {
    if body.contains_key("sellers") || body.contains_key("multiSellerPolicy") {
        return Err(Error::Invalid(
            "pass the seller legs and multiSellerPolicy as arguments, not in the request body",
        ));
    }
    check_charge_amount(body)?;
    let policy = policy.unwrap_or(marketplace_multi_seller_policy::ALL_OR_NOTHING);
    if !marketplace_multi_seller_policy::ALL.contains(&policy) {
        return Err(Error::Invalid(
            "multiSellerPolicy must be ALL_OR_NOTHING or PARTIAL_ALLOWED",
        ));
    }
    let legs = sellers
        .as_array()
        .filter(|legs| !legs.is_empty())
        .ok_or(Error::Invalid(
            "a marketplace split requires at least one seller leg",
        ))?
        .iter()
        .map(seller_leg)
        .collect::<Result<Vec<_>>>()?;
    let mut total: i64 = 0;
    for (_, amount) in &legs {
        total = total.checked_add(*amount).ok_or(Error::Invalid(
            "the seller legs exceed the 64-bit amount range",
        ))?;
    }
    let amount = positive_minor(
        body.get("amount").unwrap_or(&Value::Null),
        "amount must be positive minor-unit text",
    )?;
    if total != amount {
        return Err(Error::Invalid(
            "the seller legs' amountMinor must add up to amount; the gateway refuses any other total",
        ));
    }
    let mut out = body.clone();
    out.insert(
        "sellers".to_owned(),
        Value::Array(legs.into_iter().map(|(leg, _)| leg).collect()),
    );
    out.insert("multiSellerPolicy".to_owned(), Value::from(policy));
    Ok(Value::Object(out))
}

fn seller_leg(seller: &Value) -> Result<(Value, i64)> {
    let leg = compact(object_of(seller, "a seller leg must be a JSON object")?);
    if leg
        .keys()
        .any(|key| !MARKETPLACE_SELLER_FIELDS.contains(&key.as_str()))
    {
        return Err(Error::Invalid(
            "a seller leg takes only subDealerId, amountMinor and scenario",
        ));
    }
    let sub_dealer_id = leg
        .get("subDealerId")
        .and_then(Value::as_str)
        .ok_or(Error::Invalid("a seller leg requires subDealerId"))?;
    let sub_dealer_id = require_text(sub_dealer_id, "a seller leg requires subDealerId")?;
    let amount_text = leg.get("amountMinor").unwrap_or(&Value::Null);
    let amount = positive_minor(
        amount_text,
        "a seller leg's amountMinor must be positive minor-unit text",
    )?;
    let mut out = Map::new();
    out.insert("subDealerId".to_owned(), Value::String(sub_dealer_id));
    out.insert("amountMinor".to_owned(), amount_text.clone());
    if let Some(scenario) = leg.get("scenario") {
        if !scenario.is_object() {
            return Err(Error::Invalid(
                "a seller leg's scenario must be a JSON object",
            ));
        }
        out.insert("scenario".to_owned(), scenario.clone());
    }
    Ok((Value::Object(out), amount))
}

/// `amount` (the basket total) is positive minor-unit text; `price` /
/// `paidPrice` are gone; `preauth` is a boolean.
fn check_charge_amount(body: &Map<String, Value>) -> Result<()> {
    if body.contains_key("price") || body.contains_key("paidPrice") {
        return Err(Error::Invalid(
            "price / paidPrice: removed from the payment body; send the amount only in amount (minor units)",
        ));
    }
    positive_minor(
        body.get("amount").unwrap_or(&Value::Null),
        "amount must be positive minor-unit text",
    )?;
    if body
        .get("preauth")
        .is_some_and(|preauth| !preauth.is_boolean())
    {
        return Err(Error::Invalid(
            "preauth must be a boolean (true = pre-authorization, false = immediate capture)",
        ));
    }
    Ok(())
}
