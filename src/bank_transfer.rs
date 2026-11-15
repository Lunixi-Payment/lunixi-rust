//! Para Transferi (`BANK_TRANSFER`, bank transfer by havale / EFT / FAST): the
//! buyer sends the money from their own bank to the merchant's bank account.
//! The money never passes through Lunixi; the incoming transaction is read
//! from the merchant's connected bank account and matched to the payment.

use crate::{Error, Method, Request, Response, Result, payment_action_type, payments::Payments};
use serde::Deserialize;
use serde_json::Value;

/// The "Other banks" option of the `bankCode` field: the buyer pays by
/// EFT/FAST from a bank that is not listed.
pub const BANK_TRANSFER_OTHER_BANKS_CODE: &str = "9999";
/// Largest `limit` of `list_bank_transfer_exceptions`.
pub const BANK_TRANSFER_EXCEPTION_LIMIT_MAX: u32 = 200;

/// Kind of a bank transfer exception (`kind`).
pub mod bank_transfer_exception_kind {
    /// Arrived after the deadline (within 7 days).
    pub const LATE: &str = "LATE";
    /// The deadline passed with less than expected; the payment waits for your decision.
    pub const UNDERPAID: &str = "UNDERPAID";
    /// More than expected arrived; refund the excess.
    pub const OVERPAID: &str = "OVERPAID";
}

/// Status of a bank transfer exception (`status`).
pub mod bank_transfer_exception_status {
    /// `PENDING_DECISION`
    pub const PENDING_DECISION: &str = "PENDING_DECISION";
    /// `ACCEPTED`
    pub const ACCEPTED: &str = "ACCEPTED";
    /// `REFUND_PENDING`
    pub const REFUND_PENDING: &str = "REFUND_PENDING";
    /// `REFUNDED`
    pub const REFUNDED: &str = "REFUNDED";
    /// Every status.
    pub const ALL: &[&str] = &[PENDING_DECISION, ACCEPTED, REFUND_PENDING, REFUNDED];
}

/// Bank transfer webhook events of a payment (`payment.*` envelope). They do
/// not change the payment status: the outcome is still `payment.captured` /
/// `payment.failed`. Distinct from the Bank Connect `bank_transfer.*` events.
pub mod payment_bank_transfer_events {
    /// Part of the expected amount arrived.
    pub const PARTIALLY_RECEIVED: &str = "payment.bank_transfer.partially_received";
    /// The deadline passed with less than expected (an exception opened).
    pub const UNDERPAID: &str = "payment.bank_transfer.underpaid";
    /// More than expected arrived (an exception opened).
    pub const OVERPAID: &str = "payment.bank_transfer.overpaid";
    /// A transfer arrived after the deadline (an exception opened).
    pub const LATE_ARRIVAL: &str = "payment.bank_transfer.late_arrival";
}

/// Filters of `list_bank_transfer_exceptions`; `None` filters are not sent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BankTransferExceptionFilters {
    /// `TEST` or `LIVE`.
    pub environment: Option<String>,
    /// One of [`bank_transfer_exception_status`].
    pub status: Option<String>,
    /// 1 to [`BANK_TRANSFER_EXCEPTION_LIMIT_MAX`].
    pub limit: Option<u32>,
}
impl BankTransferExceptionFilters {
    /// Only exceptions of `TEST` or `LIVE` payments.
    pub fn with_environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = Some(environment.into());
        self
    }
    /// Only exceptions in this status ([`bank_transfer_exception_status`]).
    pub fn with_status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }
    /// At most `limit` exceptions (1 to [`BANK_TRANSFER_EXCEPTION_LIMIT_MAX`]).
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }
    fn query(&self) -> Result<Vec<(String, String)>> {
        let mut query = vec![];
        if let Some(environment) = &self.environment {
            if !["TEST", "LIVE"].contains(&environment.as_str()) {
                return Err(Error::Invalid(
                    "bank transfer exception environment (TEST or LIVE)",
                ));
            }
            query.push(("environment".into(), environment.clone()));
        }
        if let Some(status) = &self.status {
            if !bank_transfer_exception_status::ALL.contains(&status.as_str()) {
                return Err(Error::Invalid("bank transfer exception status"));
            }
            query.push(("status".into(), status.clone()));
        }
        if let Some(limit) = self.limit {
            if !(1..=BANK_TRANSFER_EXCEPTION_LIMIT_MAX).contains(&limit) {
                return Err(Error::Invalid("bank transfer exception limit (1-200)"));
            }
            query.push(("limit".into(), limit.to_string()));
        }
        Ok(query)
    }
}

