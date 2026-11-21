//! `client.bank_connect()`: connected bank accounts, balances and
//! transactions; `transfers()` and `fx()` for money movement.

use super::values::{
    BANK_CONNECT_BOOKED_RANGE_MAX_DAYS, BANK_CONNECT_MAX_ACCOUNT_FILTER, BANK_CONNECT_MAX_LIMIT,
    BANK_TRANSFER_DESCRIPTION_MAX, BANK_TRANSFER_IBAN_MAX, BANK_TRANSFER_RECEIVER_NAME_MAX,
    BANK_TRANSFER_RECEIVER_NAME_MIN, BANK_TRANSFER_ROUTES, bank_account_status,
    bank_connect_filters, bank_connection_status, bank_fx_order_status, bank_transaction_direction,
    bank_transaction_type, bank_transfer_status,
};
use crate::{
    Client, Error, Method, Request, Response, Result,
    route_table::{
        CursorPage, fill_path, find, minor_text, new_idempotency_key, object_of, path_segment,
        require_currency, to_page, unwrap, valid_printable_key,
    },
};
use serde_json::{Map, Value};

const BASE: &str = "/api/v1/bank-connect";
const CURSOR_MAX_LENGTH: usize = 2048;

/// Bank accounts — read your connected bank accounts, balances and
/// transactions (`/api/v1/bank-connect`, product `BANK_CONNECT`;
/// `client.bank_connect()`).
///
/// Every call is signed, reads included. Scopes: `bank-connect:view` for
/// banks, connections, accounts and `request_sync`;
/// `bank-connect:transactions:view` for balances and transactions. The key
/// decides the environment (a TEST key reads only test accounts). Connecting
/// a bank happens in the Lunixi dashboard only.
///
/// Filters are JSON objects (`Value::Null` for none) checked against
/// [`BANK_CONNECT_FILTERS`](crate::BANK_CONNECT_FILTERS) before sending: an
/// unknown filter, a value outside its set, a `limit` outside 1–100, more
/// than 100 account ids, a malformed amount, `minAmountMinor` above
/// `maxAmountMinor`, `bookedTo` without `bookedFrom` or a window over 92
/// days is `Error::Invalid`. Lists are [`CursorPage`]; other answers are the
/// envelope's `data`.
pub struct BankConnect<'a> {
    client: &'a Client,
}
impl Client {
    /// Returns the bank-connect facade; no independent HTTP client is constructed.
    pub fn bank_connect(&self) -> BankConnect<'_> {
        BankConnect { client: self }
    }
}

/// Outgoing bank transfers (`bank_connect().transfers()`), on a connection
/// whose `capabilities` include `OUTGOING_TRANSFER`. Every route needs
/// `bank-connect:transfer` and a key with an IP allowlist. With an API key
/// there is no e-mail code: `create` answers `SUBMITTED` and the transfer is
/// queued for the bank; follow it with `get` or the `bank_transfer.*`
/// webhooks. Lunixi does not check the balance: the bank decides.
pub struct BankConnectTransfers<'a> {
    client: &'a Client,
}

/// Currency exchange between two of your accounts on one connection
/// (`bank_connect().fx()`), on a connection whose `capabilities` include
/// `FX_EXCHANGE`: `list_rates` → `create_quote` → `create_order` before
/// `quote.expiresAt`. There is no webhook for exchange orders.
pub struct BankConnectFx<'a> {
    client: &'a Client,
}

impl<'a> BankConnect<'a> {
    /// Outgoing transfers (`bank-connect:transfer`).
    pub fn transfers(&self) -> BankConnectTransfers<'a> {
        BankConnectTransfers {
            client: self.client,
        }
    }
    /// Currency exchange (`bank-connect:transfer`).
    pub fn fx(&self) -> BankConnectFx<'a> {
        BankConnectFx {
            client: self.client,
        }
    }
}

