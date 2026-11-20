//! Split payments: leg block rule, list filters and webhook event names.

use crate::{Error, Method, Request, Response, Result, payment_method, payments::Payments};
use serde_json::Value;

/// Method blocks a non-card split leg can carry. Meal card, open banking,
/// shopping loan and bank transfer cannot be split legs.
pub const SPLIT_LEG_BLOCK_FIELDS: &[&str] = &["mobileBilling", "wallet", "promissoryNote"];
/// Body fields of `POST /payments/split`. The order details (`buyer`,
/// `billingAddress`, `basketItems`; `shippingAddress` for a `PHYSICAL` item)
/// are required ([`ORDER_DETAILS_REQUIRED_FIELDS`](crate::ORDER_DETAILS_REQUIRED_FIELDS)).
pub const SPLIT_PAYMENT_FIELDS: &[&str] = &[
    "totalAmount",
    "currency",
    "ownerRef",
    "legs",
    "orderId",
    "ownerScope",
    "callbackUrl",
    "buyer",
    "billingAddress",
    "shippingAddress",
    "basketItems",
];
/// Fields of one split leg.
pub const SPLIT_PAYMENT_LEG_FIELDS: &[&str] = &[
    "paymentMethod",
    "cardToken",
    "amount",
    "force3D",
    "installment",
    "installmentQuoteToken",
    "mobileBilling",
    "wallet",
    "promissoryNote",
];
/// Filters of [`Payments::list_splits`].
pub const LIST_SPLIT_PAYMENTS_FILTERS: &[&str] = &[
    "status",
    "currency",
    "createdFrom",
    "createdTo",
    "orderId",
    "ownerRef",
    "limit",
    "after",
];
/// Put this key into a webhook endpoint's event types to also receive the
/// events of each leg. It is not an event type and `payment.*` does not include it.
pub const SPLIT_LEG_EVENTS_SUBSCRIPTION: &str = "payment.split.leg_events";

/// Webhook event types of a split payment. The result of the main payment
/// arrives as `payment.captured` / `payment.failed` with `settlementMode: SPLIT`;
/// deliver the order on that event.
pub mod split_payment_events {
    /// Every leg was charged.
    pub const COMPLETED: &str = "payment.split.completed";
    /// Failed without a charge left on any leg.
    pub const VOIDED: &str = "payment.split.voided";
    /// A leg failed: holds are released and charged legs are refunded.
    pub const COMPENSATING: &str = "payment.split.compensating";
    /// The non-reversible leg's result is being confirmed.
    pub const OUTCOME_PENDING: &str = "payment.split.outcome_pending";
    /// The order could not be fully collected (`shortfallAmountMinor`, `reason`).
    pub const RECONCILIATION_REQUIRED: &str = "payment.split.reconciliation_required";
    /// Leg event: a charged leg is being refunded and retried until it succeeds.
    pub const LEG_REFUND_PENDING: &str = "payment.split.leg_refund_pending";
}

const NON_LEG_BLOCKS: &[&str] = &["mealCard", "openBanking", "shoppingLoan", "bankTransfer"];
const SPLIT_STATUSES: &[&str] = &[
    "COMPOSING",
    "AUTHORIZING",
    "AWAITING_3DS",
    "AWAITING_ACTION",
    "AWAITING_CREDIT_APPROVAL",
    "AWAITING_REPLACEMENT_CARD",
    "AUTHORIZED",
    "CAPTURING",
    "COMPLETED",
    "COMPENSATING",
    "VOIDED",
    "FAILED",
    "RECONCILIATION_REQUIRED",
];

fn given<'a>(leg: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a Value> {
    leg.get(key).filter(|v| !v.is_null())
}

