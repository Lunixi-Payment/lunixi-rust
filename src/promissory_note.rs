//! Promissory-note payments (`PROMISSORY_NOTE`): plan rules checked before
//! sending, note endpoints and value sets.
//!
//! The plan is a schedule (`[{ amount, dueDate }]`) or a note count, never
//! both. The SDK checks what can be checked locally: canonical minor-unit
//! amounts adding up to the payment amount, `YYYY-MM-DD` calendar dates in
//! ascending order and at most 24 rows. That a due date lies in the future and
//! that the note count fits your provider connection is checked by the gateway
//! (`400 payment.method.block_invalid`).

use crate::{
    Error, Method, Request, Response, Result,
    client::{require_key, valid_id},
    payments::Payments,
};
use serde::Serialize;
use serde_json::{Value, json};

/// Most rows of a schedule (and most notes of a payment).
pub const PROMISSORY_NOTE_MAX_SCHEDULE_ITEMS: usize = 24;

/// Status of one note (`notes[].status`).
pub mod promissory_note_status {
    /// `ISSUED`
    pub const ISSUED: &str = "ISSUED";
    /// `PAID`
    pub const PAID: &str = "PAID";
    /// `PROTESTED`
    pub const PROTESTED: &str = "PROTESTED";
    /// `CANCELLED`
    pub const CANCELLED: &str = "CANCELLED";
}

/// Document type of the issued notes, from your provider connection settings.
pub mod promissory_note_document_type {
    /// `PROMISSORY_NOTE`
    pub const PROMISSORY_NOTE: &str = "PROMISSORY_NOTE";
    /// `PAYMENT_ORDER`
    pub const PAYMENT_ORDER: &str = "PAYMENT_ORDER";
}

/// Per-note result of `cancel_promissory_notes` (`cancelResults[].result`).
pub mod promissory_note_cancel_result {
    /// The note was cancelled.
    pub const CANCELLED: &str = "CANCELLED";
    /// The note was already cancelled.
    pub const ALREADY_CLOSED: &str = "ALREADY_CLOSED";
    /// The note was collected or protested; it cannot be cancelled.
    pub const NOT_CANCELLABLE: &str = "NOT_CANCELLABLE";
    /// The provider did not cancel the note; `errorCode` tells whether to try again.
    pub const FAILED: &str = "FAILED";
}

/// `cancelResults[].errorCode`: a Lunixi code, never the provider's.
pub mod promissory_note_cancel_error_code {
    /// The note was collected or the provider does not cancel it (`NOT_CANCELLABLE`).
    pub const NOT_CANCELLABLE: &str = "payment.promissory_note.not_cancellable";
    /// Temporary: the provider could not be reached; cancel again later (`FAILED`).
    pub const PROVIDER_UNAVAILABLE: &str = "payment.promissory_note.provider_unavailable";
    /// Permanent: the provider refused the cancellation (`FAILED`).
    pub const CANCEL_REJECTED: &str = "payment.promissory_note.cancel_rejected";
}

/// Language of the printable notes.
pub mod promissory_note_document_locale {
    /// Turkish.
    pub const TR: &str = "tr-TR";
    /// English.
    pub const EN: &str = "en-GB";
}

/// Webhook events of a note. The payload adds `promissoryNote: { serialNumber,
/// sequence, amountMinor, dueDate, status, occurredAt }`.
pub mod promissory_note_events {
    /// A note was paid.
    pub const PAID: &str = "payment.promissory_note.paid";
    /// A note was protested.
    pub const PROTESTED: &str = "payment.promissory_note.protested";
    /// A note was cancelled.
    pub const CANCELLED: &str = "payment.promissory_note.cancelled";
}

/// One note of a schedule: `amount` in canonical minor units, `due_date` as
/// `YYYY-MM-DD` (sent as `dueDate`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromissoryNoteScheduleRow {
    pub amount: String,
    pub due_date: String,
}
impl PromissoryNoteScheduleRow {
    /// Creates a row; it is validated when the request is built.
    pub fn new(amount: impl Into<String>, due_date: impl Into<String>) -> Self {
        Self {
            amount: amount.into(),
            due_date: due_date.into(),
        }
    }
}

