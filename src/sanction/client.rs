//! `client.sanction()`: sanctions screening, ongoing monitoring, entity
//! intelligence and the deny list.

use super::values::{SANCTION_NESTED_FIELDS, SANCTION_ROUTES, SANCTION_VALUE_SETS};
use crate::{
    Client, Error, Request, Response, Result,
    client::error_code,
    route_table::{
        CursorPage, Route, check_fields, fill_path, find, object_of, present, query_pairs, to_page,
        unwrap,
    },
};
use serde_json::{Map, Value, json};

/// Sanctions screening — `/api/v1/sanction/*` (`client.sanction()`). The
/// key needs the `SANCTION_SERVICE` product and `sanction:view_results`.
///
/// Screening returns matches, not verdicts: the decision is yours. Read a
/// result with [`is_clear_screening`](crate::is_clear_screening): an empty
/// match list is clear only when `coverageStatus` is `COMPLETE`. When the
/// screening cannot run the gateway answers 503 (`Error::Api`); a 2xx answer
/// whose `status` is not `success` is `Error::Api` as well, never an empty
/// result.
///
/// Bodies and filters are JSON objects checked against
/// [`SANCTION_ROUTES`](crate::SANCTION_ROUTES),
/// [`SANCTION_NESTED_FIELDS`](crate::SANCTION_NESTED_FIELDS) and
/// [`SANCTION_VALUE_SETS`](crate::SANCTION_VALUE_SETS) before sending: an
/// unknown field, a missing required field, an empty `any_of` group, a bad
/// nested row or a value outside its set is `Error::Invalid` (the service
/// would silently drop an unknown field). Answers are the envelope's `data`
/// (the screening's own id is `screeningRequestId`); lists are
/// [`CursorPage`].
pub struct Sanction<'a> {
    client: &'a Client,
}
impl Client {
    /// Returns the sanction facade; no independent HTTP client is constructed.
    pub fn sanction(&self) -> Sanction<'_> {
        Sanction { client: self }
    }
}

