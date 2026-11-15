//! Physical POS values, request fields and filters (`/api/v1/physical-pos`).
//!
//! Statuses and operations are open sets on the wire: the SDK lists the
//! values the gateway sends today and never rejects one it does not know.
//! The field and filter lists are closed: the gateway rejects an unknown body
//! field or query parameter with 400, so the SDK checks them before sending.

/// Payment operations (`operation`).
pub mod physical_pos_payment_operation {
    /// A sale on the device.
    pub const SALE: &str = "SALE";
    /// Opens a pre-authorisation.
    pub const PREAUTH_OPEN: &str = "PREAUTH_OPEN";
    /// A refund without an original payment.
    pub const REFUND_UNMATCHED: &str = "REFUND_UNMATCHED";
    /// Reversal: void of an unsettled payment.
    pub const VOID: &str = "VOID";
    /// Reversal: refund by amount or by items.
    pub const REFUND: &str = "REFUND";
    /// Reversal: closes a pre-authorisation.
    pub const PREAUTH_CLOSE: &str = "PREAUTH_CLOSE";
    /// Reversal: cancels a pre-authorisation.
    pub const PREAUTH_CANCEL: &str = "PREAUTH_CANCEL";
    /// Every operation, in documented order.
    pub const ALL: &[&str] = &[
        SALE,
        PREAUTH_OPEN,
        REFUND_UNMATCHED,
        VOID,
        REFUND,
        PREAUTH_CLOSE,
        PREAUTH_CANCEL,
    ];
}

/// Payment statuses (`paymentStatus`).
pub mod physical_pos_payment_status {
    /// Accepted; the start command has not reached the device yet.
    pub const NOT_STARTED: &str = "NOT_STARTED";
    /// Running on the device.
    pub const IN_PROGRESS: &str = "IN_PROGRESS";
    /// Approved.
    pub const APPROVED: &str = "APPROVED";
    /// Approved for part of the amount.
    pub const PARTIALLY_APPROVED: &str = "PARTIALLY_APPROVED";
    /// Declined.
    pub const DECLINED: &str = "DECLINED";
    /// Cancelled.
    pub const CANCELLED: &str = "CANCELLED";
    /// Expired.
    pub const EXPIRED: &str = "EXPIRED";
    /// Failed.
    pub const FAILED: &str = "FAILED";
    /// The device outcome is unknown.
    pub const UNKNOWN: &str = "UNKNOWN";
    /// Under manual review (resolved in the panel).
    pub const MANUAL_REVIEW: &str = "MANUAL_REVIEW";
    /// Every status, in documented order.
    pub const ALL: &[&str] = &[
        NOT_STARTED,
        IN_PROGRESS,
        APPROVED,
        PARTIALLY_APPROVED,
        DECLINED,
        CANCELLED,
        EXPIRED,
        FAILED,
        UNKNOWN,
        MANUAL_REVIEW,
    ];
}

/// Reversal statuses of a payment (`reversalStatus`).
pub mod physical_pos_reversal_status {
    /// Not reversed.
    pub const NONE: &str = "NONE";
    /// Partly refunded.
    pub const PARTIALLY_REFUNDED: &str = "PARTIALLY_REFUNDED";
    /// Fully refunded.
    pub const REFUNDED: &str = "REFUNDED";
    /// Voided.
    pub const VOIDED: &str = "VOIDED";
    /// Pre-authorisation closed.
    pub const PREAUTH_CLOSED: &str = "PREAUTH_CLOSED";
    /// Pre-authorisation cancelled.
    pub const PREAUTH_CANCELLED: &str = "PREAUTH_CANCELLED";
    /// Pre-authorisation expired.
    pub const PREAUTH_EXPIRED: &str = "PREAUTH_EXPIRED";
    /// Every status, in documented order.
    pub const ALL: &[&str] = &[
        NONE,
        PARTIALLY_REFUNDED,
        REFUNDED,
        VOIDED,
        PREAUTH_CLOSED,
        PREAUTH_CANCELLED,
        PREAUTH_EXPIRED,
    ];
}

