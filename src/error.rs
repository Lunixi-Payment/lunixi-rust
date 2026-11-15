use std::fmt;

/// A safe SDK error. Upstream bodies, credentials and URLs are never retained.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Configuration/request validation failed before network access.
    Invalid(&'static str),
    /// HTTP/API failure with safe diagnostics only.
    Api {
        status: u16,
        code: String,
        request_id: Option<String>,
    },
    /// Transport failure; underlying errors may contain URLs and are omitted.
    Transport,
    /// The bounded response could not be read or decoded.
    Response(&'static str),
    /// A webhook was invalid, stale, tampered with or collided with an event ID.
    Webhook,
    /// Durable storage failed. Do not acknowledge this webhook with HTTP 2xx.
    Inbox,
    /// Physical POS: a request carried card data (a card-data field name, or
    /// a card number in a scanned free-text value). The card is read on the
    /// device; card numbers, expiry dates and security codes are never sent.
    /// `status` is 0 when the SDK refused it before sending, otherwise the
    /// gateway's answer (`physical_pos.payment.card_data_rejected`). `field`
    /// is the field path (`items[1].cvv`); the value is never kept.
    PhysicalPosCardDataRejected {
        field: Option<String>,
        status: u16,
        request_id: Option<String>,
    },
    /// Physical POS: the outcome of a payment write (start, reversal, cancel)
    /// is unknown — the payment may exist. Raised when an attempt got no HTTP
    /// answer, a 2xx without a readable payment, or a 5xx that does not say
    /// "not started", and then for the whole call whatever a later attempt
    /// answered. Call again with the SAME `idempotency_key` (an existing
    /// payment comes back with `replayed: true`) or look it up with
    /// `list_payments` by `merchantReference`; never use a new key for it.
    /// `status` / `code` are the last HTTP answer's (0 / `None` without one).
    PhysicalPosOutcomeUnknown {
        idempotency_key: String,
        attempts: u32,
        status: u16,
        code: Option<String>,
        request_id: Option<String>,
        retry_after: Option<u64>,
    },
    /// Physical POS: HTTP 429 (payment writes are limited per API key).
    /// `retry_after` is the `Retry-After` header in seconds, or
    /// `details.retryAfterSeconds` when the header is missing.
    PhysicalPosRateLimit {
        code: String,
        request_id: Option<String>,
        retry_after: Option<u64>,
    },
    /// Physical POS: 409 `physical_pos.payment.void_after_settlement` — the
    /// payment is already in a device end-of-day and can no longer be voided.
    /// Return the amount with a `REFUND` reversal (with a new key).
    /// `allowed_operation` is `details.allowedOperation` (`REFUND`).
    PhysicalPosVoidAfterSettlement {
        allowed_operation: Option<String>,
        request_id: Option<String>,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(field) => write!(f, "lunixi: invalid {field}"),
            Self::Api { status, code, .. } => {
                write!(f, "lunixi: API request failed (HTTP {status}, code {code})")
            }
            Self::Transport => f.write_str("lunixi: transport failed"),
            Self::Response(reason) => write!(f, "lunixi: invalid response ({reason})"),
            Self::Webhook => f.write_str("lunixi: webhook verification failed"),
            Self::Inbox => f.write_str("lunixi: webhook persistence failed"),
            Self::PhysicalPosCardDataRejected { field, .. } => write!(
                f,
                "lunixi: physical POS requests must not contain card data (field {}); the card is read on the device",
                field.as_deref().unwrap_or("unknown")
            ),
            Self::PhysicalPosOutcomeUnknown { attempts, .. } => write!(
                f,
                "lunixi: physical POS payment outcome unknown after {attempts} attempt(s); call again with the same idempotency key"
            ),
            Self::PhysicalPosRateLimit { retry_after, .. } => match retry_after {
                Some(seconds) => write!(
                    f,
                    "lunixi: physical POS rate limit (HTTP 429), retry after {seconds} s"
                ),
                None => f.write_str("lunixi: physical POS rate limit (HTTP 429)"),
            },
            Self::PhysicalPosVoidAfterSettlement { .. } => f.write_str(
                "lunixi: physical POS payment is already settled; use a REFUND reversal",
            ),
        }
    }
}
impl std::error::Error for Error {}
impl Error {
    /// `physical_pos.payment.card_data_rejected`.
    pub const PHYSICAL_POS_CARD_DATA_REJECTED_CODE: &str =
        "physical_pos.payment.card_data_rejected";
    /// `physical_pos.payment.void_after_settlement`.
    pub const PHYSICAL_POS_VOID_AFTER_SETTLEMENT_CODE: &str =
        "physical_pos.payment.void_after_settlement";
}
/// SDK result type.
pub type Result<T> = std::result::Result<T, Error>;
