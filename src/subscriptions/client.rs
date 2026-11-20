//! `client.subscriptions()`: subscriptions and the plan, customer, card,
//! invoice, portal, catalog, feature, add-on and usage sub-facades.

use super::values::{
    SUBSCRIPTION_BILLING_RULE_FIELDS, SUBSCRIPTION_ROUTES, subscription_billing_frequency,
    subscription_card_verification_mode, subscription_collection_method,
    subscription_plan_change_behavior, subscription_portal_sdk_mode,
};
use crate::{
    Client, Error, Response, Result,
    route_request::{
        RouteCall, boolean_member, check_member, count_member, data, idempotency_key_of,
        integer_list, list_page, merge_root, object_member, object_or_empty, optional_text,
        require_text, send_route,
    },
    route_table::{CursorPage, object_of},
};
use serde_json::{Map, Value, json};

/// Subscriptions — `/api/v1/subscriptions/*` (`client.subscriptions()`).
///
/// Bodies and filters are JSON objects checked against
/// [`SUBSCRIPTION_ROUTES`](crate::SUBSCRIPTION_ROUTES) before anything is
/// sent: an unknown field or filter, a missing required field, a value
/// outside its set or a missing `Idempotency-Key` is `Error::Invalid`.
/// Creating a subscription needs an `Idempotency-Key` (reuse it when
/// retrying the same subscription; it is sent in the header and the body);
/// plan, customer, card, plan-change and charge-retry writes accept one.
///
/// Amounts are canonical minor-unit text (`"9900"` = 99.00 TRY; `"0"` for a
/// free plan), checked in bodies, filters and answers. Entity calls return
/// the envelope's `data`; subscription, plan, customer, card, invoice and
/// audit-log lists return a [`CursorPage`] with `page_size` (send
/// `next_cursor` back as `pageToken`). The related resources hang off
/// `plans()`, `customers()`, `cards()`, `invoices()`, `portal()`,
/// `products()`, `features()`, `addons()` and `usage()`.
pub struct Subscriptions<'a> {
    client: &'a Client,
}
impl Client {
    /// Returns the subscriptions facade; no independent HTTP client is constructed.
    pub fn subscriptions(&self) -> Subscriptions<'_> {
        Subscriptions { client: self }
    }
}

