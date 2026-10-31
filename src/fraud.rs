use crate::{
    Client, Error, Method, Request, Response, Result,
    client::{require_key, valid_id},
};
use serde_json::Value;
/// Fraud decision/event facade. Tenant identity comes from credentials.
pub struct Fraud<'a> {
    pub(crate) client: &'a Client,
}
impl Client {
    /// Returns the fraud facade sharing the same authenticated transport.
    pub fn fraud(&self) -> Fraud<'_> {
        Fraud { client: self }
    }
}
impl Fraud<'_> {
    /// Evaluates a transaction; unknown fields and tenant spoofing fail locally.
    pub async fn evaluate(&self, body: Value) -> Result<Response> {
        let object = body.as_object().ok_or(Error::Invalid("fraud object"))?;
        let fields = [
            "integrationMode",
            "productContext",
            "transactionRef",
            "channel",
            "surface",
            "stage",
            "operationKind",
            "authModeRequested",
            "authModeApplied",
            "subMerchantRef",
            "subjectType",
            "subjectId",
            "paymentRail",
            "transferDirection",
            "merchantGroupIds",
            "paymentId",
            "payment",
            "bankTransfer",
            "cryptoTransfer",
            "walletOperation",
            "billPayment",
            "loanApplication",
            "payout",
            "customer",
            "device",
            "session",
            "merchant",
            "mobileApp",
            "telemetry",
            "metadata",
            "identitySnapshot",
            "identityAssertion",
        ];
        if object.keys().any(|k| !fields.contains(&k.as_str())) {
            return Err(Error::Invalid("unknown fraud field"));
        }
        self.client
            .send(Request::new(Method::POST, "/api/v1/fraud/decisions/evaluate").json(body))
            .await
    }
    /// Ingests a stable transaction event with an idempotency key.
    pub async fn record_event(&self, body: Value, key: &str) -> Result<Response> {
        require_key(key)?;
        for field in ["eventId", "subjectType", "eventType", "occurredAt"] {
            if body
                .get(field)
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(Error::Invalid("fraud event required fields"));
            }
        }
        self.client
            .send(
                Request::new(Method::POST, "/api/v1/fraud/events/transactions")
                    .json(body)
                    .idempotency_key(key),
            )
            .await
    }
    /// Lists decisions without discarding pageInfo.
    pub async fn decisions(&self, query: Vec<(String, String)>) -> Result<Response> {
        let mut r = Request::new(Method::GET, "/api/v1/fraud/decision-logs");
        r.query = query;
        self.client.send(r).await
    }
    /// Reads one decision by its trace ID.
    pub async fn decision(&self, trace_id: &str) -> Result<Response> {
        self.client
            .send(Request::new(
                Method::GET,
                format!("/api/v1/fraud/decision-logs/{}", valid_id(trace_id)?),
            ))
            .await
    }
}
