//! `client.physical_pos()`: payments on card terminals, transactions,
//! reports, end-of-day runs and results.

use super::{
    request::{
        build_body, build_query, key_of, keyed_write, new_idempotency_key, path_id, payment_write,
        record_of, require_key, send, to_page, unwrap,
    },
    results::{PhysicalPosPage, PhysicalPosWriteResult},
};
use crate::{Client, Method, Result};
use serde_json::{Value, json};

/// Physical POS — payments on your card terminals, transactions, reports,
/// end-of-day runs and results (`/api/v1/physical-pos`).
///
/// Every call is signed; the API key decides the environment (a TEST key can
/// read but every payment write is 403
/// `physical_pos.environment.test_key_read_only`). Payment writes need
/// `physical_pos:payments`; reads `physical_pos:view` or
/// `physical_pos:payments`.
///
/// Card data is never sent: the card is read on the device. A request field
/// that looks like card data (`pan`, `cardNumber`, `cvv`, `expiry` …) or a
/// card number in `merchantReference`, an item name / tax group or a
/// free-text filter is `Error::PhysicalPosCardDataRejected` before anything
/// is sent; any other unknown field is `Error::Invalid`.
///
/// A payment write is asynchronous: the answer means "accepted", not "paid".
/// Follow the outcome with the `physical_pos.payment.*` webhooks or
/// `get_payment`. Store the idempotency key with your order first, then
/// call; on `Error::PhysicalPosOutcomeUnknown` call again later with the
/// SAME key. A new key for the same payment can charge the card twice. For a
/// `SALE`, also send `merchantReference` with `allowDuplicateReference:
/// false`: the gateway then refuses a second sale while one with that
/// reference is unresolved.
pub struct PhysicalPos<'a> {
    client: &'a Client,
}
impl Client {
    /// Returns the physical POS payments facade; no independent HTTP client is constructed.
    pub fn physical_pos(&self) -> PhysicalPos<'_> {
        PhysicalPos { client: self }
    }
}
impl PhysicalPos<'_> {
    /// A new `Idempotency-Key` (UUID v4). Store it with your order before you call.
    pub fn new_idempotency_key() -> String {
        new_idempotency_key()
    }

    /// Starts a payment on a device: `SALE`, `PREAUTH_OPEN` or
    /// `REFUND_UNMATCHED` (`POST /payments`). `target` is exactly one of
    /// `{deviceId}`, `{vendorCode, serialNumber}` or `{externalRef}`; money
    /// is canonical minor-unit text (`"4500"`), counts are JSON numbers.
    ///
    /// `instrument` is `CARD` (default) or a provider-collected card such as
    /// `MEAL_CARD`; for those send `interaction` (`INSTRUMENT_READ` or
    /// `CODE_DISPLAY`) and, when the terminal has several provider rows,
    /// `terminalProviderId`. Leave both out for `CARD`.
    ///
    /// The key is required and sent exactly as given. 202 with
    /// `paymentStatus: NOT_STARTED` for a new payment, 200 with
    /// `replayed: true` when the same key and body were sent before.
    pub async fn create_payment(
        &self,
        request: &Value,
        idempotency_key: &str,
    ) -> Result<PhysicalPosWriteResult> {
        let key = require_key(idempotency_key)?;
        let body = build_body(request, "createPayment")?;
        payment_write(self.client, "/payments", body, key).await
    }

    /// Reverses an approved payment (`POST /payments/{paymentId}/reversals`):
    /// `VOID`, `REFUND` (by `amountMinor` or by
    /// `items: [{lineNo, quantityMilli}]`), `PREAUTH_CLOSE` or
    /// `PREAUTH_CANCEL`. The reversal is a new child payment followed like
    /// any payment. Same key rule, result and errors as `create_payment`.
    pub async fn create_reversal(
        &self,
        payment_id: &str,
        request: &Value,
        idempotency_key: &str,
    ) -> Result<PhysicalPosWriteResult> {
        let id = path_id(payment_id)?;
        let key = require_key(idempotency_key)?;
        let body = build_body(request, "createReversal")?;
        payment_write(self.client, &format!("/payments/{id}/reversals"), body, key).await
    }

    /// Cancels a payment before its start command reaches the device
    /// (`POST /payments/{paymentId}/cancel`, 200). Only `EXPIRED` with
    /// `statusReason: MERCHANT_CANCELLED_BEFORE_DISPATCH` means it was
    /// cancelled; a payment already in a final state comes back unchanged
    /// with `replayed: true`. To undo an approved sale use a reversal. The
    /// key is optional (a new UUID for `None`).
    pub async fn cancel_payment(
        &self,
        payment_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<PhysicalPosWriteResult> {
        let id = path_id(payment_id)?;
        let key = key_of(idempotency_key)?;
        payment_write(
            self.client,
            &format!("/payments/{id}/cancel"),
            json!({}),
            key,
        )
        .await
    }

    /// One payment with its items and refund items (`GET /payments/{paymentId}`).
    pub async fn get_payment(&self, payment_id: &str) -> Result<Value> {
        let path = format!("/payments/{}", path_id(payment_id)?);
        Ok(record_of(&self.read(&path, vec![]).await?, "payment"))
    }

    /// Payments, newest first (`GET /payments`). Roots and reversals are in
    /// one list; `items` and `refundItems` are empty here. `merchantReference`
    /// finds a payment after an unknown outcome.
    pub async fn list_payments(&self, filters: &Value) -> Result<PhysicalPosPage> {
        self.page("/payments", filters, "listPayments").await
    }

    /// Transaction trees (root payment + its reversals), newest first (`GET /transactions`).
    pub async fn list_transactions(&self, filters: &Value) -> Result<PhysicalPosPage> {
        self.page("/transactions", filters, "listTransactions")
            .await
    }

    /// The tree of a root or child payment with its timeline (`GET /transactions/{paymentId}`).
    pub async fn get_transaction(&self, payment_id: &str) -> Result<Value> {
        let path = format!("/transactions/{}", path_id(payment_id)?);
        Ok(record_of(&self.read(&path, vec![]).await?, "transaction"))
    }

    /// Payments with an unknown outcome or under manual review (`GET /transactions/ambiguous`).
    pub async fn list_ambiguous_payments(&self, filters: &Value) -> Result<PhysicalPosPage> {
        self.page("/transactions/ambiguous", filters, "listAmbiguousPayments")
            .await
    }

    /// Sale / refund / void counts and amounts with net per currency
    /// (`GET /reports/sales-summary`). `dateFrom` / `dateTo` are business
    /// days (`YYYY-MM-DD`, Europe/Istanbul); `groupBy` DAY, STORE, DEVICE or
    /// INSTRUMENT.
    pub async fn get_sales_summary(&self, filters: &Value) -> Result<Value> {
        let query = build_query(filters, "getSalesSummary")?;
        Ok(unwrap(&self.read("/reports/sales-summary", query).await?))
    }

    /// Tax breakdown with refunds and voids deducted (`GET /reports/tax-summary`).
    pub async fn get_tax_summary(&self, filters: &Value) -> Result<Value> {
        let query = build_query(filters, "getTaxSummary")?;
        Ok(unwrap(&self.read("/reports/tax-summary", query).await?))
    }

    /// End-of-day results per bank line, with the reported / system /
    /// difference totals (`GET /settlements`).
    pub async fn list_settlements(&self, filters: &Value) -> Result<PhysicalPosPage> {
        self.page("/settlements", filters, "listSettlements").await
    }

    /// Starts end-of-day (batch close) on a device
    /// (`POST /devices/{deviceId}/settlement`). End-of-day cannot be undone.
    /// The answer means "accepted": follow the result with
    /// `get_settlement_run` or `list_settlements`. `data.run`, 202 new / 200
    /// replay. Pass the key you stored, or `None` for a new UUID (returned as
    /// `idempotency_key`; after an unknown outcome call again with it).
    pub async fn start_settlement(
        &self,
        device_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<PhysicalPosWriteResult> {
        let path = format!("/devices/{}/settlement", path_id(device_id)?);
        keyed_write(self.client, &path, json!({}), idempotency_key).await
    }

    /// End-of-day runs started by Lunixi (manual and scheduled), newest
    /// first (`GET /settlement-runs`). Filters: `deviceId`, `source`
    /// (MANUAL_API, MANUAL_PANEL, SCHEDULED), `runStatus` (PENDING, STARTED,
    /// SKIPPED, FAILED), `createdFrom` / `createdTo`, `includeTotal`.
    pub async fn list_settlement_runs(&self, filters: &Value) -> Result<PhysicalPosPage> {
        self.page("/settlement-runs", filters, "listSettlementRuns")
            .await
    }

    /// One end-of-day run with its command status and result state (`GET /settlement-runs/{runId}`).
    pub async fn get_settlement_run(&self, run_id: &str) -> Result<Value> {
        let path = format!("/settlement-runs/{}", path_id(run_id)?);
        Ok(record_of(&self.read(&path, vec![]).await?, "run"))
    }

    async fn read(&self, path: &str, query: Vec<(String, String)>) -> Result<crate::Response> {
        send(self.client, Method::GET, path, None, query, None).await
    }

    async fn page(&self, path: &str, filters: &Value, route: &str) -> Result<PhysicalPosPage> {
        let query = build_query(filters, route)?;
        Ok(to_page(&self.read(path, query).await?))
    }
}