macro_rules! sub_facades {
    ($($(#[$doc:meta])* $name:ident => $accessor:ident;)*) => {
        $(
            $(#[$doc])*
            pub struct $name<'a> {
                client: &'a Client,
            }
        )*
        impl<'a> Subscriptions<'a> {
            $(
                $(#[$doc])*
                pub fn $accessor(&self) -> $name<'a> {
                    $name { client: self.client }
                }
            )*
        }
    };
}

sub_facades! {
    /// Plans: the templates a subscription is created against.
    SubscriptionPlans => plans;
    /// Customers, upserted by your own `externalId` (one stable customer per user).
    SubscriptionCustomers => customers;
    /// Saved cards for automatic charges. A card is added from the checkout
    /// SDK's `paymentMethodToken`; a card number or security code is never
    /// sent (the fields are refused, and the refusal names no value).
    SubscriptionCards => cards;
    /// Invoices: billing history, void and charge retry.
    SubscriptionInvoices => invoices;
    /// Customer portal: embedded SDK sessions and hosted portal links.
    SubscriptionPortal => portal;
    /// Catalog products (groups of plans).
    SubscriptionProducts => products;
    /// Entitlement features.
    SubscriptionFeatures => features;
    /// Add-ons.
    SubscriptionAddons => addons;
    /// Metered usage and entitlement checks.
    SubscriptionUsage => usage;
}

async fn call(client: &Client, name: &str, call: RouteCall<'_>) -> Result<Response> {
    send_route(client, SUBSCRIPTION_ROUTES, name, call).await
}

async fn entity(client: &Client, name: &str, call: RouteCall<'_>) -> Result<Value> {
    Ok(data(&self::call(client, name, call).await?))
}

async fn page(client: &Client, name: &str, call: RouteCall<'_>) -> Result<CursorPage> {
    Ok(list_page(&self::call(client, name, call).await?))
}

/// Subscription lists carry `nextPageToken` and `pageSize` at the envelope root.
async fn cursor_page(client: &Client, name: &str, call: RouteCall<'_>) -> Result<CursorPage> {
    let response = self::call(client, name, call).await?;
    let mut page = list_page(&response);
    let size = response
        .envelope
        .as_ref()
        .and_then(|envelope| envelope.get("pageSize"))
        .and_then(Value::as_i64);
    page.page_size = Some(size.unwrap_or(i64::try_from(page.items.len()).unwrap_or(i64::MAX)));
    Ok(page)
}

fn changes(value: &Value) -> Result<&Value> {
    object_of(value, "subscription request body must be a JSON object")?;
    Ok(value)
}

fn reason_body(reason: Option<&str>) -> Value {
    json!({ "reason": optional_text(reason) })
}

impl Subscriptions<'_> {
    /// Creates a subscription (`POST /subscriptions`): `customerId`,
    /// `planId`, `paymentCardId?`, `paymentMethodId?`, `collectionMethod?`
    /// ([`subscription_collection_method`](crate::subscription_collection_method)),
    /// `startsAt?`, `chargeOnCreate?`, `skipTrial?`, `metadata?` …. The
    /// `Idempotency-Key` is required: the argument or the request's own
    /// `idempotencyKey` (both given, they must agree); it is also sent in
    /// the body.
    pub async fn create(&self, request: &Value, idempotency_key: Option<&str>) -> Result<Value> {
        let mut body =
            object_or_empty(request, "subscription create request must be a JSON object")?;
        let key = idempotency_key_of(&body, idempotency_key)?;
        if let Some(key) = &key {
            body.insert("idempotencyKey".to_owned(), Value::from(key.as_str()));
        }
        check_member(
            &body,
            "collectionMethod",
            subscription_collection_method::ALL,
            "collectionMethod must be AUTO_CHARGE, SEND_INVOICE or MANUAL",
        )?;
        for field in ["chargeOnCreate", "skipTrial"] {
            boolean_member(&body, field, "chargeOnCreate / skipTrial must be booleans")?;
        }
        object_member(&body, "metadata", "metadata must be a JSON object")?;
        let body = Value::Object(body);
        let request = RouteCall {
            body: Some(&body),
            key: key.as_deref(),
            ..RouteCall::default()
        };
        entity(self.client, "create", request).await
    }
    /// One subscription (`GET /subscriptions/{subscriptionId}`).
    pub async fn get(&self, subscription_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[subscription_id],
            ..RouteCall::default()
        };
        entity(self.client, "get", request).await
    }
    /// Subscriptions; filters `customerId`, `status`, `pageSize`, `pageToken`.
    pub async fn list(&self, filters: &Value) -> Result<CursorPage> {
        let request = RouteCall {
            query: Some(filters),
            ..RouteCall::default()
        };
        cursor_page(self.client, "list", request).await
    }
    /// Cancels now, or at the period end with `at_period_end`.
    pub async fn cancel(
        &self,
        subscription_id: &str,
        at_period_end: bool,
        reason: Option<&str>,
    ) -> Result<Value> {
        let body = json!({ "cancelAtPeriodEnd": at_period_end, "reason": optional_text(reason) });
        let request = RouteCall {
            params: &[subscription_id],
            body: Some(&body),
            ..RouteCall::default()
        };
        entity(self.client, "cancel", request).await
    }
    /// Pauses billing; `resume_at` (ISO-8601) resumes it automatically.
    pub async fn pause(&self, subscription_id: &str, resume_at: Option<&str>) -> Result<Value> {
        let body = json!({ "resumeAt": optional_text(resume_at) });
        let request = RouteCall {
            params: &[subscription_id],
            body: Some(&body),
            ..RouteCall::default()
        };
        entity(self.client, "pause", request).await
    }
    /// Resumes a paused subscription.
    pub async fn resume(&self, subscription_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[subscription_id],
            ..RouteCall::default()
        };
        entity(self.client, "resume", request).await
    }
    /// Changes the plan: `newPlanId`, `behavior?` (`IMMEDIATE` /
    /// `NEXT_PERIOD`, case-insensitive), `reason?`. An `Idempotency-Key`
    /// (argument or the request's own `idempotencyKey`) is sent in the
    /// header and the body.
    pub async fn change_plan(
        &self,
        subscription_id: &str,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<Value> {
        let mut body = object_or_empty(request, "changePlan request must be a JSON object")?;
        let key = idempotency_key_of(&body, idempotency_key)?;
        if let Some(key) = &key {
            body.insert("idempotencyKey".to_owned(), Value::from(key.as_str()));
        }
        if let Some(behavior) = body.get("behavior").filter(|value| !value.is_null()) {
            let upper = behavior
                .as_str()
                .map(str::to_ascii_uppercase)
                .filter(|value| subscription_plan_change_behavior::ALL.contains(&value.as_str()))
                .ok_or(Error::Invalid("behavior must be IMMEDIATE or NEXT_PERIOD"))?;
            body.insert("behavior".to_owned(), Value::String(upper));
        }
        let body = Value::Object(body);
        let request = RouteCall {
            params: &[subscription_id],
            body: Some(&body),
            key: key.as_deref(),
            ..RouteCall::default()
        };
        entity(self.client, "changePlan", request).await
    }
    /// Audit trail of a subscription; filters `limit`, `pageToken`.
    pub async fn audit_logs(&self, subscription_id: &str, filters: &Value) -> Result<CursorPage> {
        let request = RouteCall {
            params: &[subscription_id],
            query: Some(filters),
            ..RouteCall::default()
        };
        cursor_page(self.client, "auditLogs", request).await
    }
}