impl BankConnect<'_> {
    /// The bank catalogue: whether each bank can be connected and what is
    /// read from it (`GET /banks`; `limit`, `cursor`).
    pub async fn list_banks(&self, filters: &Value) -> Result<CursorPage> {
        self.page("/banks", filters, "listBanks").await
    }
    /// Your bank connections, newest first (`GET /connections`; `status` one
    /// value or a list, `bankId`).
    pub async fn list_connections(&self, filters: &Value) -> Result<CursorPage> {
        self.page("/connections", filters, "listConnections").await
    }
    /// One connection; 404 `bank_connect.connection.not_found` for an unknown one.
    pub async fn get_connection(&self, connection_id: &str) -> Result<Value> {
        let path = format!("/connections/{}", path_segment(connection_id)?);
        Ok(unwrap(&self.read(&path, vec![]).await?))
    }
    /// Your connected accounts, newest first, each with its data `freshness`
    /// (`GET /accounts`; `connectionId`, `bankId`, `currency`, `status`).
    pub async fn list_accounts(&self, filters: &Value) -> Result<CursorPage> {
        self.page("/accounts", filters, "listAccounts").await
    }
    /// One account; 404 `bank_connect.account.not_found` for an unknown one.
    pub async fn get_account(&self, account_id: &str) -> Result<Value> {
        let path = format!("/accounts/{}", path_segment(account_id)?);
        Ok(unwrap(&self.read(&path, vec![]).await?))
    }
    /// Asks for a fresh read of the account from the bank
    /// (`POST /accounts/{accountId}/sync`, HTTP 202). It never reads earlier
    /// than the bank's minimum interval (`SCHEDULED` with `scheduledAt`).
    /// 409 `bank_connect.connection.not_active` unless the connection is
    /// `ACTIVE` or `DEGRADED`; 429 `RATE_LIMITED` when the key sent too many
    /// sync requests in the last minute.
    pub async fn request_sync(&self, account_id: &str) -> Result<Value> {
        let path = format!("{BASE}/accounts/{}/sync", path_segment(account_id)?);
        let response = self.client.send(Request::new(Method::POST, path)).await?;
        Ok(unwrap(&response))
    }
    /// The latest balance of each account (`GET /balances`; `accountId` one
    /// id or a list of at most 100, `currency`).
    pub async fn list_balances(&self, filters: &Value) -> Result<CursorPage> {
        self.page("/balances", filters, "listBalances").await
    }
    /// Transactions, newest recorded first (`GET /transactions`).
    /// `bookedFrom` (inclusive) / `bookedTo` (exclusive) filter on the bank
    /// date: ISO 8601 with a time zone, at most 92 days apart; `bookedTo`
    /// needs `bookedFrom`. `minAmountMinor` / `maxAmountMinor` are inclusive
    /// minor-unit text. `direction`, `canonicalType` (one or a list).
    pub async fn list_transactions(&self, filters: &Value) -> Result<CursorPage> {
        let query = build_query(filters, "listTransactions")?;
        check_booked_range(&query)?;
        let amount = |name: &str| {
            query
                .iter()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.parse::<i64>().ok())
        };
        if let (Some(min), Some(max)) = (amount("minAmountMinor"), amount("maxAmountMinor"))
            && min > max
        {
            return Err(Error::Invalid(
                "bank-connect minAmountMinor must not be greater than maxAmountMinor",
            ));
        }
        Ok(to_page(&self.read("/transactions", query).await?))
    }
    /// One transaction with its detail fields (`balanceAfterMinor`, `documentNo`, `bank`).
    pub async fn get_transaction(&self, transaction_id: &str) -> Result<Value> {
        let path = format!("/transactions/{}", path_segment(transaction_id)?);
        Ok(unwrap(&self.read(&path, vec![]).await?))
    }

    async fn read(&self, path: &str, query: Vec<(String, String)>) -> Result<Response> {
        let mut request = Request::new(Method::GET, format!("{BASE}{path}"));
        request.query = query;
        self.client.send(request).await
    }

    async fn page(&self, path: &str, filters: &Value, route: &str) -> Result<CursorPage> {
        let query = build_query(filters, route)?;
        Ok(to_page(&self.read(path, query).await?))
    }
}

