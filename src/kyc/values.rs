//! KYC / KYB / evidence values and the route table (`/api/v1/kyc`).

use crate::route_table::Route;

/// Session types. `EVIDENCE` is proof-of-X (address, funds, card ownership
/// …), not only onboarding.
pub mod kyc_session_type {
    /// A person's identity.
    pub const KYC: &str = "KYC";
    /// A company and its beneficial owners.
    pub const KYB: &str = "KYB";
    /// Evidence (proof of address, funds, ownership …).
    pub const EVIDENCE: &str = "EVIDENCE";
    /// Every type.
    pub const ALL: &[&str] = &[KYC, KYB, EVIDENCE];
}

/// Session status.
pub mod kyc_session_status {
    pub const CREATED: &str = "CREATED";
    pub const INITIATED: &str = "INITIATED";
    pub const DOCUMENT_SUBMITTED: &str = "DOCUMENT_SUBMITTED";
    pub const PROCESSING: &str = "PROCESSING";
    pub const AWAITING_PROVIDER_CALLBACK: &str = "AWAITING_PROVIDER_CALLBACK";
    pub const AWAITING_ADDITIONAL_DOCUMENT: &str = "AWAITING_ADDITIONAL_DOCUMENT";
    pub const AWAITING_USER_ACTION: &str = "AWAITING_USER_ACTION";
    pub const UNDER_REVIEW: &str = "UNDER_REVIEW";
    pub const PENDING_ADDITIONAL_INFO: &str = "PENDING_ADDITIONAL_INFO";
    pub const RESCREEN_REQUIRED: &str = "RESCREEN_REQUIRED";
    pub const REOPENED: &str = "REOPENED";
    pub const APPROVED: &str = "APPROVED";
    pub const REJECTED: &str = "REJECTED";
    pub const MANUAL_REVIEW: &str = "MANUAL_REVIEW";
    pub const EXPIRED: &str = "EXPIRED";
    pub const ABANDONED: &str = "ABANDONED";
    pub const CANCELLED: &str = "CANCELLED";
    /// Every status.
    pub const ALL: &[&str] = &[
        CREATED,
        INITIATED,
        DOCUMENT_SUBMITTED,
        PROCESSING,
        AWAITING_PROVIDER_CALLBACK,
        AWAITING_ADDITIONAL_DOCUMENT,
        AWAITING_USER_ACTION,
        UNDER_REVIEW,
        PENDING_ADDITIONAL_INFO,
        RESCREEN_REQUIRED,
        REOPENED,
        APPROVED,
        REJECTED,
        MANUAL_REVIEW,
        EXPIRED,
        ABANDONED,
        CANCELLED,
    ];
    /// Terminal states.
    pub const TERMINAL_STATES: &[&str] = &[
        APPROVED,
        REJECTED,
        MANUAL_REVIEW,
        EXPIRED,
        ABANDONED,
        CANCELLED,
    ];
}

/// Terminal session states (`APPROVED`, `REJECTED`, `MANUAL_REVIEW`,
/// `EXPIRED`, `ABANDONED`, `CANCELLED`).
pub const KYC_SESSION_TERMINAL_STATES: &[&str] = kyc_session_status::TERMINAL_STATES;

/// `evidencePurpose` of an `EVIDENCE` session.
pub mod kyc_evidence_purpose {
    pub const IDENTITY_VERIFICATION: &str = "IDENTITY_VERIFICATION";
    pub const BUSINESS_VERIFICATION: &str = "BUSINESS_VERIFICATION";
    pub const PROOF_OF_FUNDS: &str = "PROOF_OF_FUNDS";
    pub const PROOF_OF_ADDRESS: &str = "PROOF_OF_ADDRESS";
    pub const SOURCE_OF_FUNDS: &str = "SOURCE_OF_FUNDS";
    pub const SOURCE_OF_WEALTH: &str = "SOURCE_OF_WEALTH";
    pub const TRANSACTION_EVIDENCE: &str = "TRANSACTION_EVIDENCE";
    pub const CARD_OWNERSHIP: &str = "CARD_OWNERSHIP";
    pub const PAYMENT_METHOD_OWNERSHIP: &str = "PAYMENT_METHOD_OWNERSHIP";
    pub const PAYOUT_EVIDENCE: &str = "PAYOUT_EVIDENCE";
    pub const MERCHANT_ONBOARDING: &str = "MERCHANT_ONBOARDING";
    /// Every purpose.
    pub const ALL: &[&str] = &[
        IDENTITY_VERIFICATION,
        BUSINESS_VERIFICATION,
        PROOF_OF_FUNDS,
        PROOF_OF_ADDRESS,
        SOURCE_OF_FUNDS,
        SOURCE_OF_WEALTH,
        TRANSACTION_EVIDENCE,
        CARD_OWNERSHIP,
        PAYMENT_METHOD_OWNERSHIP,
        PAYOUT_EVIDENCE,
        MERCHANT_ONBOARDING,
    ];
}