impl SubscriptionPlans<'_> {
    /// Creates a plan: `productId`, `code`, `name`, `price`, `currency`,
    /// `billing`, `description?`, `trialPeriodDays?`, `maxDunningAttempts?`,
    /// `dunningScheduleDays?`, `gracePeriodDays?`, `metadata?`. `price` is
    /// minor-unit text (`"0"` for a free plan); `currency` a supported ISO
    /// code; `billing` is `{frequency, interval?, byMonthDay?, byWeekDay?,
    /// timezone?, lastDayOfMonthFix?, count?}` (`interval` defaults to 1).
    pub async fn create(&self, request: &Value, idempotency_key: Option<&str>) -> Result<Value> {
        let mut body = object_or_empty(request, "plans.create request must be a JSON object")?;
        for field in ["productId", "code", "name"] {
            if let Some(value) = body.get(field).filter(|value| !value.is_null()) {
                let text = value
                    .as_str()
                    .ok_or(Error::Invalid("productId, code and name must be text"))?;
                let text = require_text(text, "productId, code and name must not be empty")?;
                body.insert(field.to_owned(), Value::String(text));
            }
        }
        if body.get("price").is_none_or(Value::is_null) {
            return Err(Error::Invalid(
                "plans.create requires price (minor-unit text, \"0\" for a free plan)",
            ));
        }
        if let Some(currency) = body.get("currency").filter(|value| !value.is_null()) {
            let currency = currency
                .as_str()
                .and_then(crate::money::supported_currency)
                .ok_or(Error::Invalid("currency must be a supported ISO 4217 code"))?;
            body.insert("currency".to_owned(), Value::String(currency));
        }
        if let Some(billing) = body.get("billing").filter(|value| !value.is_null()) {
            let rule = billing_rule(billing)?;
            body.insert("billing".to_owned(), rule);
        }
        for field in ["trialPeriodDays", "maxDunningAttempts", "gracePeriodDays"] {
            count_member(
                &body,
                field,
                0,
                "trialPeriodDays, maxDunningAttempts and gracePeriodDays must be integers of at least 0",
            )?;
        }
        if let Some(days) = body
            .get("dunningScheduleDays")
            .filter(|value| !value.is_null())
        {
            integer_list(
                days,
                0,
                crate::MAX_SAFE_INTEGER,
                "dunningScheduleDays must be an array of integers of at least 0",
            )?;
        }
        let body = Value::Object(body);
        let request = RouteCall {
            body: Some(&body),
            key: idempotency_key,
            ..RouteCall::default()
        };
        entity(self.client, "plans.create", request).await
    }
    /// One plan.
    pub async fn get(&self, plan_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[plan_id],
            ..RouteCall::default()
        };
        entity(self.client, "plans.get", request).await
    }
    /// Plans; filters `productId`, `includeArchived`, `pageSize`, `pageToken`.
    pub async fn list(&self, filters: &Value) -> Result<CursorPage> {
        let request = RouteCall {
            query: Some(filters),
            ..RouteCall::default()
        };
        cursor_page(self.client, "plans.list", request).await
    }
    /// Price, billing, metadata …; a `price` is minor-unit text.
    pub async fn update(&self, plan_id: &str, changes: &Value) -> Result<Value> {
        let request = RouteCall {
            params: &[plan_id],
            body: Some(self::changes(changes)?),
            ..RouteCall::default()
        };
        entity(self.client, "plans.update", request).await
    }
    /// Archives a plan (`DELETE`).
    pub async fn archive(&self, plan_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[plan_id],
            ..RouteCall::default()
        };
        entity(self.client, "plans.archive", request).await
    }
    /// The plan's translations.
    pub async fn translations(&self, plan_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[plan_id],
            ..RouteCall::default()
        };
        entity(self.client, "plans.translations", request).await
    }
    /// Creates or replaces the plan's translation for `locale`.
    pub async fn upsert_translation(
        &self,
        plan_id: &str,
        locale: &str,
        translation: &Value,
    ) -> Result<Value> {
        let request = RouteCall {
            params: &[plan_id, locale],
            body: Some(changes(translation)?),
            ..RouteCall::default()
        };
        entity(self.client, "plans.upsertTranslation", request).await
    }
}