impl BankConnectTransfers<'_> {
    /// A new `Idempotency-Key` (UUID v4). Store it with your payout first to
    /// retry safely across processes.
    pub fn new_idempotency_key() -> String {
        new_idempotency_key()
    }
    /// Payment purpose codes the bank accepts on this connection
    /// (`GET /transfer-purposes`): `{items: [{code, name, order}]}`.
    pub async fn list_purposes(&self, connection_id: &str) -> Result<Value> {
        let query = vec![("connectionId".to_owned(), id_text(connection_id)?)];
        let response =
            route_call(self.client, "listTransferPurposes", &[], query, None, None).await?;
        Ok(unwrap(&response))
    }
    /// Creates an outgoing transfer (`POST /transfers`, HTTP 201):
    /// `{transfer, stepUp, replayed}`. `request`: `accountId`,
    /// `receiverIban` (TR IBAN, at most 42 characters), `receiverName`
    /// (2–140 characters), `amountMinor` (positive minor-unit text,
    /// `"150075"` = 1,500.75 TRY), optional `currency`, `paymentPurpose`
    /// (a code from `list_purposes`) and `description` (at most 140).
    ///
    /// `idempotency_key` (8–200 printable ASCII) is a new UUID for `None`;
    /// the request is retried (`Config::max_retries`) after a network error
    /// or a 5xx with the same key and body. The same key with the same body
    /// answers the existing transfer with `replayed: true`; the key used is
    /// in `transfer.idempotencyKey`.
    pub async fn create(&self, request: &Value, idempotency_key: Option<&str>) -> Result<Value> {
        let mut body = check_body("createTransfer", request)?;
        check_transfer_body(&mut body)?;
        let key = key_for(idempotency_key)?;
        let response = route_call(
            self.client,
            "createTransfer",
            &[],
            vec![],
            Some(Value::Object(body)),
            Some(key),
        )
        .await?;
        Ok(unwrap(&response))
    }
    /// Transfers, newest first (`GET /transfers`): `accountId`, `status` (one
    /// or a list of [`bank_transfer_status`]), `createdFrom` (inclusive) /
    /// `createdTo` (exclusive), `limit` 1–100, `cursor`.
    pub async fn list(&self, filters: &Value) -> Result<CursorPage> {
        let query = transfer_query(filters, "listTransfers")?;
        let response = route_call(self.client, "listTransfers", &[], query, None, None).await?;
        Ok(to_page(&response))
    }
    /// One transfer; 404 `bank_connect.transfer.not_found` for an unknown id.
    pub async fn get(&self, transfer_id: &str) -> Result<Value> {
        let response = route_call(
            self.client,
            "getTransfer",
            &[transfer_id],
            vec![],
            None,
            None,
        )
        .await?;
        Ok(unwrap(&response))
    }
}

impl BankConnectFx<'_> {
    /// Current rates on the connection (`GET /fx/rates`):
    /// `{items, refreshSeconds, fetchedAt}`; rates are decimal text.
    pub async fn list_rates(&self, connection_id: &str) -> Result<Value> {
        let query = vec![("connectionId".to_owned(), id_text(connection_id)?)];
        let response = route_call(self.client, "listFxRates", &[], query, None, None).await?;
        Ok(unwrap(&response))
    }
    /// Quotes an exchange (`POST /fx/quotes`, HTTP 201): `sourceAccountId`
    /// (you sell from it), `targetAccountId` (you buy into it) and exactly
    /// one of `sourceAmountMinor` / `targetAmountMinor` (positive minor-unit
    /// text). Nothing is booked.
    pub async fn create_quote(&self, request: &Value) -> Result<Value> {
        let body = check_body("createFxQuote", request)?;
        if body.contains_key("sourceAmountMinor") == body.contains_key("targetAmountMinor") {
            return Err(Error::Invalid(
                "createFxQuote takes exactly one of sourceAmountMinor and targetAmountMinor",
            ));
        }
        if body.get("sourceAccountId") == body.get("targetAccountId") {
            return Err(Error::Invalid("createFxQuote needs two different accounts"));
        }
        let response = route_call(
            self.client,
            "createFxQuote",
            &[],
            vec![],
            Some(Value::Object(body)),
            None,
        )
        .await?;
        Ok(unwrap(&response))
    }
    /// Executes a quote (`POST /fx/orders`, HTTP 201): `{order, replayed}`.
    /// `request`: `quoteId`, optional `description` (at most 140
    /// characters). Same key rule as `transfers().create`. 409
    /// `bank_connect.fx.quote_expired` / `quote_used`: ask for a new quote.
    pub async fn create_order(
        &self,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<Value> {
        let body = check_body("createFxOrder", request)?;
        if let Some(description) = body.get("description") {
            check_description(description)?;
        }
        let key = key_for(idempotency_key)?;
        let response = route_call(
            self.client,
            "createFxOrder",
            &[],
            vec![],
            Some(Value::Object(body)),
            Some(key),
        )
        .await?;
        Ok(unwrap(&response))
    }
    /// Exchange orders, newest first (`GET /fx/orders`): `status` (one or a
    /// list of [`bank_fx_order_status`]), `limit`, `cursor`.
    pub async fn list_orders(&self, filters: &Value) -> Result<CursorPage> {
        let query = transfer_query(filters, "listFxOrders")?;
        let response = route_call(self.client, "listFxOrders", &[], query, None, None).await?;
        Ok(to_page(&response))
    }
    /// One exchange order; 404 `bank_connect.fx.order_not_found` for an unknown id.
    pub async fn get_order(&self, order_id: &str) -> Result<Value> {
        let response =
            route_call(self.client, "getFxOrder", &[order_id], vec![], None, None).await?;
        Ok(unwrap(&response))
    }
}