/// Webhook event types. `physical_pos.payment.created` and `.in_progress`
/// are internal and never delivered. Reversals (void, refund,
/// pre-authorisation close / cancel) use the same payment events; tell them
/// apart with `data.operation` and `data.rootPaymentId`, and the original
/// payment's source with `data.original.origin`.
///
/// `PAYMENT_DEVICE_SALE` reports a sale made directly on the device
/// (`origin: "DEVICE_DIRECT"`): it arrives approved, with
/// `deviceOccurredAt`; no `approved` event is sent for it.
///
/// End-of-day events (`physical_pos.settlement.*`) have their own payloads
/// and carry no `historySeq`; deduplicate them by the event id.
pub mod physical_pos_events {
    pub const PAYMENT_APPROVED: &str = "physical_pos.payment.approved";
    pub const PAYMENT_PARTIALLY_APPROVED: &str = "physical_pos.payment.partially_approved";
    pub const PAYMENT_DECLINED: &str = "physical_pos.payment.declined";
    pub const PAYMENT_CANCELLED: &str = "physical_pos.payment.cancelled";
    pub const PAYMENT_EXPIRED: &str = "physical_pos.payment.expired";
    pub const PAYMENT_FAILED: &str = "physical_pos.payment.failed";
    pub const PAYMENT_UNKNOWN: &str = "physical_pos.payment.unknown";
    pub const PAYMENT_MANUAL_REVIEW: &str = "physical_pos.payment.manual_review";
    pub const PAYMENT_DEVICE_SALE: &str = "physical_pos.payment.device_sale";
    pub const FISCAL_DOCUMENT_ISSUED: &str = "physical_pos.fiscal_document.issued";
    pub const FISCAL_DOCUMENT_FAILED: &str = "physical_pos.fiscal_document.failed";
    pub const FISCAL_DOCUMENT_CANCELLED: &str = "physical_pos.fiscal_document.cancelled";
    pub const FISCAL_DOCUMENT_ACTION_REQUIRED: &str =
        "physical_pos.fiscal_document.action_required";
    pub const SETTLEMENT_COMPLETED: &str = "physical_pos.settlement.completed";
    pub const SETTLEMENT_FAILED: &str = "physical_pos.settlement.failed";
    pub const SETTLEMENT_OVERDUE: &str = "physical_pos.settlement.overdue";
    pub const SETTLEMENT_SKIPPED: &str = "physical_pos.settlement.skipped";
}

/// Webhook subscription pattern of every physical POS event.
pub const PHYSICAL_POS_EVENT_PATTERN: &str = "physical_pos.*";

/// Every delivered physical POS event type, in documented order.
pub const PHYSICAL_POS_EVENT_TYPES: &[&str] = {
    use physical_pos_events::*;
    &[
        PAYMENT_APPROVED,
        PAYMENT_PARTIALLY_APPROVED,
        PAYMENT_DECLINED,
        PAYMENT_CANCELLED,
        PAYMENT_EXPIRED,
        PAYMENT_FAILED,
        PAYMENT_UNKNOWN,
        PAYMENT_MANUAL_REVIEW,
        PAYMENT_DEVICE_SALE,
        FISCAL_DOCUMENT_ISSUED,
        FISCAL_DOCUMENT_FAILED,
        FISCAL_DOCUMENT_CANCELLED,
        FISCAL_DOCUMENT_ACTION_REQUIRED,
        SETTLEMENT_COMPLETED,
        SETTLEMENT_FAILED,
        SETTLEMENT_OVERDUE,
        SETTLEMENT_SKIPPED,
    ]
};

/// Largest page size (`limit`).
pub const PHYSICAL_POS_MAX_LIMIT: i64 = 100;
/// Largest `rowVersion` / `expectedRowVersion`.
pub const PHYSICAL_POS_ROW_VERSION_MAX: i64 = 2_147_483_647;
/// Shortest `Idempotency-Key` (printable ASCII without spaces).
pub const PHYSICAL_POS_IDEMPOTENCY_KEY_MIN: usize = 8;
/// Longest `Idempotency-Key`.
pub const PHYSICAL_POS_IDEMPOTENCY_KEY_MAX: usize = 200;

/// 5xx codes after which the gateway guarantees nothing was written (the
/// payment did not start). Any other 5xx, and a request without an HTTP
/// answer, leaves the outcome of a payment write unknown.
pub const PHYSICAL_POS_NOT_STARTED_CODES: &[&str] = &[
    "physical_pos.presence.unavailable",
    "physical_pos.signing.unavailable",
    "physical_pos.service.busy",
    "physical_pos.service.clock_skew",
    "physical_pos.service.not_configured",
];

/// Device selection of a payment: exactly one of `deviceId`,
/// `vendorCode` + `serialNumber`, or `externalRef`.
pub const PHYSICAL_POS_PAYMENT_TARGET_FIELDS: &[&str] =
    &["deviceId", "vendorCode", "serialNumber", "externalRef"];
