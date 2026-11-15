//! Insurance values, request fields and query filters (`/api/v1/insurance`).

/// Who collects the premium (`premiumCollectedBy`). `PROVIDER` orders cannot
/// be completed through this API yet.
pub mod insurance_premium_collected_by {
    pub const MERCHANT: &str = "MERCHANT";
    pub const PROVIDER: &str = "PROVIDER";
    /// Every value.
    pub const ALL: &[&str] = &[MERCHANT, PROVIDER];
}

/// Status of a quote option. Only a `QUOTED` option that has not expired becomes an order.
pub mod insurance_quote_status {
    pub const REQUESTED: &str = "REQUESTED";
    pub const QUOTED: &str = "QUOTED";
    pub const EXPIRED: &str = "EXPIRED";
    pub const REJECTED: &str = "REJECTED";
    pub const ERROR: &str = "ERROR";
    /// Every status.
    pub const ALL: &[&str] = &[REQUESTED, QUOTED, EXPIRED, REJECTED, ERROR];
}

/// The order's overall status, derived on every read from the four axes.
pub mod insurance_order_status {
    pub const CREATED: &str = "CREATED";
    pub const PROCESSING: &str = "PROCESSING";
    pub const ACTION_REQUIRED: &str = "ACTION_REQUIRED";
    pub const PARTIALLY_ACTIVE: &str = "PARTIALLY_ACTIVE";
    pub const ACTIVE: &str = "ACTIVE";
    pub const FAILED: &str = "FAILED";
    pub const CANCELLED: &str = "CANCELLED";
    pub const REFUNDED: &str = "REFUNDED";
    /// Every status.
    pub const ALL: &[&str] = &[
        CREATED,
        PROCESSING,
        ACTION_REQUIRED,
        PARTIALLY_ACTIVE,
        ACTIVE,
        FAILED,
        CANCELLED,
        REFUNDED,
    ];
}

/// Premium collection axis (`collectionStatus`).
pub mod insurance_collection_status {
    pub const NOT_STARTED: &str = "NOT_STARTED";
    pub const PENDING: &str = "PENDING";
    pub const ACTION_REQUIRED: &str = "ACTION_REQUIRED";
    pub const COLLECTED: &str = "COLLECTED";
    pub const FAILED: &str = "FAILED";
    pub const UNKNOWN: &str = "UNKNOWN";
    /// Every status.
    pub const ALL: &[&str] = &[
        NOT_STARTED,
        PENDING,
        ACTION_REQUIRED,
        COLLECTED,
        FAILED,
        UNKNOWN,
    ];
}

/// Policy axis (`policyStatus`) of an order and of each of its policies.
pub mod insurance_policy_status {
    pub const NOT_REQUESTED: &str = "NOT_REQUESTED";
    pub const PENDING: &str = "PENDING";
    pub const PARTIALLY_ISSUED: &str = "PARTIALLY_ISSUED";
    pub const ISSUED: &str = "ISSUED";
    pub const FAILED_RETRYABLE: &str = "FAILED_RETRYABLE";
    pub const FAILED_FINAL: &str = "FAILED_FINAL";
    pub const UNKNOWN: &str = "UNKNOWN";
    pub const CANCEL_PENDING: &str = "CANCEL_PENDING";
    pub const CANCELLED: &str = "CANCELLED";
    /// Every status.
    pub const ALL: &[&str] = &[
        NOT_REQUESTED,
        PENDING,
        PARTIALLY_ISSUED,
        ISSUED,
        FAILED_RETRYABLE,
        FAILED_FINAL,
        UNKNOWN,
        CANCEL_PENDING,
        CANCELLED,
    ];
}

/// Premium refund axis (`refundStatus`).
pub mod insurance_refund_status {
    pub const NONE: &str = "NONE";
    pub const REQUIRED: &str = "REQUIRED";
    pub const PENDING: &str = "PENDING";
    pub const REFUNDED: &str = "REFUNDED";
    pub const FAILED: &str = "FAILED";
    /// Every status.
    pub const ALL: &[&str] = &[NONE, REQUIRED, PENDING, REFUNDED, FAILED];
}

/// Policy document axis (`documentStatus`); it does not change the overall status.
pub mod insurance_document_status {
    pub const NOT_AVAILABLE: &str = "NOT_AVAILABLE";
    pub const PENDING: &str = "PENDING";
    pub const AVAILABLE: &str = "AVAILABLE";
    pub const FAILED: &str = "FAILED";
    /// Every status.
    pub const ALL: &[&str] = &[NOT_AVAILABLE, PENDING, AVAILABLE, FAILED];
}

/// How the premium of an order is paid (`paymentMode`).
pub mod insurance_payment_mode {
    pub const LUNIXI_PAYMENT: &str = "LUNIXI_PAYMENT";
    pub const EXTERNAL_PAYMENT: &str = "EXTERNAL_PAYMENT";
    pub const NO_PAYMENT: &str = "NO_PAYMENT";
    /// Every mode.
    pub const ALL: &[&str] = &[LUNIXI_PAYMENT, EXTERNAL_PAYMENT, NO_PAYMENT];
}