/// A card leg (`paymentMethod` absent, empty or `CARD`) carries no method
/// block; a non-card leg carries exactly its own block, one of
/// [`SPLIT_LEG_BLOCK_FIELDS`].
pub(crate) fn validate_leg_blocks(leg: &Value) -> Result<()> {
    let leg = leg.as_object().ok_or(Error::Invalid("split leg object"))?;
    if NON_LEG_BLOCKS.iter().any(|f| leg.contains_key(*f)) {
        return Err(Error::Invalid(
            "split leg block (only mobileBilling, wallet or promissoryNote)",
        ));
    }
    let method = match given(leg, "paymentMethod") {
        None => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(payment_method::normalize(s)?),
        Some(_) => return Err(Error::Invalid("split leg paymentMethod")),
    };
    let own = match method {
        None | Some(payment_method::CARD) => None,
        Some(code) => {
            let name = payment_method::block_name(code)?;
            if !SPLIT_LEG_BLOCK_FIELDS.contains(&name.as_str()) {
                return Err(Error::Invalid(
                    "split leg payment method (MEAL_CARD, OPEN_BANKING, SHOPPING_LOAN and BANK_TRANSFER cannot be legs)",
                ));
            }
            Some(name)
        }
    };
    if SPLIT_LEG_BLOCK_FIELDS
        .iter()
        .any(|f| Some(*f) != own.as_deref() && given(leg, f).is_some())
    {
        return Err(Error::Invalid(
            "split leg carries only its own method block (a card leg carries none)",
        ));
    }
    if let Some(name) = own
        && !given(leg, &name).is_some_and(Value::is_object)
    {
        return Err(Error::Invalid("split leg requires its method block"));
    }
    Ok(())
}

fn iso_8601(value: &str) -> bool {
    // YYYY-MM-DD[THH:MM[:SS[.fraction]][Z|±HH[:]MM]]
    let b = value.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|s| s.iter().all(u8::is_ascii_digit));
    if b.len() < 10
        || !digits(0..4)
        || b[4] != b'-'
        || !digits(5..7)
        || b[7] != b'-'
        || !digits(8..10)
    {
        return false;
    }
    let mut i = 10;
    if i == b.len() {
        return true;
    }
    if b[i] != b'T' || !digits(i + 1..i + 3) || b.get(i + 3) != Some(&b':') || !digits(i + 4..i + 6)
    {
        return false;
    }
    i += 6;
    if b.get(i) == Some(&b':') {
        if !digits(i + 1..i + 3) {
            return false;
        }
        i += 3;
        if b.get(i) == Some(&b'.') {
            let start = i + 1;
            let mut end = start;
            while b.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if !(1..=9).contains(&(end - start)) {
                return false;
            }
            i = end;
        }
    }
    match b.get(i) {
        None => true,
        Some(b'Z') => i + 1 == b.len(),
        Some(b'+' | b'-') => {
            let rest = &value[i + 1..];
            let r = rest.as_bytes();
            (r.len() == 4 && r.iter().all(u8::is_ascii_digit))
                || (r.len() == 5 && r[2] == b':' && digits(i + 1..i + 3) && digits(i + 4..i + 6))
        }
        Some(_) => false,
    }
}

fn split_list_query(filters: Vec<(String, String)>) -> Result<Vec<(String, String)>> {
    let mut query = vec![];
    let (mut statuses, mut currencies) = (0, 0);
    for (key, value) in filters {
        if !LIST_SPLIT_PAYMENTS_FILTERS.contains(&key.as_str()) {
            return Err(Error::Invalid(
                "split list filter (see LIST_SPLIT_PAYMENTS_FILTERS)",
            ));
        }
        if value.is_empty() {
            continue;
        }
        let valid = match key.as_str() {
            "status" => {
                statuses += 1;
                statuses <= 20 && SPLIT_STATUSES.contains(&value.as_str())
            }
            "currency" => {
                currencies += 1;
                currencies <= 20
                    && value.len() == 3
                    && value.bytes().all(|c| c.is_ascii_uppercase())
            }
            "createdFrom" | "createdTo" => iso_8601(&value),
            "limit" => {
                !value.starts_with('0')
                    && value.bytes().all(|c| c.is_ascii_digit())
                    && value.parse::<u32>().is_ok_and(|n| (1..=100).contains(&n))
            }
            "after" => !value.trim().is_empty() && value.len() <= 2048,
            _ => !value.trim().is_empty() && value.len() <= 200,
        };
        if !valid {
            return Err(Error::Invalid("split list filter value"));
        }
        query.push((key, value));
    }
    Ok(query)
}

impl Payments<'_> {
    /// Split payments, newest first (`GET /payments/split`). Filters (pairs,
    /// `status` and `currency` may repeat): `status`, `currency`,
    /// `createdFrom`, `createdTo` (ISO 8601), `orderId`, `ownerRef`, `limit`
    /// (1–100) and `after` (the `pageInfo.endCursor` of the previous page).
    /// Empty values are not sent.
    pub async fn list_splits(&self, filters: Vec<(String, String)>) -> Result<Response> {
        let mut request = Request::new(Method::GET, "/api/v1/payments/split");
        request.query = split_list_query(filters)?;
        self.client.send(request).await
    }
}