impl Sanction<'_> {
    /// Person or organization (`POST /screening/name`): `name`, `subjectType?`,
    /// `country?`, `dateOfBirth?`, `identifiers?` (`[{type, value}]`),
    /// `riskDomains?`, `mode?`, `threshold?`, `customerReference?`.
    pub async fn screen_name(&self, request: &Value) -> Result<Value> {
        self.screen("screenName", request).await
    }
    /// Crypto wallet address (`POST /screening/wallet`).
    pub async fn screen_wallet(&self, request: &Value) -> Result<Value> {
        self.screen("screenWallet", request).await
    }
    /// Bank account / IBAN against your own deny list and account risk data
    /// (`POST /screening/account`; no coverage reported).
    pub async fn screen_account(&self, request: &Value) -> Result<Value> {
        self.screen("screenAccount", request).await
    }
    /// Every party and account of a transaction (`POST /screening/transaction`;
    /// at least one of `parties` or `accounts`).
    pub async fn screen_transaction(&self, request: &Value) -> Result<Value> {
        self.screen("screenTransaction", request).await
    }
    /// Public procurement debarment lists by name and TCKN / VKN (at least
    /// one). A list that is not published makes `coverageStatus` `PARTIAL`
    /// (`uncoveredSources` names it): no match is not a clean result for it.
    pub async fn screen_public_procurement_ban(&self, request: &Value) -> Result<Value> {
        self.screen("screenPublicProcurementBan", request).await
    }
    /// Adverse media; runs only when your adverse-media policy is enabled.
    pub async fn screen_adverse_media(&self, request: &Value) -> Result<Value> {
        self.screen("screenAdverseMedia", request).await
    }
    /// Behavioural risk signal evaluation.
    pub async fn evaluate_behavioral_alert(&self, request: &Value) -> Result<Value> {
        self.screen("evaluateBehavioralAlert", request).await
    }
    /// Earlier screenings, newest first (`limit`, `cursor`); `raw` carries
    /// the summary (`runsToday`, `reviewDecisions`, `blockedCount`, `avgLatencyMs`).
    pub async fn list_screening_history(&self, filters: &Value) -> Result<CursorPage> {
        self.page("listScreeningHistory", &[], filters).await
    }
    /// Registers a subject for ongoing monitoring; returns
    /// `{monitoredEntityId}`. A `profile` object (name, dateOfBirth,
    /// country, identifiers …) is sent as the `profileJson` text; pass one of
    /// the two.
    pub async fn register_monitoring(&self, request: &Value) -> Result<Value> {
        let mut body = object_of(request, "sanction request must be a JSON object")?.clone();
        if let Some(profile) = body.remove("profile").filter(|p| !p.is_null()) {
            if body.get("profileJson").is_some_and(|p| !p.is_null()) {
                return Err(Error::Invalid(
                    "registerMonitoring takes profile or profileJson, not both",
                ));
            }
            let text = serde_json::to_string(&profile)
                .map_err(|_| Error::Invalid("registerMonitoring profile"))?;
            body.insert("profileJson".to_owned(), Value::String(text));
        }
        let response = self
            .call(
                "registerMonitoring",
                &[],
                &Value::Null,
                Some(&Value::Object(body)),
            )
            .await?;
        Ok(unwrap(&response))
    }
    /// New-match alerts raised by monitoring (`status`, `limit`, `cursor`, `includeTotal`).
    pub async fn list_monitoring_alerts(&self, filters: &Value) -> Result<CursorPage> {
        self.page("listMonitoringAlerts", &[], filters).await
    }
    /// Monitored subjects (`activeOnly` defaults to `true` on the gateway).
    pub async fn list_monitored_entities(&self, filters: &Value) -> Result<CursorPage> {
        self.page("listMonitoredEntities", &[], filters).await
    }
    /// Matches raised by monitoring (`status`, `limit`, `cursor`).
    pub async fn list_monitoring_matches(&self, filters: &Value) -> Result<CursorPage> {
        self.page("listMonitoringMatches", &[], filters).await
    }
    /// Freshness of the source lists: `{manifest}`. A manifest that could not
    /// be produced (`status: "failure"`) is an error.
    pub async fn get_dataset_freshness(&self) -> Result<Value> {
        let response = self
            .call("getDatasetFreshness", &[], &Value::Null, None)
            .await?;
        screening_data(&response)
    }
    /// Entity search by name (`q`, `limit`).
    pub async fn search_entities(&self, filters: &Value) -> Result<Value> {
        let response = self.call("searchEntities", &[], filters, None).await?;
        Ok(unwrap(&response))
    }
    /// Entity risk profile; `screening_request_id` reads it on that
    /// screening's list publication.
    pub async fn get_entity_profile(
        &self,
        entity_id: &str,
        screening_request_id: Option<&str>,
    ) -> Result<Value> {
        let query = json!({ "screeningRequestId": screening_request_id });
        let response = self
            .call("getEntityProfile", &[entity_id], &query, None)
            .await?;
        Ok(unwrap(&response))
    }
    /// Relationship graph, `depth` 1–3.
    pub async fn get_entity_graph(&self, entity_id: &str, depth: Option<u8>) -> Result<Value> {
        let query = json!({ "depth": depth });
        let response = self
            .call("getEntityGraph", &[entity_id], &query, None)
            .await?;
        Ok(unwrap(&response))
    }
    /// Indirect exposure paths; `entity_ids` are the records of a merged
    /// match (sent comma-separated; empty for none).
    pub async fn get_indirect_exposure(
        &self,
        entity_id: &str,
        max_depth: Option<u8>,
        entity_ids: &[&str],
    ) -> Result<Value> {
        let query = json!({ "maxDepth": max_depth, "entityIds": id_list(entity_ids) });
        let response = self
            .call("getIndirectExposure", &[entity_id], &query, None)
            .await?;
        Ok(unwrap(&response))
    }
    /// Ownership aggregation (50% rule); `entity_ids` as in
    /// `get_indirect_exposure`.
    pub async fn get_ownership_aggregation(
        &self,
        entity_id: &str,
        depth: Option<u8>,
        entity_ids: &[&str],
    ) -> Result<Value> {
        let query = json!({ "depth": depth, "entityIds": id_list(entity_ids) });
        let response = self
            .call("getOwnershipAggregation", &[entity_id], &query, None)
            .await?;
        Ok(unwrap(&response))
    }
    /// Your own deny-list entries (read only with an API key; `limit`, `cursor`).
    pub async fn list_blacklist_entries(&self, filters: &Value) -> Result<CursorPage> {
        self.page("listBlacklistEntries", &[], filters).await
    }

    async fn screen(&self, name: &str, request: &Value) -> Result<Value> {
        let response = self.call(name, &[], &Value::Null, Some(request)).await?;
        screening_data(&response)
    }

    async fn page(&self, name: &str, params: &[&str], filters: &Value) -> Result<CursorPage> {
        Ok(to_page(&self.call(name, params, filters, None).await?))
    }

    async fn call(
        &self,
        name: &str,
        params: &[&str],
        filters: &Value,
        body: Option<&Value>,
    ) -> Result<Response> {
        let route = find(SANCTION_ROUTES, name)?;
        let mut request = Request::new(route.http_method(), fill_path(route, params)?);
        request.query = query_pairs(route.query, filters)?;
        if route.has_body() {
            request.body = Some(check_body(route, body.unwrap_or(&Value::Null))?);
        }
        self.client.send(request).await
    }
}

