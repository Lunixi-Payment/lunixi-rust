//! Physical POS page and write results.

use serde_json::{Map, Value};
use std::fmt;

/// One page of a physical POS list: `items` plus the envelope's `pageInfo`.
/// `total_count` is filled only with `includeTotal: true` on entity lists.
/// Pass `next_cursor` as the `cursor` filter to read the next page.
#[derive(Clone, PartialEq)]
#[non_exhaustive]
pub struct PhysicalPosPage {
    /// The page's records (payments, devices, stores …).
    pub items: Vec<Value>,
    /// `true` when another page exists (and `next_cursor` is set).
    pub has_more: bool,
    /// The cursor of the next page.
    pub next_cursor: Option<String>,
    /// The total number of records, when asked for.
    pub total_count: Option<i64>,
    /// The gateway's `X-Request-Id`.
    pub request_id: Option<String>,
}
impl fmt::Debug for PhysicalPosPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PhysicalPosPage")
            .field("items", &self.items.len())
            .field("has_more", &self.has_more)
            .field("total_count", &self.total_count)
            .finish_non_exhaustive()
    }
}

/// The answer of a physical POS write that carries an `Idempotency-Key`.
///
/// Payment writes (`create_payment`, `create_reversal`) answer 202 for a new
/// payment — accepted, not paid: follow the outcome by webhook or
/// `get_payment` — and 200 with `replayed: true` when the same key and body
/// were sent again (the payment's CURRENT view). `cancel_payment` answers
/// 200. End-of-day starts answer 202 / 200; management creates 201 / 200.
#[derive(Clone, PartialEq)]
#[non_exhaustive]
pub struct PhysicalPosWriteResult {
    /// The whole `data` object (`payment`, `run`, `store`, `device`, sibling
    /// fields such as `closedAssignment` or `link`).
    pub data: Map<String, Value>,
    /// `true` when the same key and body were already recorded.
    pub replayed: bool,
    /// The gateway's status: the envelope's `statusCode`, else the HTTP status.
    pub status_code: u16,
    /// The `Idempotency-Key` that was sent (yours, or the one the SDK made).
    pub idempotency_key: String,
    /// The gateway's `X-Request-Id`.
    pub request_id: Option<String>,
}
impl PhysicalPosWriteResult {
    /// The payment of a payment write (`None` for other writes).
    pub fn payment(&self) -> Option<&Value> {
        self.record("payment")
    }
    /// One record of `data` (`payment`, `run`, `store`, `terminal`, `device` …).
    pub fn record(&self, name: &str) -> Option<&Value> {
        self.data.get(name).filter(|value| value.is_object())
    }
    /// `true` for 202 Accepted: a new payment, reversal or end-of-day run
    /// was queued to the device.
    pub fn accepted(&self) -> bool {
        self.status_code == 202
    }
}
impl fmt::Debug for PhysicalPosWriteResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PhysicalPosWriteResult")
            .field("replayed", &self.replayed)
            .field("status_code", &self.status_code)
            .field("idempotency_key", &self.idempotency_key)
            .field("request_id", &self.request_id)
            .finish_non_exhaustive()
    }
}
