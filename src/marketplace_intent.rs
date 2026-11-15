//! Marketplace checkout (`POST /api/v1/payments/marketplace/intents`) and
//! payment analytics (`GET /api/v1/payments/analytics/*`).

use crate::{
    Error, IntentRequest, Method, Request, Response, Result,
    client::{require_key, valid_id},
    payments::Payments,
};
use serde::Serialize;
use serde_json::{Map, Value};

/// One seller of a marketplace intent: your sub-dealer, its share of the
/// amount in canonical minor units (`amountMinor`, digits only) and an
/// optional commission `scenario` override (a JSON object). Serialized, it
/// is the same leg `client.marketplace()` takes as JSON.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MarketplaceSeller {
    pub sub_dealer_id: String,
    pub amount_minor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario: Option<Map<String, Value>>,
}
impl MarketplaceSeller {
    /// A seller and its share of the intent amount.
    pub fn new(sub_dealer_id: impl Into<String>, amount_minor: impl Into<String>) -> Self {
        Self {
            sub_dealer_id: sub_dealer_id.into(),
            amount_minor: amount_minor.into(),
            scenario: None,
        }
    }
    /// Commission scenario override of this seller: a JSON object (a
    /// string, number or array cannot be represented, so it never reaches
    /// the gateway).
    pub fn with_scenario(mut self, scenario: Map<String, Value>) -> Self {
        self.scenario = Some(scenario);
        self
    }
}

/// Body of `create_marketplace_intent`: the checkout intent fields plus
/// `sellers` and an optional `multiSellerPolicy`. The sellers' `amountMinor`
/// values must add up to the intent `amount`; any other total fails before
/// any request (the gateway refuses it with HTTP 400 and charges nothing).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MarketplaceIntentRequest {
    #[serde(flatten)]
    pub intent: IntentRequest,
    pub sellers: Vec<MarketplaceSeller>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multi_seller_policy: Option<String>,
}
impl MarketplaceIntentRequest {
    /// Creates a marketplace intent from a checkout intent and its sellers.
    pub fn new(intent: IntentRequest, sellers: Vec<MarketplaceSeller>) -> Self {
        Self {
            intent,
            sellers,
            multi_seller_policy: None,
        }
    }
    /// How the gateway treats a basket of more than one seller.
    pub fn with_multi_seller_policy(mut self, value: impl Into<String>) -> Self {
        self.multi_seller_policy = Some(value.into());
        self
    }
    fn validate(&self) -> Result<()> {
        self.intent.validate()?;
        if self.sellers.is_empty() {
            return Err(Error::Invalid("marketplace sellers"));
        }
        let mut total = 0i64;
        for seller in &self.sellers {
            if seller.sub_dealer_id.trim().is_empty() || seller.sub_dealer_id.len() > 200 {
                return Err(Error::Invalid("marketplace seller subDealerId"));
            }
            let share = crate::money::parse_minor(&seller.amount_minor, false, false)?;
            total = total
                .checked_add(share)
                .ok_or(Error::Invalid("marketplace sellers total"))?;
        }
        if total != crate::payments::money(&self.intent.amount)? {
            return Err(Error::Invalid(
                "marketplace sellers amountMinor total must equal amount",
            ));
        }
        if self
            .multi_seller_policy
            .as_ref()
            .is_some_and(|s| s.trim().is_empty())
        {
            return Err(Error::Invalid("multiSellerPolicy"));
        }
        Ok(())
    }
}

impl Payments<'_> {
    /// Creates a marketplace checkout intent
    /// (`POST /payments/marketplace/intents`) with an already persisted
    /// attempt key. Checks the intent rules and the sellers' total locally.
    pub async fn create_marketplace_intent(
        &self,
        request: &MarketplaceIntentRequest,
        key: &str,
    ) -> Result<Response> {
        request.validate()?;
        require_key(key)?;
        let body =
            serde_json::to_value(request).map_err(|_| Error::Invalid("marketplace intent JSON"))?;
        self.client
            .send(
                Request::new(Method::POST, "/api/v1/payments/marketplace/intents")
                    .json(body)
                    .idempotency_key(key),
            )
            .await
    }
    /// Volume and count of payments rescued by provider failover retries
    /// (`GET /payments/analytics/failover-recovery`), e.g. `from` / `to` filters.
    pub async fn failover_recovery_analytics(
        &self,
        filters: Vec<(String, String)>,
    ) -> Result<Response> {
        let mut request = Request::new(Method::GET, "/api/v1/payments/analytics/failover-recovery");
        request.query = analytics_query(filters)?;
        self.client.send(request).await
    }
    /// Performance of one provider credential
    /// (`GET /payments/analytics/provider-credentials/{credentialId}`).
    pub async fn provider_credential_analytics(
        &self,
        credential_id: &str,
        filters: Vec<(String, String)>,
    ) -> Result<Response> {
        let mut request = Request::new(
            Method::GET,
            format!(
                "/api/v1/payments/analytics/provider-credentials/{}",
                valid_id(credential_id)?
            ),
        );
        request.query = analytics_query(filters)?;
        self.client.send(request).await
    }
}

/// Drops empty values; a filter name must be a plain identifier.
fn analytics_query(filters: Vec<(String, String)>) -> Result<Vec<(String, String)>> {
    let mut query = vec![];
    for (name, value) in filters {
        let valid = (1..=64).contains(&name.len())
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
        if !valid {
            return Err(Error::Invalid("analytics filter name"));
        }
        if !value.is_empty() {
            query.push((name, value));
        }
    }
    Ok(query)
}
