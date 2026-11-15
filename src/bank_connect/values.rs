//! Bank accounts, transfers and currency exchange (`/api/v1/bank-connect`):
//! value sets, filters and the transfer route table.

use crate::route_table::Route;

/// Status of a bank connection. Data is read only while a connection is
/// `ACTIVE` or `DEGRADED` (`DEGRADED`: the last reads from the bank failed,
/// so the data may be older; see the account's `freshness`).
pub mod bank_connection_status {
    /// Documents are awaited.
    pub const PENDING_DOCUMENTS: &str = "PENDING_DOCUMENTS";
    /// Under review.
    pub const UNDER_REVIEW: &str = "UNDER_REVIEW";
    /// Being set up.
    pub const PENDING_SETUP: &str = "PENDING_SETUP";
    /// Active.
    pub const ACTIVE: &str = "ACTIVE";
    /// The last reads failed; data may be older.
    pub const DEGRADED: &str = "DEGRADED";
    /// Suspended.
    pub const SUSPENDED: &str = "SUSPENDED";
    /// Disconnected.
    pub const DISCONNECTED: &str = "DISCONNECTED";
    /// Every status.
    pub const ALL: &[&str] = &[
        PENDING_DOCUMENTS,
        UNDER_REVIEW,
        PENDING_SETUP,
        ACTIVE,
        DEGRADED,
        SUSPENDED,
        DISCONNECTED,
    ];
}

/// Status of a connected account.
pub mod bank_account_status {
    /// Active.
    pub const ACTIVE: &str = "ACTIVE";
    /// Inactive.
    pub const INACTIVE: &str = "INACTIVE";
    /// Every status.
    pub const ALL: &[&str] = &[ACTIVE, INACTIVE];
}

/// Money in (`CREDIT`) or out (`DEBIT`). `amountMinor` is always positive.
pub mod bank_transaction_direction {
    /// Money in.
    pub const CREDIT: &str = "CREDIT";
    /// Money out.
    pub const DEBIT: &str = "DEBIT";
    /// Every direction.
    pub const ALL: &[&str] = &[CREDIT, DEBIT];
}

/// Lunixi's bank-independent transaction type (`canonicalType`).
pub mod bank_transaction_type {
    pub const TRANSFER_IN: &str = "TRANSFER_IN";
    pub const TRANSFER_OUT: &str = "TRANSFER_OUT";
    pub const INTERNAL_IN: &str = "INTERNAL_IN";
    pub const INTERNAL_OUT: &str = "INTERNAL_OUT";
    pub const POS_SETTLEMENT: &str = "POS_SETTLEMENT";
    pub const POS_SETTLEMENT_REFUND: &str = "POS_SETTLEMENT_REFUND";
    pub const FEE: &str = "FEE";
    pub const TAX: &str = "TAX";
    pub const CASH_IN: &str = "CASH_IN";
    pub const CASH_OUT: &str = "CASH_OUT";
    pub const CARD: &str = "CARD";
    pub const LOAN: &str = "LOAN";
    pub const FX: &str = "FX";
    pub const INTEREST: &str = "INTEREST";
    pub const OTHER: &str = "OTHER";
    /// Every type.
    pub const ALL: &[&str] = &[
        TRANSFER_IN,
        TRANSFER_OUT,
        INTERNAL_IN,
        INTERNAL_OUT,
        POS_SETTLEMENT,
        POS_SETTLEMENT_REFUND,
        FEE,
        TAX,
        CASH_IN,
        CASH_OUT,
        CARD,
        LOAN,
        FX,
        INTEREST,
        OTHER,
    ];
}

/// Whether a bank can be connected (connections are started in the Lunixi dashboard).
pub mod bank_availability {
    /// The bank can be connected.
    pub const AVAILABLE: &str = "AVAILABLE";
    /// Not yet.
    pub const COMING_SOON: &str = "COMING_SOON";
    /// Every value.
    pub const ALL: &[&str] = &[AVAILABLE, COMING_SOON];
}

/// What a bank or a connection offers (`capabilities`). `OUTGOING_TRANSFER`
/// and `FX_EXCHANGE` open `bank_connect().transfers()` and `fx()`.
pub mod bank_capability {
    pub const ACCOUNT_LIST: &str = "ACCOUNT_LIST";
    pub const BALANCE: &str = "BALANCE";
    pub const TRANSACTIONS: &str = "TRANSACTIONS";
    pub const OUTGOING_TRANSFER: &str = "OUTGOING_TRANSFER";
    pub const FX_EXCHANGE: &str = "FX_EXCHANGE";
    /// Every capability.
    pub const ALL: &[&str] = &[
        ACCOUNT_LIST,
        BALANCE,
        TRANSACTIONS,
        OUTGOING_TRANSFER,
        FX_EXCHANGE,
    ];
}