/// Member ids of a merged match, comma-separated (`None` for none).
fn id_list(ids: &[&str]) -> Option<String> {
    let values: Vec<&str> = ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .collect();
    (!values.is_empty()).then(|| values.join(","))
}

/// A 2xx answer that is not `status: "success"` is refused rather than
/// read: a screening that did not run must never look like an empty result.
fn screening_data(response: &Response) -> Result<Value> {
    let status = response
        .envelope
        .as_ref()
        .and_then(|envelope| envelope.get("status"))
        .and_then(Value::as_str);
    if status.is_some_and(|status| status != "success") {
        let status_code = response
            .envelope
            .as_ref()
            .and_then(|envelope| envelope.get("statusCode"))
            .and_then(Value::as_u64)
            .and_then(|code| u16::try_from(code).ok())
            .unwrap_or(response.status);
        return Err(Error::Api {
            status: status_code,
            code: error_code(response).unwrap_or_else(|| "SCREENING_NOT_COMPLETED".to_owned()),
            request_id: response.request_id.clone(),
        });
    }
    Ok(unwrap(response))
}

fn check_rows(rows: &Value, required: &[&str], optional: &[&str]) -> Result<Value> {
    let list = rows
        .as_array()
        .ok_or(Error::Invalid("sanction nested list must be an array"))?;
    let checked = list
        .iter()
        .map(|row| {
            let object = object_of(row, "sanction nested row must be a JSON object")?;
            check_fields(object, required, optional).map(Value::Object)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Value::Array(checked))
}

fn check_values(value: &Value, allowed: &[&str]) -> Result<()> {
    let values = match value {
        Value::Array(list) => list.iter().collect::<Vec<_>>(),
        other => vec![other],
    };
    if values
        .iter()
        .all(|entry| entry.as_str().is_some_and(|text| allowed.contains(&text)))
    {
        Ok(())
    } else {
        Err(Error::Invalid(
            "sanction value outside its set (risk domain, mode or subject type)",
        ))
    }
}

/// Known fields only, required fields present, the `any_of` group not
/// empty, nested rows element by element and value sets.
fn check_body(route: &Route, body: &Value) -> Result<Value> {
    let object = object_of(body, "sanction request must be a JSON object")?;
    let mut plain: Map<String, Value> = check_fields(object, route.required, route.optional)?;
    if !route.any_of.is_empty()
        && !route
            .any_of
            .iter()
            .any(|key| plain.get(*key).is_some_and(present))
    {
        return Err(Error::Invalid(
            "sanction request needs at least one of its subject fields (any_of)",
        ));
    }
    let keys: Vec<String> = plain.keys().cloned().collect();
    for key in keys {
        let label = format!("{}.{key}", route.name);
        if let Some((_, fields)) = SANCTION_NESTED_FIELDS
            .iter()
            .find(|(name, _)| *name == label)
            && let Some(rows) = plain.get(&key)
        {
            let checked = check_rows(rows, fields.required, fields.optional)?;
            plain.insert(key.clone(), checked);
        }
        if let Some((_, allowed)) = SANCTION_VALUE_SETS.iter().find(|(name, _)| *name == label)
            && let Some(value) = plain.get(&key)
        {
            check_values(value, allowed)?;
        }
    }
    Ok(Value::Object(plain))
}