/// Fields of a sale item (`createPayment` `items[]`).
pub const PHYSICAL_POS_SALE_ITEM_FIELDS: &[&str] = &[
    "lineNo",
    "name",
    "quantityMilli",
    "unitCode",
    "unitPriceMinor",
    "taxGroupCode",
    "lineTotalMinor",
    "barcode",
];
/// Fields of a refund item (`createReversal` `items[]`).
pub const PHYSICAL_POS_REFUND_ITEM_FIELDS: &[&str] = &["lineNo", "quantityMilli", "amountMinor"];
/// Filters scanned for card numbers.
pub const PHYSICAL_POS_FREE_TEXT_FILTERS: &[&str] = &["merchantReference", "q"];

/// One request body: `(field, kind)`. Kinds: `string`, `int`, `money`,
/// `bool`, `strings` (array of strings), `rowVersion`, `target`,
/// `saleItems`, `refundItems`. A `*` suffix marks a required field; `?`
/// allows `null` (an explicit clear).
pub type PhysicalPosFieldKinds = &'static [(&'static str, &'static str)];

/// Request bodies per operation (`createPayment`, `updateStore`,
/// `statusChange` for archive / restore / close / retire …).
pub const PHYSICAL_POS_REQUEST_FIELDS: &[(&str, PhysicalPosFieldKinds)] = &[
    (
        "createPayment",
        &[
            ("operation", "string*"),
            ("target", "target*"),
            ("amountMinor", "money*"),
            ("currency", "string*"),
            ("installmentCount", "int?"),
            ("instrument", "string?"),
            ("merchantReference", "string?"),
            ("allowDuplicateReference", "bool"),
            ("startTimeoutSeconds", "int?"),
            ("terminalAcquirerId", "string?"),
            ("items", "saleItems"),
            ("interaction", "string?"),
            ("terminalProviderId", "string?"),
        ],
    ),
    (
        "createReversal",
        &[
            ("operation", "string*"),
            ("amountMinor", "money"),
            ("currency", "string*"),
            ("merchantReference", "string?"),
            ("startTimeoutSeconds", "int?"),
            ("target", "target?"),
            ("items", "refundItems"),
        ],
    ),
    (
        "createStore",
        &[
            ("code", "string*"),
            ("name", "string*"),
            ("city", "string"),
            ("district", "string"),
            ("addressLine", "string"),
            ("timezone", "string"),
            ("externalRef", "string"),
        ],
    ),
    (
        "updateStore",
        &[
            ("rowVersion", "rowVersion*"),
            ("name", "string"),
            ("city", "string?"),
            ("district", "string?"),
            ("addressLine", "string?"),
            ("timezone", "string"),
            ("externalRef", "string?"),
        ],
    ),
    (
        "statusChange",
        &[("rowVersion", "rowVersion*"), ("reasonCode", "string*")],
    ),
    (
        "createRegister",
        &[("code", "string*"), ("name", "string*")],
    ),
    (
        "updateRegister",
        &[("rowVersion", "rowVersion*"), ("name", "string")],
    ),
    (
        "createTerminal",
        &[
            ("code", "string*"),
            ("displayName", "string*"),
            ("vendorCode", "string"),
            ("virtualSerial", "string"),
            ("vendorTerminalRef", "string"),
        ],
    ),
    (
        "updateTerminal",
        &[
            ("rowVersion", "rowVersion*"),
            ("displayName", "string"),
            ("vendorCode", "string"),
        ],
    ),
    (
        "addTerminalAcquirer",
        &[
            ("vendorAcquirerRef", "string*"),
            ("merchantNumber", "string*"),
            ("terminalNumber", "string*"),
            ("label", "string"),
            ("currencies", "strings*"),
        ],
    ),
    (
        "updateTerminalAcquirer",
        &[
            ("rowVersion", "rowVersion*"),
            ("label", "string?"),
            ("currencies", "strings"),
            ("status", "string"),
        ],
    ),
    (
        "addTerminalProvider",
        &[
            ("vendorAccountId", "string*"),
            ("merchantCode", "string*"),
            ("terminalCode", "string*"),
            ("productCode", "string"),
            ("label", "string"),
        ],
    ),
    (
        "updateTerminalProvider",
        &[
            ("rowVersion", "rowVersion*"),
            ("label", "string?"),
            ("status", "string"),
        ],
    ),
    (
        "changeTerminalDevice",
        &[
            ("rowVersion", "rowVersion*"),
            ("deviceId", "string*"),
            ("reasonCode", "string*"),
        ],
    ),
    (
        "createDevice",
        &[
            ("vendorCode", "string*"),
            ("deviceModelId", "string*"),
            ("serialNumber", "string*"),
            ("displayName", "string"),
            ("externalRef", "string"),
            ("storeId", "string"),
            ("registerId", "string"),
            ("integrationChannel", "string*"),
            ("channelVariant", "string"),
            ("appMode", "string"),
        ],
    ),
    (
        "updateDevice",
        &[
            ("rowVersion", "rowVersion*"),
            ("displayName", "string?"),
            ("externalRef", "string?"),
            ("integrationChannel", "string"),
            ("channelVariant", "string?"),
            ("appMode", "string?"),
        ],
    ),
    (
        "assignDevice",
        &[
            ("rowVersion", "rowVersion*"),
            ("storeId", "string*"),
            ("registerId", "string?"),
            ("reasonCode", "string*"),
        ],
    ),
    (
        "changeDeviceLifecycle",
        &[
            ("rowVersion", "rowVersion*"),
            ("target", "string*"),
            ("reasonCode", "string*"),
        ],
    ),
    (
        "revokeDeviceKey",
        &[("rowVersion", "rowVersion*"), ("reasonCode", "string*")],
    ),
    ("revertDeviceEnrollment", &[("reasonCode", "string*")]),
    ("revokeActivationCode", &[("reasonCode", "string*")]),
    (
        "setSettlementPolicy",
        &[
            ("mode", "string*"),
            ("scheduleTime", "string"),
            ("timeZone", "string"),
            ("scheduleDays", "strings"),
            ("retryWindowMinutes", "int"),
            ("retryIntervalMinutes", "int"),
            ("offlineBehavior", "string"),
            ("overdueAlertHours", "int"),
            ("expectedRowVersion", "rowVersion"),
        ],
    ),
];