// ── Transfer routes ─────────────────────────────────────────────────────────

async fn route_call(
    client: &Client,
    name: &str,
    params: &[&str],
    query: Vec<(String, String)>,
    body: Option<Value>,
    key: Option<String>,
) -> Result<Response> {
    let route = find(BANK_TRANSFER_ROUTES, name)?;
    let mut request = Request::new(route.http_method(), fill_path(route, params)?);
    request.query = query;
    request.body = body;
    request.idempotency_key = key;
    client.send(request).await
}

/// A caller's key is checked (never trimmed); `None` or `""` gets a new UUID v4.
fn key_for(key: Option<&str>) -> Result<String> {
    match key {
        None | Some("") => Ok(new_idempotency_key()),
        Some(key) if valid_printable_key(key) => Ok(key.to_owned()),
        Some(_) => Err(Error::Invalid(
            "bank transfer Idempotency-Key: 8-200 printable ASCII characters without spaces",
        )),
    }
}

const AMOUNT_FIELDS: &[&str] = &["amountMinor", "sourceAmountMinor", "targetAmountMinor"];

/// Whitelists the body against the route: unknown or missing fields fail
/// before sending; `null` and `""` leave a field out; amounts are positive
/// minor-unit text; required text fields are trimmed.
fn check_body(name: &str, request: &Value) -> Result<Map<String, Value>> {
    let route = find(BANK_TRANSFER_ROUTES, name)?;
    let object = object_of(request, "bank transfer request must be a JSON object")?;
    let mut body = Map::new();
    for (key, value) in object {
        if value.is_null() || value.as_str() == Some("") {
            continue;
        }
        if !route.accepts(key) {
            return Err(Error::Invalid(
                "bank transfer request field not accepted (the gateway rejects unknown fields with 400)",
            ));
        }
        let checked = if AMOUNT_FIELDS.contains(&key.as_str()) {
            Value::String(minor_text(
                value,
                true,
                "bank transfer amount: positive minor-unit text",
            )?)
        } else {
            value.clone()
        };
        body.insert(key.clone(), checked);
    }
    for key in route.required {
        let Some(value) = body.get(*key) else {
            return Err(Error::Invalid("required bank transfer field missing"));
        };
        if !AMOUNT_FIELDS.contains(key) {
            let text = value
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .ok_or(Error::Invalid(
                    "bank transfer field must be a non-empty string",
                ))?
                .to_owned();
            body.insert((*key).to_owned(), Value::String(text));
        }
    }
    Ok(body)
}