/// `syncStatus` of `request_sync`: `QUEUED` (now or already waiting) or
/// `SCHEDULED` (at `scheduledAt`).
pub mod bank_sync_status {
    pub const QUEUED: &str = "QUEUED";
    pub const SCHEDULED: &str = "SCHEDULED";
    /// Every status.
    pub const ALL: &[&str] = &[QUEUED, SCHEDULED];
}

/// Bank-account webhook event types (same signed envelope as every Lunixi
/// webhook).
pub mod bank_connect_events {
    /// A new transaction (CREDIT or DEBIT) was read from a connected account.
    /// Sent once per transaction seen by a regular read after the connection
    /// became active; the event id is the same on every redelivery.
    pub const TRANSACTION_RECEIVED: &str = "bank_account.transaction.received";
}
/// Subscription pattern of every bank-account event.
pub const BANK_CONNECT_EVENT_PATTERN: &str = "bank_account.*";
/// Every bank-account event type.
pub const BANK_CONNECT_EVENT_TYPES: &[&str] = &[bank_connect_events::TRANSACTION_RECEIVED];

/// Status of an outgoing transfer. `PENDING_OTP` only for dashboard
/// transfers; `COMPLETED` can still become `RETURNED`; `FAILED`,
/// `RETURNED` and `CANCELED` are final.
pub mod bank_transfer_status {
    pub const PENDING_OTP: &str = "PENDING_OTP";
    pub const SUBMITTED: &str = "SUBMITTED";
    pub const PROCESSING: &str = "PROCESSING";
    pub const COMPLETED: &str = "COMPLETED";
    pub const FAILED: &str = "FAILED";
    pub const RETURNED: &str = "RETURNED";
    pub const CANCELED: &str = "CANCELED";
    /// Every status.
    pub const ALL: &[&str] = &[
        PENDING_OTP,
        SUBMITTED,
        PROCESSING,
        COMPLETED,
        FAILED,
        RETURNED,
        CANCELED,
    ];
}

/// The rail the bank chose: own accounts, same bank, FAST, EFT; `UNKNOWN` until it says.
pub mod bank_transfer_rail {
    pub const INTERNAL: &str = "INTERNAL";
    pub const INTRABANK: &str = "INTRABANK";
    pub const FAST: &str = "FAST";
    pub const EFT: &str = "EFT";
    pub const UNKNOWN: &str = "UNKNOWN";
    /// Every rail.
    pub const ALL: &[&str] = &[INTERNAL, INTRABANK, FAST, EFT, UNKNOWN];
}

/// Where a transfer or exchange order was started.
pub mod bank_transfer_initiated_via {
    pub const PANEL: &str = "PANEL";
    pub const API: &str = "API";
    /// Every value.
    pub const ALL: &[&str] = &[PANEL, API];
}

/// Status of a currency exchange order (created `SUBMITTED`).
pub mod bank_fx_order_status {
    pub const SUBMITTED: &str = "SUBMITTED";
    pub const COMPLETED: &str = "COMPLETED";
    pub const FAILED: &str = "FAILED";
    /// Every status.
    pub const ALL: &[&str] = &[SUBMITTED, COMPLETED, FAILED];
}

/// Which amount of a quote you fixed: `SOURCE` (sell) or `TARGET` (buy).
pub mod bank_fx_requested_side {
    pub const SOURCE: &str = "SOURCE";
    pub const TARGET: &str = "TARGET";
    /// Every side.
    pub const ALL: &[&str] = &[SOURCE, TARGET];
}

/// Bank Connect transfer webhook event types (`bank_transfer.*`; distinct
/// from the payment events in
/// [`payment_bank_transfer_events`](crate::payment_bank_transfer_events)).
/// One event per transfer and type; the event id is the same on every
/// redelivery.
pub mod bank_transfer_events {
    /// A transfer was created (API: `SUBMITTED`; dashboard: `PENDING_OTP`).
    pub const CREATED: &str = "bank_transfer.created";
    /// The bank completed the transfer.
    pub const COMPLETED: &str = "bank_transfer.completed";
    /// The transfer failed; `failureCode` says why.
    pub const FAILED: &str = "bank_transfer.failed";
    /// The receiving bank sent a completed transfer back.
    pub const RETURNED: &str = "bank_transfer.returned";
}
/// Subscription pattern of every transfer event.
pub const BANK_TRANSFER_EVENT_PATTERN: &str = "bank_transfer.*";
/// Every transfer event type.
pub const BANK_TRANSFER_EVENT_TYPES: &[&str] = &[
    bank_transfer_events::CREATED,
    bank_transfer_events::COMPLETED,
    bank_transfer_events::FAILED,
    bank_transfer_events::RETURNED,
];