/// Assurance ladder (rank-aligned with NIST 800-63 IAL / eIDAS LoA). A gate
/// passes when achieved ≥ required ([`kyc_assurance_meets`]).
pub mod kyc_assurance_level {
    pub const NONE: &str = "NONE";
    pub const NOT_PERFORMED: &str = "NOT_PERFORMED";
    pub const LOW: &str = "LOW";
    pub const SUBSTANTIAL: &str = "SUBSTANTIAL";
    pub const HIGH: &str = "HIGH";
    /// Every level.
    pub const ALL: &[&str] = &[NONE, NOT_PERFORMED, LOW, SUBSTANTIAL, HIGH];
}

/// Domains of the achieved assurance snapshot
/// (`session.decision.assuranceSnapshot.achieved`), each an assurance level.
pub mod kyc_assurance_domain {
    pub const IDENTITY: &str = "identity_evidence_strength";
    pub const DOCUMENT: &str = "document_authenticity_level";
    pub const BIOMETRIC: &str = "possession_biometric_binding";
    pub const AUTHORITATIVE: &str = "authoritative_source_level";
    pub const ADDRESS: &str = "address_assurance";
    pub const AML: &str = "aml_freshness";
    pub const HUMAN_REVIEW: &str = "human_review_level";
    /// Every domain.
    pub const ALL: &[&str] = &[
        IDENTITY,
        DOCUMENT,
        BIOMETRIC,
        AUTHORITATIVE,
        ADDRESS,
        AML,
        HUMAN_REVIEW,
    ];
}

/// Webhook event types (the same signed envelope as payment events). `data`
/// always carries `sessionId` and `externalCustomerId`; terminal events
/// carry the decision. A rescreen or a screening hit on an approved session
/// must revoke access.
pub mod kyc_events {
    pub const SESSION_CREATED: &str = "kyc.session.created";
    pub const SESSION_INITIATED: &str = "kyc.session.initiated";
    pub const SESSION_EXPIRED: &str = "kyc.session.expired";
    pub const SESSION_ABANDONED: &str = "kyc.session.abandoned";
    pub const SESSION_CANCELLED: &str = "kyc.session.cancelled";
    pub const DOCUMENT_RECEIVED: &str = "kyc.document.received";
    pub const DOCUMENT_REJECTED: &str = "kyc.document.rejected";
    pub const LIVENESS_COMPLETED: &str = "kyc.liveness.completed";
    pub const PROCESSING_STARTED: &str = "kyc.processing.started";
    pub const MANUAL_REVIEW_REQUIRED: &str = "kyc.manual_review.required";
    pub const APPROVED: &str = "kyc.approved";
    pub const REJECTED: &str = "kyc.rejected";
    pub const RESCREEN_REQUIRED: &str = "kyc.rescreen.required";
    pub const KYB_SESSION_CREATED: &str = "kyb.session.created";
    pub const KYB_COMPANY_VERIFIED: &str = "kyb.company.verified";
    pub const KYB_COMPANY_NOT_FOUND: &str = "kyb.company.not_found";
    pub const KYB_UBO_PENDING: &str = "kyb.ubo.pending";
    pub const KYB_RELATED_KYC_REQUIRED: &str = "kyb.related_kyc.required";
    pub const KYB_RELATED_KYC_COMPLETED: &str = "kyb.related_kyc.completed";
    pub const KYB_MANUAL_REVIEW_REQUIRED: &str = "kyb.manual_review.required";
    pub const KYB_APPROVED: &str = "kyb.approved";
    pub const KYB_REJECTED: &str = "kyb.rejected";
    pub const SCREENING_MATCH_DETECTED: &str = "screening.match.detected";
    pub const SCREENING_FALSE_POSITIVE_CLEARED: &str = "screening.false_positive.cleared";
    pub const MONITORING_ALERT_CREATED: &str = "ongoing_monitoring.alert.created";
    /// Every event type.
    pub const ALL: &[&str] = &[
        SESSION_CREATED,
        SESSION_INITIATED,
        SESSION_EXPIRED,
        SESSION_ABANDONED,
        SESSION_CANCELLED,
        DOCUMENT_RECEIVED,
        DOCUMENT_REJECTED,
        LIVENESS_COMPLETED,
        PROCESSING_STARTED,
        MANUAL_REVIEW_REQUIRED,
        APPROVED,
        REJECTED,
        RESCREEN_REQUIRED,
        KYB_SESSION_CREATED,
        KYB_COMPANY_VERIFIED,
        KYB_COMPANY_NOT_FOUND,
        KYB_UBO_PENDING,
        KYB_RELATED_KYC_REQUIRED,
        KYB_RELATED_KYC_COMPLETED,
        KYB_MANUAL_REVIEW_REQUIRED,
        KYB_APPROVED,
        KYB_REJECTED,
        SCREENING_MATCH_DETECTED,
        SCREENING_FALSE_POSITIVE_CLEARED,
        MONITORING_ALERT_CREATED,
    ];
}