fn check_transfer_body(body: &mut Map<String, Value>) -> Result<()> {
    let length = |key: &str| {
        body.get(key)
            .and_then(Value::as_str)
            .map_or(0, |text| text.chars().count())
    };
    if body
        .get("receiverIban")
        .and_then(Value::as_str)
        .is_some_and(|iban| iban.len() > BANK_TRANSFER_IBAN_MAX)
    {
        return Err(Error::Invalid(
            "bank transfer receiverIban: at most 42 characters",
        ));
    }
    let name = length("receiverName");
    if !(BANK_TRANSFER_RECEIVER_NAME_MIN..=BANK_TRANSFER_RECEIVER_NAME_MAX).contains(&name) {
        return Err(Error::Invalid(
            "bank transfer receiverName: 2-140 characters",
        ));
    }
    if body.contains_key("currency") {
        let currency = require_currency(
            body.get("currency"),
            "bank transfer currency: an ISO 4217 code such as TRY",
        )?;
        body.insert("currency".to_owned(), Value::String(currency));
    }
    if let Some(purpose) = body.get("paymentPurpose") {
        let valid = purpose.as_str().is_some_and(|code| {
            (1..=64).contains(&code.len())
                && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        });
        if !valid {
            return Err(Error::Invalid(
                "bank transfer paymentPurpose: a code from list_purposes (letters, digits, underscore; at most 64)",
            ));
        }
    }
    if let Some(description) = body.get("description") {
        check_description(description)?;
    }
    Ok(())
}

fn check_description(value: &Value) -> Result<()> {
    if value
        .as_str()
        .is_some_and(|text| text.chars().count() <= BANK_TRANSFER_DESCRIPTION_MAX)
    {
        Ok(())
    } else {
        Err(Error::Invalid(
            "bank transfer description: a string of at most 140 characters",
        ))
    }
}

fn id_text(value: &str) -> Result<String> {
    let id = value.trim();
    if id.is_empty() {
        Err(Error::Invalid("bank-connect id (a non-empty string)"))
    } else {
        Ok(id.to_owned())
    }
}

/// Filters of `list` / `list_orders`, checked against the route's query fields.
fn transfer_query(filters: &Value, route: &str) -> Result<Vec<(String, String)>> {
    let allowed = find(BANK_TRANSFER_ROUTES, route)?.query;
    let statuses = if route == "listFxOrders" {
        bank_fx_order_status::ALL
    } else {
        bank_transfer_status::ALL
    };
    pick(filters, allowed, |key, value| match key {
        "status" => list_of(value, statuses, "bank transfer status filter"),
        "accountId" => text_of(value).and_then(|text| id_text(&text)),
        "createdFrom" | "createdTo" => text_of(value),
        other => common_filter(other, value),
    })
}

// ── Bank-account filters ────────────────────────────────────────────────────

fn build_query(filters: &Value, route: &str) -> Result<Vec<(String, String)>> {
    let allowed = bank_connect_filters(route).ok_or(Error::Invalid("bank-connect route"))?;
    pick(filters, allowed, |key, value| match key {
        "status" if route == "listAccounts" => one_of(
            value,
            bank_account_status::ALL,
            "bank account status filter",
        ),
        "status" => list_of(
            value,
            bank_connection_status::ALL,
            "bank connection status filter",
        ),
        "direction" => one_of(
            value,
            bank_transaction_direction::ALL,
            "bank transaction direction filter",
        ),
        "canonicalType" => list_of(
            value,
            bank_transaction_type::ALL,
            "bank transaction canonicalType filter",
        ),
        "accountId" => account_ids(value),
        "bankId" | "connectionId" => text_of(value).and_then(|text| id_text(&text)),
        "currency" => require_currency(
            Some(value),
            "bank-connect currency: an ISO 4217 code such as TRY",
        ),
        "bookedFrom" | "bookedTo" => text_of(value),
        "minAmountMinor" | "maxAmountMinor" => minor_text(
            value,
            false,
            "bank-connect amount filter: minor-unit text (0 or more)",
        ),
        other => common_filter(other, value),
    })
}