/// Query parameters per route; anything else is rejected before sending.
/// `history` is the filter set of every audit, link, assignment and key list.
pub const PHYSICAL_POS_FILTERS: &[(&str, &[&str])] = &[
    (
        "listPayments",
        &[
            "limit",
            "cursor",
            "deviceId",
            "paymentStatus",
            "createdFrom",
            "createdTo",
            "merchantReference",
        ],
    ),
    (
        "listTransactions",
        &[
            "limit",
            "cursor",
            "deviceId",
            "storeId",
            "registerId",
            "paymentStatus",
            "createdFrom",
            "createdTo",
            "merchantReference",
            "amountFromMinor",
            "amountToMinor",
            "origin",
            "operation",
        ],
    ),
    (
        "listAmbiguousPayments",
        &["limit", "cursor", "deviceId", "paymentStatus"],
    ),
    (
        "getSalesSummary",
        &["dateFrom", "dateTo", "deviceId", "storeId", "groupBy"],
    ),
    (
        "getTaxSummary",
        &["dateFrom", "dateTo", "deviceId", "storeId"],
    ),
    (
        "listSettlements",
        &[
            "limit",
            "cursor",
            "deviceId",
            "lineStatus",
            "createdFrom",
            "createdTo",
        ],
    ),
    (
        "listSettlementRuns",
        &[
            "limit",
            "cursor",
            "includeTotal",
            "deviceId",
            "source",
            "runStatus",
            "createdFrom",
            "createdTo",
        ],
    ),
    ("listDeviceModels", &["vendorCode", "deviceType", "status"]),
    (
        "listStores",
        &["limit", "cursor", "includeTotal", "status", "q"],
    ),
    (
        "listRegisters",
        &["limit", "cursor", "includeTotal", "status"],
    ),
    (
        "listTerminals",
        &["limit", "cursor", "includeTotal", "status", "q"],
    ),
    (
        "listTerminalAcquirers",
        &["limit", "cursor", "includeTotal"],
    ),
    (
        "listTerminalProviders",
        &["limit", "cursor", "includeTotal"],
    ),
    (
        "listDevices",
        &[
            "limit",
            "cursor",
            "includeTotal",
            "storeId",
            "registerId",
            "vendorCode",
            "deviceModelId",
            "deviceType",
            "lifecycleStatus",
            "ownershipStatus",
            "integrationChannel",
            "q",
        ],
    ),
    ("history", &["limit", "cursor"]),
];

/// The field kinds of one request body (`PHYSICAL_POS_REQUEST_FIELDS`).
pub fn physical_pos_request_fields(operation: &str) -> Option<PhysicalPosFieldKinds> {
    PHYSICAL_POS_REQUEST_FIELDS
        .iter()
        .find(|(name, _)| *name == operation)
        .map(|(_, fields)| *fields)
}

/// The allowed query parameters of one route (`PHYSICAL_POS_FILTERS`).
pub fn physical_pos_filters(route: &str) -> Option<&'static [&'static str]> {
    PHYSICAL_POS_FILTERS
        .iter()
        .find(|(name, _)| *name == route)
        .map(|(_, filters)| *filters)
}