/// Where an order was created (`surface`).
pub mod insurance_order_surface {
    pub const MERCHANT_API: &str = "MERCHANT_API";
    pub const CHECKOUT_FORM: &str = "CHECKOUT_FORM";
    pub const PANEL: &str = "PANEL";
    /// Every surface.
    pub const ALL: &[&str] = &[MERCHANT_API, CHECKOUT_FORM, PANEL];
}

/// Status of a lead: `PENDING` (the provider call is running), `SUBMITTED`
/// (the provider received it), `UNKNOWN` (the outcome is not known; it is
/// never sent again) or `FAILED`.
pub mod insurance_lead_status {
    pub const PENDING: &str = "PENDING";
    pub const SUBMITTED: &str = "SUBMITTED";
    pub const UNKNOWN: &str = "UNKNOWN";
    pub const FAILED: &str = "FAILED";
    /// Every status.
    pub const ALL: &[&str] = &[PENDING, SUBMITTED, UNKNOWN, FAILED];
}

/// Lunixi reference code lists (`list_reference_values(kind, …)`).
/// `DISTRICT`, `DEVICE_MODEL`, `VEHICLE_MODEL`, `BANK_BRANCH` and
/// `PET_BREED` need `parentCode`.
pub mod insurance_reference_kind {
    pub const COUNTRY: &str = "COUNTRY";
    pub const CITY: &str = "CITY";
    pub const DISTRICT: &str = "DISTRICT";
    pub const DEVICE_BRAND: &str = "DEVICE_BRAND";
    pub const DEVICE_MODEL: &str = "DEVICE_MODEL";
    pub const VEHICLE_BRAND: &str = "VEHICLE_BRAND";
    pub const VEHICLE_MODEL: &str = "VEHICLE_MODEL";
    pub const VEHICLE_TYPE: &str = "VEHICLE_TYPE";
    pub const PRODUCT_CATEGORY: &str = "PRODUCT_CATEGORY";
    pub const VEHICLE_USAGE: &str = "VEHICLE_USAGE";
    pub const LIABILITY_LIMIT: &str = "LIABILITY_LIMIT";
    pub const OCCUPATION: &str = "OCCUPATION";
    pub const PET_BREED: &str = "PET_BREED";
    pub const DWELLING_TYPE: &str = "DWELLING_TYPE";
    pub const DWELLING_FLOOR: &str = "DWELLING_FLOOR";
    pub const BANK: &str = "BANK";
    pub const BANK_BRANCH: &str = "BANK_BRANCH";
    pub const FINANCE_INSTITUTION: &str = "FINANCE_INSTITUTION";
    pub const MEDICAL_PROCEDURE_COVER: &str = "MEDICAL_PROCEDURE_COVER";
    /// Every kind.
    pub const ALL: &[&str] = &[
        COUNTRY,
        CITY,
        DISTRICT,
        DEVICE_BRAND,
        DEVICE_MODEL,
        VEHICLE_BRAND,
        VEHICLE_MODEL,
        VEHICLE_TYPE,
        PRODUCT_CATEGORY,
        VEHICLE_USAGE,
        LIABILITY_LIMIT,
        OCCUPATION,
        PET_BREED,
        DWELLING_TYPE,
        DWELLING_FLOOR,
        BANK,
        BANK_BRANCH,
        FINANCE_INSTITUTION,
        MEDICAL_PROCEDURE_COVER,
    ];
}

/// Insurance webhook event types (subscription `insurance.*`). Dedupe on
/// the event id (`data.eventId` is the same value).
pub mod insurance_events {
    /// The order's policies were issued.
    pub const POLICY_ISSUED: &str = "insurance.policy.issued";
    /// The policy could not be issued or the premium was not collected (`data.errorCode`).
    pub const POLICY_FAILED: &str = "insurance.policy.failed";
    /// The collected premium has to be refunded (`data.errorCode`).
    pub const REFUND_REQUIRED: &str = "insurance.refund.required";
    /// A policy document can be downloaded (`policies[].documentUrl`).
    pub const POLICY_DOCUMENT_READY: &str = "insurance.policy.document_ready";
    /// The provider confirmed the cancellation.
    pub const POLICY_CANCELLED: &str = "insurance.policy.cancelled";
    /// The premium refund completed.
    pub const REFUND_COMPLETED: &str = "insurance.refund.completed";
    /// The insurance provider received a lead (`data` is a lead summary, not an order).
    pub const LEAD_SUBMITTED: &str = "insurance.lead.submitted";
    /// The order events ([`InsuranceWebhookData`](crate::InsuranceWebhookData)).
    pub const ORDER_EVENTS: &[&str] = &[
        POLICY_ISSUED,
        POLICY_FAILED,
        REFUND_REQUIRED,
        POLICY_DOCUMENT_READY,
        POLICY_CANCELLED,
        REFUND_COMPLETED,
    ];
    /// The lead events ([`InsuranceLeadWebhookData`](crate::InsuranceLeadWebhookData)).
    pub const LEAD_EVENTS: &[&str] = &[LEAD_SUBMITTED];
}
/// Subscription pattern of every insurance event.
pub const INSURANCE_EVENT_PATTERN: &str = "insurance.*";
/// Every insurance event type: the order events, then the lead events.
pub const INSURANCE_EVENT_TYPES: &[&str] = {
    use insurance_events::*;
    &[
        POLICY_ISSUED,
        POLICY_FAILED,
        REFUND_REQUIRED,
        POLICY_DOCUMENT_READY,
        POLICY_CANCELLED,
        REFUND_COMPLETED,
        LEAD_SUBMITTED,
    ]
};