/// Reads the allowed filters in order; `null`, `""` and `[]` leave a filter
/// out; an unknown filter is refused.
fn pick(
    filters: &Value,
    allowed: &[&str],
    value_of: impl Fn(&str, &Value) -> Result<String>,
) -> Result<Vec<(String, String)>> {
    if filters.is_null() {
        return Ok(vec![]);
    }
    let object = object_of(filters, "bank-connect filters must be a JSON object")?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(Error::Invalid(
            "unknown bank-connect filter (the gateway rejects unknown query fields with 400)",
        ));
    }
    let mut query = vec![];
    for key in allowed {
        let Some(value) = object.get(*key) else {
            continue;
        };
        let empty = match value {
            Value::Null => true,
            Value::String(text) => text.is_empty(),
            Value::Array(list) => list.is_empty(),
            _ => false,
        };
        if !empty {
            query.push(((*key).to_owned(), value_of(key, value)?));
        }
    }
    Ok(query)
}

fn common_filter(key: &str, value: &Value) -> Result<String> {
    match key {
        "limit" => value
            .as_i64()
            .filter(|n| (1..=BANK_CONNECT_MAX_LIMIT).contains(n))
            .map(|n| n.to_string())
            .ok_or(Error::Invalid(
                "bank-connect limit: an integer from 1 to 100",
            )),
        "cursor" => value
            .as_str()
            .filter(|cursor| cursor.len() <= CURSOR_MAX_LENGTH)
            .map(str::to_owned)
            .ok_or(Error::Invalid(
                "bank-connect cursor: the nextCursor of the previous page, unchanged",
            )),
        _ => Err(Error::Invalid("unknown bank-connect filter")),
    }
}

fn text_of(value: &Value) -> Result<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or(Error::Invalid(
            "bank-connect filter must be a non-empty string",
        ))
}

fn one_of(value: &Value, allowed: &[&str], message: &'static str) -> Result<String> {
    value
        .as_str()
        .filter(|text| allowed.contains(text))
        .map(str::to_owned)
        .ok_or(Error::Invalid(message))
}

/// One value or a list, sent comma-separated.
fn list_of(value: &Value, allowed: &[&str], message: &'static str) -> Result<String> {
    let values = match value {
        Value::Array(list) => list.iter().collect::<Vec<_>>(),
        other => vec![other],
    };
    let checked = values
        .into_iter()
        .map(|entry| one_of(entry, allowed, message))
        .collect::<Result<Vec<_>>>()?;
    Ok(checked.join(","))
}

fn account_ids(value: &Value) -> Result<String> {
    let values = match value {
        Value::Array(list) => list.iter().collect::<Vec<_>>(),
        other => vec![other],
    };
    if values.len() > BANK_CONNECT_MAX_ACCOUNT_FILTER {
        return Err(Error::Invalid(
            "bank-connect accountId: at most 100 account ids",
        ));
    }
    let ids = values
        .into_iter()
        .map(|entry| {
            entry
                .as_str()
                .map(str::trim)
                .filter(|id| !id.is_empty() && !id.contains(','))
                .map(str::to_owned)
                .ok_or(Error::Invalid(
                    "bank-connect accountId: account ids (strings without commas)",
                ))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ids.join(","))
}

/// `bookedTo` needs `bookedFrom`; when both parse, `bookedFrom <= bookedTo`
/// and at most 92 days apart.
fn check_booked_range(query: &[(String, String)]) -> Result<()> {
    let get = |name: &str| {
        query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    let (from, to) = (get("bookedFrom"), get("bookedTo"));
    if to.is_some() && from.is_none() {
        return Err(Error::Invalid(
            "bank-connect bookedTo needs bookedFrom (a window without a start is unbounded)",
        ));
    }
    let parse = |text: Option<&str>| text.and_then(crate::timestamp::parse_rfc3339);
    if let (Some(from), Some(to)) = (parse(from), parse(to)) {
        if from > to {
            return Err(Error::Invalid(
                "bank-connect bookedFrom must not be after bookedTo",
            ));
        }
        if to.duration_since(from).map_or(0, |span| span.as_secs())
            > BANK_CONNECT_BOOKED_RANGE_MAX_DAYS as u64 * 86_400
        {
            return Err(Error::Invalid(
                "bank-connect bookedFrom and bookedTo: at most 92 days apart",
            ));
        }
    }
    Ok(())
}