impl SubscriptionCustomers<'_> {
    /// Creates or updates a customer by `externalId`. Renewals take the buyer
    /// from this record, so `name`, `surname`, `email`, `gsmNumber`,
    /// `identityNumber`, `billingAddress`, `billingCity`, `billingCountry` and
    /// `billingZipCode` are required; `billingContactName` and the
    /// `shipping*` fields are optional. A missing field fails before sending;
    /// the gateway answers an invalid one with
    /// [`subscription_customer_error_code`](crate::subscription_customer_error_code).
    pub async fn upsert(&self, request: &Value, idempotency_key: Option<&str>) -> Result<Value> {
        let request = RouteCall {
            body: Some(request),
            key: idempotency_key,
            ..RouteCall::default()
        };
        entity(self.client, "customers.upsert", request).await
    }
    /// One customer.
    pub async fn get(&self, customer_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[customer_id],
            ..RouteCall::default()
        };
        entity(self.client, "customers.get", request).await
    }
    /// The customer with your own `externalId`.
    pub async fn get_by_external_id(&self, external_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[external_id],
            ..RouteCall::default()
        };
        entity(self.client, "customers.getByExternalId", request).await
    }
    /// Customers; filters `search`, `includeArchived`, `pageSize`, `pageToken`.
    pub async fn list(&self, filters: &Value) -> Result<CursorPage> {
        let request = RouteCall {
            query: Some(filters),
            ..RouteCall::default()
        };
        cursor_page(self.client, "customers.list", request).await
    }
    /// The customer's credit balance.
    pub async fn credit_balance(&self, customer_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[customer_id],
            ..RouteCall::default()
        };
        entity(self.client, "customers.creditBalance", request).await
    }
    /// Grants or deducts credit: `{amountMinor, currency, reason, type, …}`
    /// (`amountMinor` minor-unit text).
    pub async fn adjust_credit(&self, customer_id: &str, adjustment: &Value) -> Result<Value> {
        let request = RouteCall {
            params: &[customer_id],
            body: Some(changes(adjustment)?),
            ..RouteCall::default()
        };
        entity(self.client, "customers.adjustCredit", request).await
    }
    /// Archives a customer (`DELETE`).
    pub async fn archive(&self, customer_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[customer_id],
            ..RouteCall::default()
        };
        entity(self.client, "customers.archive", request).await
    }
}