const PDF_ACCEPT: &str = "application/pdf, application/json";

fn given(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| !v.is_null())
}

fn is_calendar_date(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        value[..4].parse::<i32>(),
        value[5..7].parse::<u32>(),
        value[8..].parse::<u32>(),
    ) else {
        return false;
    };
    chrono::NaiveDate::from_ymd_opt(year, month, day).is_some()
}

/// Checks a schedule (1–24 `{ amount, dueDate }` rows, ascending dates) and
/// returns the sum of its amounts.
pub(crate) fn validate_schedule(value: &Value) -> Result<i64> {
    let rows = value
        .as_array()
        .filter(|rows| !rows.is_empty())
        .ok_or(Error::Invalid(
            "promissory-note schedule: non-empty list of rows",
        ))?;
    if rows.len() > PROMISSORY_NOTE_MAX_SCHEDULE_ITEMS {
        return Err(Error::Invalid("promissory-note schedule: at most 24 rows"));
    }
    let mut previous: Option<&str> = None;
    let mut total = 0i64;
    for row in rows {
        let object = row.as_object().ok_or(Error::Invalid(
            "promissory-note schedule row: { amount, dueDate }",
        ))?;
        if object.keys().any(|k| k != "amount" && k != "dueDate") {
            return Err(Error::Invalid(
                "promissory-note schedule row: unknown field (a row is { amount, dueDate })",
            ));
        }
        let amount = object
            .get("amount")
            .and_then(Value::as_str)
            .ok_or(Error::Invalid("promissory-note schedule row amount"))?;
        total = total
            .checked_add(crate::money::parse_minor(amount, true, false)?)
            .ok_or(Error::Invalid("promissory-note schedule total"))?;
        let due = object
            .get("dueDate")
            .and_then(Value::as_str)
            .filter(|d| is_calendar_date(d))
            .ok_or(Error::Invalid(
                "promissory-note schedule row dueDate (YYYY-MM-DD calendar date)",
            ))?;
        if previous.is_some_and(|p| due < p) {
            return Err(Error::Invalid(
                "promissory-note schedule: rows must be in due-date order",
            ));
        }
        previous = Some(due);
    }
    Ok(total)
}

/// A note count 1–24, as a number or its digit string; returned as the string sent.
pub(crate) fn normalize_note_count(value: &Value) -> Result<String> {
    let text = match value {
        Value::Number(n) => n.as_u64().map(|n| n.to_string()),
        Value::String(s) => Some(s.clone()),
        _ => None,
    };
    text.filter(|t| {
        let b = t.as_bytes();
        (1..=2).contains(&b.len())
            && b[0] != b'0'
            && b.iter().all(u8::is_ascii_digit)
            && t.parse::<usize>()
                .is_ok_and(|n| n <= PROMISSORY_NOTE_MAX_SCHEDULE_ITEMS)
    })
    .ok_or(Error::Invalid(
        "promissory-note noteCount: a number from 1 to 24",
    ))
}

/// The plan of a `promissoryNote` block: exactly one of `schedule` or
/// `noteCount`; a schedule must add up to `amount`.
pub(crate) fn validate_plan(block: &Value, amount: &str) -> Result<()> {
    let object = block
        .as_object()
        .ok_or(Error::Invalid("promissoryNote block object"))?;
    match (
        given(object.get("schedule")),
        given(object.get("noteCount")),
    ) {
        (Some(_), Some(_)) => Err(Error::Invalid(
            "promissoryNote: send either schedule or noteCount, not both",
        )),
        (None, None) => Err(Error::Invalid(
            "promissoryNote needs a plan: schedule or noteCount",
        )),
        (None, Some(count)) => normalize_note_count(count).map(|_| ()),
        (Some(schedule), None) => {
            if validate_schedule(schedule)? != crate::money::parse_minor(amount, true, false)? {
                return Err(Error::Invalid(
                    "promissoryNote schedule must add up to amount",
                ));
            }
            Ok(())
        }
    }
}