/// Every KYC webhook event type.
pub const KYC_EVENT_TYPES: &[&str] = kyc_events::ALL;

/// Longest session lifetime `ttlMinutes` accepts (one day).
pub const KYC_SESSION_TTL_MAX_MINUTES: i64 = 1440;

const SESSION_FIELDS: &[&str] = &[
    "externalCustomerId",
    "customerMode",
    "customer",
    "jurisdiction",
    "requiredDocumentTypes",
    "primaryDocumentType",
    "evidencePurpose",
    "ttlMinutes",
    "idempotencyKey",
    "evidenceContextJson",
];

/// Every KYC route the SDK calls (bearer-only on the gateway). The browser
/// bootstraps its own SDK token with the publishable key; that route is not
/// reachable from a server key and is not here.
pub const KYC_ROUTES: &[Route] = &[
    Route::new("createSession", "POST", "/api/v1/kyc/sessions")
        .with_optional_key()
        .with_body(&["subjectId", "sessionType"], SESSION_FIELDS),
    Route::new("getSession", "GET", "/api/v1/kyc/sessions/:sessionId"),
    // The merchant list takes only these two; an unknown filter is a 400 on the gateway.
    Route::new("listSessions", "GET", "/api/v1/kyc/sessions").with_query(&["limit", "cursor"]),
    Route::new(
        "setJurisdiction",
        "POST",
        "/api/v1/kyc/sessions/:sessionId/jurisdiction",
    )
    .with_body(&["jurisdiction"], &[]),
    Route::new(
        "setPrimaryDocument",
        "POST",
        "/api/v1/kyc/sessions/:sessionId/primary-document",
    )
    .with_body(&["documentType"], &[]),
    Route::new(
        "uploadDocument",
        "POST",
        "/api/v1/kyc/sessions/:sessionId/documents",
    )
    .with_body(
        &["documentType", "fileBase64", "mimeType"],
        &["side", "jurisdiction", "metadataJson", "idempotencyKey"],
    ),
    Route::new(
        "timeline",
        "GET",
        "/api/v1/kyc/sessions/:sessionId/timeline",
    ),
    Route::new("submit", "POST", "/api/v1/kyc/sessions/:sessionId/submit"),
    Route::new(
        "manualReview",
        "POST",
        "/api/v1/kyc/sessions/:sessionId/manual-review",
    )
    .with_body(&[], &["reasonCode", "reasonDetail"])
    .with_optional_body(),
    Route::new("evidenceRequest", "POST", "/api/v1/kyc/evidence-requests")
        .with_optional_key()
        .with_body(&["subjectId", "sessionType"], SESSION_FIELDS),
    Route::new("merchantPolicy", "GET", "/api/v1/kyc/admin/merchant-policy"),
    Route::new(
        "updateMerchantPolicy",
        "POST",
        "/api/v1/kyc/admin/merchant-policy",
    )
    .with_free_body(),
    Route::new(
        "rotateWebhookSecret",
        "POST",
        "/api/v1/kyc/admin/merchant-policy/webhook-secret/rotate",
    ),
    Route::new("webhookOutbox", "GET", "/api/v1/kyc/admin/webhook-outbox")
        .with_query(&["limit", "status"]),
    Route::new(
        "retryWebhookEvent",
        "POST",
        "/api/v1/kyc/admin/webhook-outbox/:eventId/retry",
    ),
    Route::new(
        "erasureRequests",
        "GET",
        "/api/v1/kyc/admin/erasure-requests",
    )
    .with_query(&["limit", "status"]),
];

/// `true` for `APPROVED`, `REJECTED`, `MANUAL_REVIEW`, `EXPIRED`,
/// `ABANDONED`, `CANCELLED` (case-insensitive).
pub fn is_kyc_session_terminal(status: &str) -> bool {
    KYC_SESSION_TERMINAL_STATES.contains(&status.to_ascii_uppercase().as_str())
}

/// Rank on the assurance ladder: `HIGH` 3, `SUBSTANTIAL` 2, `LOW` 1,
/// anything else 0 (case-insensitive).
pub fn kyc_assurance_rank(level: &str) -> u8 {
    match level.to_ascii_uppercase().as_str() {
        kyc_assurance_level::HIGH => 3,
        kyc_assurance_level::SUBSTANTIAL => 2,
        kyc_assurance_level::LOW => 1,
        _ => 0,
    }
}

/// `true` when `achieved` meets or exceeds `required`.
pub fn kyc_assurance_meets(achieved: &str, required: &str) -> bool {
    kyc_assurance_rank(achieved) >= kyc_assurance_rank(required)
}