/// Transfer and exchange routes: method, path, body fields, query fields
/// and the `Idempotency-Key` rule. Every route is signed and needs
/// `bank-connect:transfer` plus a key with an IP allowlist.
pub const BANK_TRANSFER_ROUTES: &[Route] = &[
    Route::new(
        "listTransferPurposes",
        "GET",
        "/api/v1/bank-connect/transfer-purposes",
    )
    .with_query(&["connectionId"]),
    Route::new("createTransfer", "POST", "/api/v1/bank-connect/transfers")
        .with_key()
        .with_body(
            &["accountId", "receiverIban", "receiverName", "amountMinor"],
            &["currency", "paymentPurpose", "description"],
        ),
    Route::new("listTransfers", "GET", "/api/v1/bank-connect/transfers").with_query(&[
        "limit",
        "cursor",
        "accountId",
        "status",
        "createdFrom",
        "createdTo",
    ]),
    Route::new(
        "getTransfer",
        "GET",
        "/api/v1/bank-connect/transfers/:transferId",
    ),
    Route::new("listFxRates", "GET", "/api/v1/bank-connect/fx/rates").with_query(&["connectionId"]),
    Route::new("createFxQuote", "POST", "/api/v1/bank-connect/fx/quotes").with_body(
        &["sourceAccountId", "targetAccountId"],
        &["sourceAmountMinor", "targetAmountMinor"],
    ),
    Route::new("createFxOrder", "POST", "/api/v1/bank-connect/fx/orders")
        .with_key()
        .with_body(&["quoteId"], &["description"]),
    Route::new("listFxOrders", "GET", "/api/v1/bank-connect/fx/orders")
        .with_query(&["limit", "cursor", "status"]),
    Route::new(
        "getFxOrder",
        "GET",
        "/api/v1/bank-connect/fx/orders/:orderId",
    ),
];

/// `Idempotency-Key` of transfers and exchange orders: 8–200 printable ASCII characters.
pub const BANK_TRANSFER_IDEMPOTENCY_KEY_PATTERN: &str = r"^[\x21-\x7E]{8,200}$";
/// `paymentPurpose`: a code from `list_purposes`.
pub const BANK_TRANSFER_PAYMENT_PURPOSE_PATTERN: &str = r"^[A-Za-z0-9_]{1,64}$";
/// Longest `description` of a transfer or an exchange order (characters).
pub const BANK_TRANSFER_DESCRIPTION_MAX: usize = 140;
/// Shortest `receiverName` (characters).
pub const BANK_TRANSFER_RECEIVER_NAME_MIN: usize = 2;
/// Longest `receiverName` (characters).
pub const BANK_TRANSFER_RECEIVER_NAME_MAX: usize = 140;
/// Longest `receiverIban` (spaces included); the service checks the TR IBAN.
pub const BANK_TRANSFER_IBAN_MAX: usize = 42;

/// Query filters each bank-account read accepts (the gateway rejects any
/// other with 400).
pub const BANK_CONNECT_FILTERS: &[(&str, &[&str])] = &[
    ("listBanks", &["limit", "cursor"]),
    ("listConnections", &["limit", "cursor", "status", "bankId"]),
    (
        "listAccounts",
        &[
            "limit",
            "cursor",
            "connectionId",
            "bankId",
            "currency",
            "status",
        ],
    ),
    (
        "listBalances",
        &["limit", "cursor", "accountId", "currency"],
    ),
    (
        "listTransactions",
        &[
            "limit",
            "cursor",
            "accountId",
            "bookedFrom",
            "bookedTo",
            "direction",
            "minAmountMinor",
            "maxAmountMinor",
            "canonicalType",
        ],
    ),
];

/// Largest page (`limit` 1–100; the gateway's default is 25).
pub const BANK_CONNECT_MAX_LIMIT: i64 = 100;
/// Most account ids one `accountId` filter may carry.
pub const BANK_CONNECT_MAX_ACCOUNT_FILTER: usize = 100;
/// Widest `bookedFrom` … `bookedTo` window of one transaction request, in days.
pub const BANK_CONNECT_BOOKED_RANGE_MAX_DAYS: i64 = 92;

/// The filters of one bank-account read (`BANK_CONNECT_FILTERS`).
pub fn bank_connect_filters(route: &str) -> Option<&'static [&'static str]> {
    BANK_CONNECT_FILTERS
        .iter()
        .find(|(name, _)| *name == route)
        .map(|(_, filters)| *filters)
}