fn validate_serial_numbers(values: &[&str]) -> Result<()> {
    if values.is_empty() || values.len() > PROMISSORY_NOTE_MAX_SCHEDULE_ITEMS {
        return Err(Error::Invalid(
            "serial numbers: 1 to 24 (pass None to cancel every open note)",
        ));
    }
    let valid = |s: &&str| {
        (4..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    };
    if values.iter().all(valid) {
        Ok(())
    } else {
        Err(Error::Invalid(
            "serial number (letters, digits and -, 4 to 64 characters)",
        ))
    }
}

fn notes_path(payment_id: &str) -> Result<String> {
    Ok(format!(
        "/api/v1/payments/{}/promissory-notes",
        valid_id(payment_id)?
    ))
}

impl Payments<'_> {
    /// The notes of a promissory-note payment
    /// (`GET /payments/{paymentId}/promissory-notes`): `documentType`,
    /// `rowSerialNumber`, `totalAmount` and per note `serialNumber`,
    /// `sequence`, `amount`, `dueDate` and `status`. A payment without notes is
    /// a 404 `payment.promissory_note.not_found`.
    pub async fn get_promissory_notes(&self, payment_id: &str) -> Result<Response> {
        self.client
            .send(Request::new(Method::GET, notes_path(payment_id)?))
            .await
    }
    /// Cancels notes at the provider
    /// (`POST /payments/{paymentId}/promissory-notes/cancel`). `None` cancels
    /// every open note (no body); otherwise 1–24 serial numbers from
    /// `get_promissory_notes`. The answer carries `cancelResults[]` per note
    /// ([`promissory_note_cancel_result`], [`promissory_note_cancel_error_code`]).
    /// The payment stays `CAPTURED`: no funds moved through the platform.
    pub async fn cancel_promissory_notes(
        &self,
        payment_id: &str,
        serial_numbers: Option<&[&str]>,
        key: Option<&str>,
    ) -> Result<Response> {
        let mut request = Request::new(Method::POST, format!("{}/cancel", notes_path(payment_id)?));
        if let Some(values) = serial_numbers {
            validate_serial_numbers(values)?;
            request = request.json(json!({ "serialNumbers": values }));
        }
        if let Some(k) = key {
            require_key(k)?;
            request = request.idempotency_key(k);
        }
        self.client.send(request).await
    }
    /// The printable notes as a PDF, one note per page
    /// (`GET /payments/{paymentId}/promissory-notes/document`). The document is
    /// in `Response::bytes` (`content_type` `application/pdf`), bounded by
    /// `max_response_bytes`; errors arrive as the usual JSON envelope.
    /// `locale` is [`promissory_note_document_locale::TR`] or `EN`. A 503
    /// `payment.promissory_note.document_unavailable` means try again later.
    pub async fn download_promissory_note_document(
        &self,
        payment_id: &str,
        locale: Option<&str>,
    ) -> Result<Response> {
        let mut request =
            Request::new(Method::GET, format!("{}/document", notes_path(payment_id)?));
        if let Some(l) = locale {
            if ![
                promissory_note_document_locale::TR,
                promissory_note_document_locale::EN,
            ]
            .contains(&l)
            {
                return Err(Error::Invalid("document locale (tr-TR or en-GB)"));
            }
            request.query.push(("locale".into(), l.into()));
        }
        let response = self.client.send_binary(request, PDF_ACCEPT).await?;
        let pdf = response
            .content_type
            .as_deref()
            .is_some_and(|t| t.to_ascii_lowercase().starts_with("application/pdf"));
        if response.envelope.is_some() || !pdf || response.bytes.is_empty() {
            return Err(Error::Response("PDF document"));
        }
        Ok(response)
    }
}