impl SubscriptionCards<'_> {
    /// Adds a saved card: `customerId`, `paymentMethodToken`, `brand?`,
    /// `last4?`, `bin?`, `expireMonth?`, `expireYear?`, `cardholderName?`,
    /// `isDefault?`, `nickname?`, `verificationMode?`
    /// ([`subscription_card_verification_mode`](crate::subscription_card_verification_mode)),
    /// `supportedCurrencies?`. A card number or security code field is refused.
    pub async fn add(&self, request: &Value, idempotency_key: Option<&str>) -> Result<Value> {
        let mut body = object_or_empty(request, "cards.add request must be a JSON object")?;
        check_member(
            &body,
            "verificationMode",
            subscription_card_verification_mode::ALL,
            "verificationMode must be NONE, ON_ADD or FIRST_CHARGE",
        )?;
        for field in ["expireMonth", "expireYear"] {
            count_member(
                &body,
                field,
                1,
                "expireMonth / expireYear must be positive integers",
            )?;
        }
        boolean_member(&body, "isDefault", "isDefault must be a boolean")?;
        if let Some(currencies) = body
            .get("supportedCurrencies")
            .filter(|value| !value.is_null())
        {
            let list = currencies
                .as_array()
                .ok_or(Error::Invalid("supportedCurrencies must be an array"))?
                .iter()
                .map(|currency| {
                    currency
                        .as_str()
                        .and_then(crate::money::supported_currency)
                        .map(Value::String)
                        .ok_or(Error::Invalid(
                            "supportedCurrencies must hold supported ISO 4217 codes",
                        ))
                })
                .collect::<Result<Vec<_>>>()?;
            body.insert("supportedCurrencies".to_owned(), Value::Array(list));
        }
        let body = Value::Object(body);
        let request = RouteCall {
            body: Some(&body),
            key: idempotency_key,
            ..RouteCall::default()
        };
        entity(self.client, "cards.add", request).await
    }
    /// One saved card.
    pub async fn get(&self, card_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[card_id],
            ..RouteCall::default()
        };
        entity(self.client, "cards.get", request).await
    }
    /// A customer's cards; filters `includeInactive` (default `false`),
    /// `pageSize`, `pageToken`.
    pub async fn list(&self, customer_id: &str, filters: &Value) -> Result<CursorPage> {
        let mut query: Map<String, Value> =
            object_or_empty(filters, "filters must be a JSON object")?;
        query.insert(
            "customerId".to_owned(),
            Value::String(require_text(customer_id, "customerId is required")?),
        );
        if query.get("includeInactive").is_none_or(Value::is_null) {
            query.insert("includeInactive".to_owned(), Value::Bool(false));
        }
        let query = Value::Object(query);
        let request = RouteCall {
            query: Some(&query),
            ..RouteCall::default()
        };
        cursor_page(self.client, "cards.list", request).await
    }
    /// Makes the card the customer's default.
    pub async fn set_default(&self, card_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[card_id],
            ..RouteCall::default()
        };
        entity(self.client, "cards.setDefault", request).await
    }
    /// Deactivates a saved card (`DELETE`).
    pub async fn deactivate(&self, card_id: &str, reason: Option<&str>) -> Result<Value> {
        let body = reason_body(reason);
        let request = RouteCall {
            params: &[card_id],
            body: Some(&body),
            ..RouteCall::default()
        };
        entity(self.client, "cards.deactivate", request).await
    }
    /// Verifies a saved card (a small authorisation or a 3-D check);
    /// `options` as the gateway documents them (`Value::Null` for none).
    pub async fn verify(&self, card_id: &str, options: &Value) -> Result<Value> {
        let request = RouteCall {
            params: &[card_id],
            body: Some(options),
            ..RouteCall::default()
        };
        entity(self.client, "cards.verify", request).await
    }
    /// Card metadata (label, expiry, billing …); never a card number.
    pub async fn update(&self, card_id: &str, changes: &Value) -> Result<Value> {
        let request = RouteCall {
            params: &[card_id],
            body: Some(self::changes(changes)?),
            ..RouteCall::default()
        };
        entity(self.client, "cards.update", request).await
    }
}