/// The type objects of the type body, one per insurance type.
pub const INSURANCE_TYPE_OBJECT_FIELDS: &[&str] = &[
    "tickets",
    "stay",
    "device",
    "product",
    "vehicle",
    "trip",
    "rental",
    "instrument",
    "event",
    "accident",
    "procedure",
    "pet",
    "residence",
    "abroadStay",
];

/// Top-level fields of `create_quote` (the gateway answers any other with
/// 400): the quote fields, the person fields, then the type objects.
pub const INSURANCE_QUOTE_FIELDS: &[&str] = &[
    "insuranceType",
    "schemaVersion",
    "currency",
    "paymentIntentId",
    "context",
    "merchantOrderId",
    "policyholder",
    "insured",
    "tickets",
    "stay",
    "device",
    "product",
    "vehicle",
    "trip",
    "rental",
    "instrument",
    "event",
    "accident",
    "procedure",
    "pet",
    "residence",
    "abroadStay",
];

/// Top-level fields of `create_lead`: the type body without the basket,
/// currency and order fields, plus `shipment`, `company` and `note`.
pub const INSURANCE_LEAD_FIELDS: &[&str] = &[
    "insuranceType",
    "schemaVersion",
    "policyholder",
    "insured",
    "tickets",
    "stay",
    "device",
    "product",
    "vehicle",
    "trip",
    "rental",
    "instrument",
    "event",
    "accident",
    "procedure",
    "pet",
    "residence",
    "abroadStay",
    "shipment",
    "company",
    "note",
];

/// Fields of `search_products`.
pub const INSURANCE_PRODUCT_SEARCH_FIELDS: &[&str] = &["currency", "insuranceType", "context"];
/// Fields of `create_order`.
pub const INSURANCE_ORDER_FIELDS: &[&str] = &["quoteId", "merchantOrderId", "consent"];
/// Fields of `consent` (`create_order`).
pub const INSURANCE_CONSENT_FIELDS: &[&str] = &["termsVersion", "termsSha256", "acceptedAt"];
/// Fields of `confirm_payment` (a premium collected outside Lunixi).
pub const INSURANCE_EXTERNAL_PAYMENT_FIELDS: &[&str] = &[
    "externalPaymentId",
    "paidAmount",
    "paidCurrency",
    "paidAt",
    "paymentProvider",
];
/// Fields of `externalRefund` (`refund_premium` for a premium collected outside Lunixi).
pub const INSURANCE_EXTERNAL_REFUND_FIELDS: &[&str] = &["externalRefundId", "refundedAt"];

/// Query filters each read accepts (the gateway rejects any other with 400).
pub const INSURANCE_FILTERS: &[(&str, &[&str])] = &[
    (
        "listProducts",
        &["currency", "paymentIntentId", "insuranceType"],
    ),
    ("listReferenceValues", &["parentCode", "limit", "cursor"]),
    (
        "listOrders",
        &["limit", "cursor", "paymentIntentId", "merchantOrderId"],
    ),
    (
        "getReconciliationSummary",
        &["from", "to", "connectionId", "insuranceType", "currency"],
    ),
    (
        "listReconciliationOrders",
        &[
            "from",
            "to",
            "connectionId",
            "insuranceType",
            "currency",
            "limit",
            "cursor",
        ],
    ),
    (
        "exportReconciliation",
        &[
            "from",
            "to",
            "connectionId",
            "insuranceType",
            "currency",
            "cursor",
        ],
    ),
    ("listLeads", &["limit", "cursor", "insuranceType", "status"]),
];

/// `Idempotency-Key` of the insurance routes: 8–200 printable ASCII characters, no spaces.
pub const INSURANCE_IDEMPOTENCY_KEY_PATTERN: &str = r"^[\x21-\x7E]{8,200}$";
/// Largest page (`limit` 1–100).
pub const INSURANCE_MAX_LIMIT: i64 = 100;
/// Most basket items one `context.items` may carry.
pub const INSURANCE_MAX_CONTEXT_ITEMS: usize = 100;
/// Orders in one reconciliation CSV file; a longer export ends with a continuation line.
pub const INSURANCE_EXPORT_ORDER_CAP: usize = 20_000;
/// Longest lead `note` (characters).
pub const INSURANCE_LEAD_NOTE_MAX_LENGTH: usize = 1000;

/// The filters of one insurance read (`INSURANCE_FILTERS`).
pub fn insurance_filters(route: &str) -> Option<&'static [&'static str]> {
    INSURANCE_FILTERS
        .iter()
        .find(|(name, _)| *name == route)
        .map(|(_, filters)| *filters)
}