/// Transfer instruction of a `BANK_TRANSFER` payment (`data.action.bankTransfer`
/// of a `DISPLAY` action): the merchant's own bank account to show the buyer.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BankTransferInstruction {
    /// 4-digit BKM code of the merchant's bank the money is sent to.
    pub bank_code: String,
    pub bank_name: String,
    /// Account holder name; the buyer enters it as the recipient name.
    pub holder_name: String,
    /// IBAN without spaces.
    pub iban: String,
    /// Amount to send (canonical minor units).
    pub amount_minor: String,
    pub currency: String,
    /// Code the buyer writes in the transfer description; it matches the transfer to the payment.
    pub payment_code: String,
    pub expires_at: String,
    /// `true`: the buyer chose "Other banks" (`9999`); the instruction is for EFT/FAST.
    pub other_bank: bool,
}
impl BankTransferInstruction {
    /// Reads the instruction from a `direct_method_payment` response. `None`
    /// when there is no action, the action is not `DISPLAY`, or it carries no
    /// readable `bankTransfer` object.
    pub fn from_response(response: &Response) -> Option<Self> {
        response.data.as_ref().and_then(Self::from_data)
    }
    /// Same as [`from_response`](Self::from_response) for the `data` member.
    pub fn from_data(data: &Value) -> Option<Self> {
        let action = data.get("action")?;
        if action.get("type").and_then(Value::as_str) != Some(payment_action_type::DISPLAY) {
            return None;
        }
        serde_json::from_value(action.get("bankTransfer")?.clone()).ok()
    }
}

pub(crate) fn is_uuid(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                *c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

fn exception_path(exception_id: &str, decision: &str) -> Result<String> {
    if !is_uuid(exception_id) {
        return Err(Error::Invalid(
            "bank transfer exception ID (UUID, items[].id)",
        ));
    }
    Ok(format!(
        "/api/v1/payments/bank-transfer/exceptions/{exception_id}/{decision}"
    ))
}

impl Payments<'_> {
    /// Bank transfer exceptions (`GET /payments/bank-transfer/exceptions`):
    /// late, short and excess transfers of `BANK_TRANSFER` payments. `data` is
    /// `{ items: [{ id, paymentId, kind, status, environment, amountMinor,
    /// currency, expectedMinor, receivedMinor, counterpartyNameMasked, … }] }`.
    pub async fn list_bank_transfer_exceptions(
        &self,
        filters: &BankTransferExceptionFilters,
    ) -> Result<Response> {
        let mut request = Request::new(Method::GET, "/api/v1/payments/bank-transfer/exceptions");
        request.query = filters.query()?;
        self.client.send(request).await
    }
    /// Accepts a bank transfer exception (`POST …/exceptions/{id}/accept`; the
    /// key needs `payment:refund`). `LATE` opens a new captured payment for the
    /// same order (`acceptedPaymentId`); `UNDERPAID` captures the amount
    /// received. `OVERPAID` cannot be accepted (409). `data` is the exception
    /// or null.
    pub async fn accept_bank_transfer_exception(&self, exception_id: &str) -> Result<Response> {
        self.client
            .send(Request::new(
                Method::POST,
                exception_path(exception_id, "accept")?,
            ))
            .await
    }
    /// Reports that you refunded the money of a bank transfer exception to the
    /// sender from your own bank (`POST …/exceptions/{id}/refunded`). Lunixi
    /// does not send the refund. For `UNDERPAID` the waiting payment closes as
    /// failed. `data` is the exception or null.
    pub async fn mark_bank_transfer_exception_refunded(
        &self,
        exception_id: &str,
    ) -> Result<Response> {
        self.client
            .send(Request::new(
                Method::POST,
                exception_path(exception_id, "refunded")?,
            ))
            .await
    }
}