impl SubscriptionInvoices<'_> {
    /// Invoices; filters `subscriptionId`, `status`, `pageSize`, `pageToken`.
    pub async fn list(&self, filters: &Value) -> Result<CursorPage> {
        let request = RouteCall {
            query: Some(filters),
            ..RouteCall::default()
        };
        cursor_page(self.client, "invoices.list", request).await
    }
    /// One invoice.
    pub async fn get(&self, invoice_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[invoice_id],
            ..RouteCall::default()
        };
        entity(self.client, "invoices.get", request).await
    }
    /// Voids an open invoice.
    pub async fn void(&self, invoice_id: &str, reason: Option<&str>) -> Result<Value> {
        let body = reason_body(reason);
        let request = RouteCall {
            params: &[invoice_id],
            body: Some(&body),
            ..RouteCall::default()
        };
        entity(self.client, "invoices.void", request).await
    }
    /// Retries the invoice's charge (an `Idempotency-Key` is accepted).
    pub async fn retry_charge(
        &self,
        invoice_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<Value> {
        let request = RouteCall {
            params: &[invoice_id],
            key: idempotency_key,
            ..RouteCall::default()
        };
        entity(self.client, "invoices.retryCharge", request).await
    }
}

impl SubscriptionPortal<'_> {
    /// Mints an origin-bound session for the embedded browser portal SDK
    /// (`origin` must be allow-listed). Returns `{portalToken, sessionProof,
    /// proofNonce, proofExpiresAt, …}` (the gateway answers at the envelope
    /// root; `nonce` / `expiresAt` are copied to the `proof*` names when
    /// those are missing); hand those four to the browser.
    pub async fn create_sdk_session(
        &self,
        customer_id: &str,
        origin: &str,
        sdk_mode: Option<&str>,
        portal_link_id: Option<&str>,
    ) -> Result<Value> {
        let sdk_mode = sdk_mode.filter(|mode| !mode.is_empty());
        if sdk_mode.is_some_and(|mode| !subscription_portal_sdk_mode::ALL.contains(&mode)) {
            return Err(Error::Invalid("sdkMode must be FULL, HEADLESS or HYBRID"));
        }
        let body = json!({
            "customerId": require_text(customer_id, "customerId is required")?,
            "origin": require_text(origin, "origin is required")?,
            "sdkMode": sdk_mode,
            "portalLinkId": optional_text(portal_link_id),
        });
        let request = RouteCall {
            body: Some(&body),
            ..RouteCall::default()
        };
        let response = call(self.client, "portal.createSdkSession", request).await?;
        let mut session = merge_root(&response);
        if let Value::Object(map) = &mut session {
            for (proof, plain) in [("proofNonce", "nonce"), ("proofExpiresAt", "expiresAt")] {
                if !map.contains_key(proof)
                    && let Some(value) = map.get(plain).cloned()
                {
                    map.insert(proof.to_owned(), value);
                }
            }
        }
        Ok(session)
    }
    /// A hosted portal link for the customer; `options`: `allowedPlanIds`,
    /// `defaultPlanId`, `permissions`, `sessionDurationMinutes`,
    /// `expiresAt`, `requireEmailVerification` …. The answer's `token`
    /// opens the hosted portal ([`subscription_portal_url`](crate::subscription_portal_url));
    /// the customer verifies with a one-time code.
    pub async fn create_portal_link(&self, customer_id: &str, options: &Value) -> Result<Value> {
        let mut body = object_or_empty(options, "createPortalLink options must be a JSON object")?;
        body.insert(
            "customerId".to_owned(),
            Value::String(require_text(customer_id, "customerId is required")?),
        );
        let body = Value::Object(body);
        let request = RouteCall {
            body: Some(&body),
            ..RouteCall::default()
        };
        entity(self.client, "portal.createPortalLink", request).await
    }
    /// Portal links; filters `customerId`, `status`, `limit`.
    pub async fn list_links(&self, filters: &Value) -> Result<CursorPage> {
        let request = RouteCall {
            query: Some(filters),
            ..RouteCall::default()
        };
        page(self.client, "portal.listLinks", request).await
    }
    /// One portal link.
    pub async fn get_link(&self, link_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[link_id],
            ..RouteCall::default()
        };
        entity(self.client, "portal.getLink", request).await
    }
    /// Changes a portal link.
    pub async fn update_link(&self, link_id: &str, changes: &Value) -> Result<Value> {
        let request = RouteCall {
            params: &[link_id],
            body: Some(self::changes(changes)?),
            ..RouteCall::default()
        };
        entity(self.client, "portal.updateLink", request).await
    }
    /// Revokes a portal link (`DELETE`).
    pub async fn revoke_link(&self, link_id: &str, reason: Option<&str>) -> Result<Value> {
        let body = reason_body(reason);
        let request = RouteCall {
            params: &[link_id],
            body: Some(&body),
            ..RouteCall::default()
        };
        entity(self.client, "portal.revokeLink", request).await
    }
    /// What happened on a portal link.
    pub async fn link_activity(&self, link_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[link_id],
            ..RouteCall::default()
        };
        entity(self.client, "portal.linkActivity", request).await
    }
    /// Emails a portal invite: `{customerId, email?, allowedPlanIds?, …}`.
    pub async fn send_invite(&self, invite: &Value) -> Result<Value> {
        let request = RouteCall {
            body: Some(changes(invite)?),
            ..RouteCall::default()
        };
        entity(self.client, "portal.sendInvite", request).await
    }
    /// Portal settings (branding, allowed origins).
    pub async fn get_settings(&self) -> Result<Value> {
        entity(self.client, "portal.getSettings", RouteCall::default()).await
    }
    /// Replaces the portal settings.
    pub async fn update_settings(&self, settings: &Value) -> Result<Value> {
        let request = RouteCall {
            body: Some(changes(settings)?),
            ..RouteCall::default()
        };
        entity(self.client, "portal.updateSettings", request).await
    }
}

