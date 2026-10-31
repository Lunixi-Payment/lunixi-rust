use crate::{
    Client, Error, Method, Request, Response, Result,
    client::{require_key, valid_id},
};
use serde::Serialize;
use serde_json::{Value, json};

/// Largest integer represented exactly by JavaScript JSON numbers.
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
/// Buyer details sent only to the gateway. Do not log customer data.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Buyer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub surname: String,
    pub identity_number: String,
    pub email: String,
    pub gsm_number: String,
    pub ip: String,
    pub city: String,
    pub country: String,
    pub zip_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_address: Option<String>,
}
/// Billing or shipping address.
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Address {
    pub contact_name: String,
    pub address: String,
    pub city: String,
    pub country: String,
    pub zip_code: String,
}
/// Checkout line. Price is the line total in integer minor units.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BasketItem {
    pub id: String,
    pub name: String,
    pub item_type: String,
    pub price: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insurance_category: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub insurance_types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insurance_data: Option<Value>,
}
impl BasketItem {
    /// Creates a physical or virtual line. Validated when creating the intent.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        item_type: impl Into<String>,
        price: i64,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            item_type: item_type.into(),
            price,
            category1: None,
            category2: None,
            insurance_category: None,
            insurance_types: vec![],
            insurance_data: None,
        }
    }
}
/// Checkout form request. Amount excludes provider-calculated installment
/// fees. There is deliberately no raw card or removed root amount field.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentRequest {
    pub amount: i64,
    pub currency: String,
    pub order_id: String,
    pub callback_url: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub basket_items: Vec<BasketItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buyer: Option<Buyer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_address: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shipping_address: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preauth: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_user_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wallet_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_group: Option<String>,
    #[serde(rename = "force3D", skip_serializing_if = "Option::is_none")]
    pub force_3d: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<Value>,
}
impl IntentRequest {
    /// Creates a checkout form request from trusted server-side order values.
    pub fn new(
        amount: i64,
        currency: impl Into<String>,
        order_id: impl Into<String>,
        callback_url: impl Into<String>,
    ) -> Self {
        Self {
            amount,
            currency: currency.into(),
            order_id: order_id.into(),
            callback_url: callback_url.into(),
            basket_items: vec![],
            buyer: None,
            billing_address: None,
            shipping_address: None,
            metadata: None,
            preauth: None,
            locale: None,
            description: None,
            customer_id: None,
            card_user_key: None,
            payment_method: None,
            wallet_provider: None,
            payment_channel: None,
            payment_group: None,
            force_3d: None,
            settings: None,
        }
    }
    fn validate(&self) -> Result<()> {
        money(self.amount)?;
        currency(&self.currency)?;
        if self.order_id.trim().is_empty() || self.order_id.len() > 200 {
            return Err(Error::Invalid("order ID"));
        }
        let u =
            reqwest::Url::parse(&self.callback_url).map_err(|_| Error::Invalid("callback URL"))?;
        if u.scheme() != "https"
            || u.host_str().is_none()
            || !u.username().is_empty()
            || u.password().is_some()
            || u.fragment().is_some()
        {
            return Err(Error::Invalid("HTTPS callback URL"));
        }
        if !self.basket_items.is_empty() {
            let mut total = 0i64;
            for item in &self.basket_items {
                money(item.price)?;
                if item.id.trim().is_empty()
                    || item.name.trim().is_empty()
                    || !["PHYSICAL", "VIRTUAL"].contains(&item.item_type.as_str())
                {
                    return Err(Error::Invalid("basket item"));
                }
                total = total
                    .checked_add(item.price)
                    .filter(|n| *n <= MAX_SAFE_INTEGER)
                    .ok_or(Error::Invalid("basket total"))?;
            }
            if total != self.amount {
                return Err(Error::Invalid("basket total must equal amount"));
            }
        }
        Ok(())
    }
}
fn money(n: i64) -> Result<()> {
    if (1..=MAX_SAFE_INTEGER).contains(&n) {
        Ok(())
    } else {
        Err(Error::Invalid("positive integer minor-unit amount"))
    }
}
fn currency(s: &str) -> Result<()> {
    if s.len() == 3 && s.bytes().all(|c| c.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(Error::Invalid("uppercase currency"))
    }
}
/// Typed payment lifecycle facade sharing the client's transport and cache.
pub struct Payments<'a> {
    pub(crate) client: &'a Client,
}
impl Client {
    /// Returns the payment facade; no independent HTTP client is constructed.
    pub fn payments(&self) -> Payments<'_> {
        Payments { client: self }
    }
}
impl Payments<'_> {
    /// Creates an intent using an already persisted attempt idempotency key.
    pub async fn create_intent(&self, request: &IntentRequest, key: &str) -> Result<Response> {
        request.validate()?;
        require_key(key)?;
        self.client
            .send(
                Request::new(Method::POST, "/api/v1/payments/intents")
                    .json(serde_json::to_value(request).map_err(|_| Error::Invalid("intent JSON"))?)
                    .idempotency_key(key),
            )
            .await
    }
    /// Reads authoritative gateway state for reconciliation.
    pub async fn get(&self, id: &str) -> Result<Response> {
        self.client
            .send(Request::new(
                Method::GET,
                format!("/api/v1/payments/{}", valid_id(id)?),
            ))
            .await
    }
    /// Lists payments, preserving the gateway envelope and pagination metadata.
    pub async fn list(&self, query: Vec<(String, String)>) -> Result<Response> {
        let mut r = Request::new(Method::GET, "/api/v1/payments");
        r.query = query;
        self.client.send(r).await
    }
    /// Lists enabled method/provider capabilities; optional method filter.
    pub async fn methods(&self, payment_method: Option<&str>) -> Result<Response> {
        let mut r = Request::new(Method::GET, "/api/v1/payments/methods");
        if let Some(m) = payment_method {
            r.query.push(("paymentMethod".into(), m.into()))
        }
        self.client.send(r).await
    }
    /// Captures all authorized funds when amount is None, otherwise partially.
    pub async fn capture(&self, id: &str, amount: Option<i64>, key: &str) -> Result<Response> {
        self.mutation("capture", id, amount, None, key).await
    }
    /// Refunds the refundable total when amount is None, otherwise partially.
    pub async fn refund(
        &self,
        id: &str,
        amount: Option<i64>,
        reason: Option<&str>,
        key: &str,
    ) -> Result<Response> {
        self.mutation("refund", id, amount, reason, key).await
    }
    async fn mutation(
        &self,
        action: &str,
        id: &str,
        amount: Option<i64>,
        reason: Option<&str>,
        key: &str,
    ) -> Result<Response> {
        valid_id(id)?;
        require_key(key)?;
        if let Some(n) = amount {
            money(n)?
        }
        if reason.is_some_and(|s| s.len() > 500) {
            return Err(Error::Invalid("reason length"));
        }
        let mut body = json!({"paymentIntentId":id});
        if let Some(n) = amount {
            body["amount"] = json!(n)
        }
        if let Some(s) = reason {
            body["reason"] = json!(s)
        }
        self.client
            .send(
                Request::new(Method::POST, format!("/api/v1/payments/{id}/{action}"))
                    .json(body)
                    .idempotency_key(key),
            )
            .await
    }
    /// Releases an authorization; this is not a captured payment refund.
    pub async fn void(&self, id: &str, key: &str) -> Result<Response> {
        valid_id(id)?;
        require_key(key)?;
        self.client
            .send(
                Request::new(Method::POST, format!("/api/v1/payments/{id}/void"))
                    .idempotency_key(key),
            )
            .await
    }
    /// Starts/continues the documented split schema. Pending HTTP 400 is
    /// preserved as Response and never interpreted as a captured payment.
    pub async fn split(&self, body: Value, key: &str) -> Result<Response> {
        require_key(key)?;
        let total = body
            .get("totalAmount")
            .and_then(Value::as_i64)
            .ok_or(Error::Invalid("split totalAmount"))?;
        money(total)?;
        currency(
            body.get("currency")
                .and_then(Value::as_str)
                .ok_or(Error::Invalid("split currency"))?,
        )?;
        if body
            .get("ownerRef")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(Error::Invalid("split ownerRef"));
        }
        let legs = body
            .get("legs")
            .and_then(Value::as_array)
            .filter(|a| (1..=5).contains(&a.len()))
            .ok_or(Error::Invalid("split legs"))?;
        let mut sum = 0i64;
        for leg in legs {
            let n = leg
                .get("amount")
                .and_then(Value::as_i64)
                .ok_or(Error::Invalid("split leg amount"))?;
            money(n)?;
            sum = sum
                .checked_add(n)
                .filter(|n| *n <= MAX_SAFE_INTEGER)
                .ok_or(Error::Invalid("split leg sum"))?;
        }
        if sum != total {
            return Err(Error::Invalid("split amount mismatch"));
        }
        self.client
            .send(
                Request::new(Method::POST, "/api/v1/payments/split")
                    .json(body)
                    .idempotency_key(key),
            )
            .await
    }
    /// Reads split aggregate/leg state; reconcile amounts before fulfillment.
    pub async fn get_split(&self, id: &str) -> Result<Response> {
        self.client
            .send(Request::new(
                Method::GET,
                format!("/api/v1/payments/split/{}", valid_id(id)?),
            ))
            .await
    }
    /// Asks the gateway to allocate a split refund. Partial/unknown outcomes
    /// require reconciliation and must not be treated as fully refunded.
    pub async fn refund_split(
        &self,
        id: &str,
        amount: i64,
        reason: Option<&str>,
        key: &str,
    ) -> Result<Response> {
        valid_id(id)?;
        money(amount)?;
        require_key(key)?;
        if reason.is_some_and(|s| s.len() > 500) {
            return Err(Error::Invalid("reason length"));
        }
        let mut body = json!({"amount":amount});
        if let Some(s) = reason {
            body["reason"] = json!(s)
        }
        self.client
            .send(
                Request::new(Method::POST, format!("/api/v1/payments/split/{id}/refund"))
                    .json(body)
                    .idempotency_key(key),
            )
            .await
    }
}