macro_rules! catalog_resource {
    ($facade:ident, $prefix:literal, $id:ident) => {
        impl $facade<'_> {
            /// Creates one (the gateway validates the body).
            pub async fn create(&self, body: &Value) -> Result<Value> {
                let request = RouteCall {
                    body: Some(changes(body)?),
                    ..RouteCall::default()
                };
                entity(self.client, concat!($prefix, ".create"), request).await
            }
            /// One by id.
            pub async fn get(&self, $id: &str) -> Result<Value> {
                let request = RouteCall {
                    params: &[$id],
                    ..RouteCall::default()
                };
                entity(self.client, concat!($prefix, ".get"), request).await
            }
            /// Changes one.
            pub async fn update(&self, $id: &str, changes: &Value) -> Result<Value> {
                let request = RouteCall {
                    params: &[$id],
                    body: Some(self::changes(changes)?),
                    ..RouteCall::default()
                };
                entity(self.client, concat!($prefix, ".update"), request).await
            }
            /// Its translations.
            pub async fn translations(&self, $id: &str) -> Result<Value> {
                let request = RouteCall {
                    params: &[$id],
                    ..RouteCall::default()
                };
                entity(self.client, concat!($prefix, ".translations"), request).await
            }
            /// Creates or replaces its translation for `locale`.
            pub async fn upsert_translation(
                &self,
                $id: &str,
                locale: &str,
                translation: &Value,
            ) -> Result<Value> {
                let request = RouteCall {
                    params: &[$id, locale],
                    body: Some(changes(translation)?),
                    ..RouteCall::default()
                };
                entity(self.client, concat!($prefix, ".upsertTranslation"), request).await
            }
        }
    };
}

catalog_resource!(SubscriptionProducts, "products", product_id);
catalog_resource!(SubscriptionFeatures, "features", feature_id);

impl SubscriptionProducts<'_> {
    /// Catalog products; filters `limit`, `cursor`, `active`.
    pub async fn list(&self, filters: &Value) -> Result<CursorPage> {
        let request = RouteCall {
            query: Some(filters),
            ..RouteCall::default()
        };
        page(self.client, "products.list", request).await
    }
    /// Archives a product (`DELETE`).
    pub async fn archive(&self, product_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[product_id],
            ..RouteCall::default()
        };
        entity(self.client, "products.archive", request).await
    }
}

impl SubscriptionFeatures<'_> {
    /// Features; any scalar filter is passed on.
    pub async fn list(&self, filters: &Value) -> Result<CursorPage> {
        let request = RouteCall {
            query: Some(filters),
            ..RouteCall::default()
        };
        page(self.client, "features.list", request).await
    }
    /// Deletes a feature.
    pub async fn delete(&self, feature_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[feature_id],
            ..RouteCall::default()
        };
        entity(self.client, "features.delete", request).await
    }
}

impl SubscriptionAddons<'_> {
    /// Creates an add-on (the gateway validates the body).
    pub async fn create(&self, addon: &Value) -> Result<Value> {
        let request = RouteCall {
            body: Some(changes(addon)?),
            ..RouteCall::default()
        };
        entity(self.client, "addons.create", request).await
    }
    /// Add-ons; any scalar filter is passed on.
    pub async fn list(&self, filters: &Value) -> Result<CursorPage> {
        let request = RouteCall {
            query: Some(filters),
            ..RouteCall::default()
        };
        page(self.client, "addons.list", request).await
    }
    /// Changes an add-on.
    pub async fn update(&self, addon_id: &str, changes: &Value) -> Result<Value> {
        let request = RouteCall {
            params: &[addon_id],
            body: Some(self::changes(changes)?),
            ..RouteCall::default()
        };
        entity(self.client, "addons.update", request).await
    }
    /// Deactivates an add-on (`DELETE`).
    pub async fn deactivate(&self, addon_id: &str) -> Result<Value> {
        let request = RouteCall {
            params: &[addon_id],
            ..RouteCall::default()
        };
        entity(self.client, "addons.deactivate", request).await
    }
}

impl SubscriptionUsage<'_> {
    /// A metered-usage event: `{customerId | subscriptionId, featureKey,
    /// quantity, timestamp?, idempotencyKey?, …}`.
    pub async fn record(&self, usage: &Value) -> Result<Value> {
        let request = RouteCall {
            body: Some(changes(usage)?),
            ..RouteCall::default()
        };
        entity(self.client, "usage.record", request).await
    }
    /// Whether the customer is entitled / has quota left:
    /// `{customerId | subscriptionId, featureKey, requested?, …}`.
    pub async fn check_entitlement(&self, check: &Value) -> Result<Value> {
        let request = RouteCall {
            body: Some(changes(check)?),
            ..RouteCall::default()
        };
        entity(self.client, "usage.checkEntitlement", request).await
    }
}

/// A plan's `billing` rule: known fields only, `frequency` upper-cased and
/// checked, `interval` defaulting to 1.
fn billing_rule(value: &Value) -> Result<Value> {
    let rule = crate::route_request::compact(object_of(value, "billing must be a JSON object")?);
    if rule
        .keys()
        .any(|key| !SUBSCRIPTION_BILLING_RULE_FIELDS.contains(&key.as_str()))
    {
        return Err(Error::Invalid(
            "billing field not accepted (frequency, interval, byMonthDay, byWeekDay, timezone, lastDayOfMonthFix, count)",
        ));
    }
    let mut out = rule;
    let frequency = out
        .get("frequency")
        .and_then(Value::as_str)
        .map(str::to_ascii_uppercase)
        .filter(|value| subscription_billing_frequency::ALL.contains(&value.as_str()))
        .ok_or(Error::Invalid(
            "billing.frequency must be DAILY, WEEKLY, MONTHLY or YEARLY",
        ))?;
    out.insert("frequency".to_owned(), Value::String(frequency));
    if !out.contains_key("interval") {
        out.insert("interval".to_owned(), Value::from(1));
    }
    count_member(
        &out,
        "interval",
        1,
        "billing.interval must be an integer of at least 1",
    )?;
    count_member(
        &out,
        "count",
        0,
        "billing.count must be an integer of at least 0",
    )?;
    if let Some(days) = out.get("byMonthDay") {
        integer_list(
            days,
            1,
            31,
            "billing.byMonthDay must be an array of integers from 1 to 31",
        )?;
    }
    if let Some(days) = out.get("byWeekDay")
        && !days
            .as_array()
            .is_some_and(|list| list.iter().all(Value::is_string))
    {
        return Err(Error::Invalid(
            "billing.byWeekDay must be an array of week days (MO, TU, WE, TH, FR, SA, SU)",
        ));
    }
    boolean_member(
        &out,
        "lastDayOfMonthFix",
        "billing.lastDayOfMonthFix must be a boolean",
    )?;
    Ok(Value::Object(out))
}
