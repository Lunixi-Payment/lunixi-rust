//! Sanctions screening values, the route table and the clear-read rule
//! (`/api/v1/sanction`).

use crate::route_table::{NestedFields, Route};
use serde_json::Value;

/// Risk domains a screening can be narrowed to (`riskDomains`). Left out: every domain.
pub mod sanction_risk_domain {
    pub const SANCTIONS: &str = "sanctions";
    pub const PEP: &str = "pep";
    pub const RCA: &str = "rca";
    pub const WANTED: &str = "wanted";
    pub const REGULATORY: &str = "regulatory";
    pub const DEBARMENT: &str = "debarment";
    pub const OWNERSHIP: &str = "ownership";
    pub const VESSEL: &str = "vessel";
    pub const SECURITIES: &str = "securities";
    /// Every domain.
    pub const ALL: &[&str] = &[
        SANCTIONS, PEP, RCA, WANTED, REGULATORY, DEBARMENT, OWNERSHIP, VESSEL, SECURITIES,
    ];
}

/// Name-screening presets (`mode`). Sent with `riskDomains`, their intersection is screened.
pub mod sanction_screen_mode {
    pub const SANCTIONS_ONLY: &str = "sanctions_only";
    pub const SANCTIONS_PEP: &str = "sanctions_pep";
    pub const FULL_RISK: &str = "full_risk";
    /// Every mode.
    pub const ALL: &[&str] = &[SANCTIONS_ONLY, SANCTIONS_PEP, FULL_RISK];
}

/// Wallet-screening depth (`mode` of `screen_wallet`).
pub mod sanction_wallet_screen_mode {
    pub const WALLET_SANCTIONS: &str = "wallet_sanctions";
    pub const WALLET_RISK: &str = "wallet_risk";
    pub const WALLET_FULL_AML: &str = "wallet_full_aml";
    /// Every mode.
    pub const ALL: &[&str] = &[WALLET_SANCTIONS, WALLET_RISK, WALLET_FULL_AML];
}

/// Subject types (`subjectType`).
pub mod sanction_subject_type {
    pub const PERSON: &str = "person";
    pub const ORGANIZATION: &str = "organization";
    /// Every type.
    pub const ALL: &[&str] = &[PERSON, ORGANIZATION];
}

/// How much of the requested scope a screening covered (`coverageStatus`).
/// `PARTIAL`: some requested domain has no list (`uncoveredRiskDomains`,
/// `uncoveredSources`). `DEGRADED`: the candidate search was limited; run
/// the screening again.
pub mod sanction_coverage_status {
    pub const COMPLETE: &str = "COMPLETE";
    pub const PARTIAL: &str = "PARTIAL";
    pub const DEGRADED: &str = "DEGRADED";
    /// Every status.
    pub const ALL: &[&str] = &[COMPLETE, PARTIAL, DEGRADED];
}

/// Match strength of a hit.
pub mod sanction_hit_type {
    pub const EXACT: &str = "exact";
    pub const PROBABLE: &str = "probable";
    pub const POSSIBLE: &str = "possible";
    /// Every type.
    pub const ALL: &[&str] = &[EXACT, PROBABLE, POSSIBLE];
}

/// Screening webhook event types (same signed envelope as every Lunixi
/// webhook). They also fire from KYC's AML screening: reconcile by the
/// carried customer or session reference.
pub mod sanction_events {
    pub const MATCH_DETECTED: &str = "screening.match.detected";
    pub const FALSE_POSITIVE_CLEARED: &str = "screening.false_positive.cleared";
    pub const MONITORING_ALERT_CREATED: &str = "ongoing_monitoring.alert.created";
    pub const MONITORING_CLEAR: &str = "ongoing_monitoring.clear";
    /// Every event type.
    pub const ALL: &[&str] = &[
        MATCH_DETECTED,
        FALSE_POSITIVE_CLEARED,
        MONITORING_ALERT_CREATED,
        MONITORING_CLEAR,
    ];
}

const PAGE: &[&str] = &["limit", "cursor"];

/// Every sanction route an API key can reach: method, path, body fields
/// (`required`, `optional`, `any_of`) and query parameters. The routes
/// need `sanction:view_results`.
pub const SANCTION_ROUTES: &[Route] = &[
    // Screening
    Route::new("screenName", "POST", "/api/v1/sanction/screening/name")
        .with_screening()
        .with_body(
            &["name"],
            &[
                "subjectType",
                "country",
                "dateOfBirth",
                "identifiers",
                "riskDomains",
                "mode",
                "threshold",
                "customerReference",
            ],
        ),
    Route::new("screenWallet", "POST", "/api/v1/sanction/screening/wallet")
        .with_screening()
        .with_body(&["walletAddress"], &["chain", "mode", "customerReference"]),
    Route::new(
        "screenAccount",
        "POST",
        "/api/v1/sanction/screening/account",
    )
    .with_screening()
    .with_body(
        &["accountValue", "accountScheme"],
        &["providerCode", "productChannel", "customerReference"],
    ),
    Route::new(
        "screenTransaction",
        "POST",
        "/api/v1/sanction/screening/transaction",
    )
    .with_screening()
    .with_body(
        &[],
        &[
            "transactionType",
            "direction",
            "amount",
            "currency",
            "channel",
            "occurredAt",
            "parties",
            "accounts",
            "metadataJson",
        ],
    )
    .with_any_of(&["parties", "accounts"]),
    Route::new(
        "screenPublicProcurementBan",
        "POST",
        "/api/v1/sanction/intelligence/public-procurement-ban",
    )
    .with_screening()
    .with_body(
        &[],
        &[
            "entityName",
            "tckn",
            "vkn",
            "entityType",
            "onlyActive",
            "limit",
            "customerReference",
            "productChannel",
        ],
    )
    .with_any_of(&["entityName", "tckn", "vkn"]),
    Route::new(
        "screenAdverseMedia",
        "POST",
        "/api/v1/sanction/intelligence/adverse-media",
    )
    .with_screening()
    .with_body(
        &[],
        &[
            "entityName",
            "entityType",
            "tckn",
            "vkn",
            "countryCode",
            "limit",
            "onlyActive",
            "customerReference",
            "productChannel",
        ],
    ),
    Route::new(
        "evaluateBehavioralAlert",
        "POST",
        "/api/v1/sanction/intelligence/behavioral-alert",
    )
    .with_screening()
    .with_body(
        &[],
        &[
            "subjectReference",
            "customerReference",
            "productChannel",
            "velocityScore",
            "unusualPattern",
            "countryRiskLevel",
            "metadataJson",
        ],
    ),
    Route::new(
        "listScreeningHistory",
        "GET",
        "/api/v1/sanction/screening/history",
    )
    .with_query(PAGE),
    // Ongoing monitoring
    Route::new(
        "registerMonitoring",
        "POST",
        "/api/v1/sanction/monitoring/register",
    )
    .with_body(
        &["externalCustomerId"],
        &["subjectType", "profileJson", "frequency", "riskDomainScope"],
    ),
    Route::new(
        "listMonitoringAlerts",
        "GET",
        "/api/v1/sanction/monitoring/alerts",
    )
    .with_query(&["status", "limit", "cursor", "includeTotal"]),
    Route::new(
        "listMonitoredEntities",
        "GET",
        "/api/v1/sanction/monitoring/entities",
    )
    .with_query(&["activeOnly", "limit", "cursor", "includeTotal"]),
    Route::new(
        "listMonitoringMatches",
        "GET",
        "/api/v1/sanction/monitoring/matches",
    )
    .with_query(&["status", "limit", "cursor"]),
    Route::new(
        "getDatasetFreshness",
        "GET",
        "/api/v1/sanction/monitoring/dataset-freshness",
    ),
    // Entity intelligence
    Route::new(
        "searchEntities",
        "GET",
        "/api/v1/sanction/intelligence/entity-search",
    )
    .with_query(&["q", "limit"]),
    Route::new(
        "getEntityProfile",
        "GET",
        "/api/v1/sanction/intelligence/entity-profiles/:entityId",
    )
    .with_query(&["screeningRequestId"]),
    Route::new(
        "getEntityGraph",
        "GET",
        "/api/v1/sanction/intelligence/entity-graph/:entityId",
    )
    .with_query(&["depth"]),
    Route::new(
        "getIndirectExposure",
        "GET",
        "/api/v1/sanction/intelligence/indirect-exposure/:entityId",
    )
    .with_query(&["maxDepth", "entityIds"]),
    Route::new(
        "getOwnershipAggregation",
        "GET",
        "/api/v1/sanction/intelligence/ownership-aggregation/:entityId",
    )
    .with_query(&["depth", "entityIds"]),
    // Your own deny list (read; adding an entry is not an API-key route)
    Route::new("listBlacklistEntries", "GET", "/api/v1/sanction/blacklist").with_query(PAGE),
];

/// Fields of the nested lists, checked element by element:
/// `("route.field", element fields)`.
pub const SANCTION_NESTED_FIELDS: &[(&str, NestedFields)] = &[
    (
        "screenName.identifiers",
        NestedFields::new(&["type", "value"], &[]),
    ),
    (
        "screenTransaction.parties",
        NestedFields::new(
            &["name"],
            &[
                "role",
                "subjectType",
                "countryCode",
                "customerId",
                "dateOfBirth",
                "nationalId",
            ],
        ),
    ),
    (
        "screenTransaction.accounts",
        NestedFields::new(
            &["accountType", "value"],
            &["role", "providerCode", "chain"],
        ),
    ),
];

/// Value sets checked before sending (`"route.field"`, allowed values);
/// the sanction service refuses anything else with 400.
pub const SANCTION_VALUE_SETS: &[(&str, &[&str])] = &[
    ("screenName.riskDomains", sanction_risk_domain::ALL),
    ("screenName.mode", sanction_screen_mode::ALL),
    ("screenName.subjectType", sanction_subject_type::ALL),
    ("screenWallet.mode", sanction_wallet_screen_mode::ALL),
    ("registerMonitoring.subjectType", sanction_subject_type::ALL),
    (
        "registerMonitoring.riskDomainScope",
        sanction_risk_domain::ALL,
    ),
];

const BLOCKING_DECISIONS: &[&str] = &[
    "block", "blocked", "reject", "rejected", "review", "hit", "escalate",
];

/// Whether a screening answer (the `data` a screening method returned) can
/// be read as "nothing found".
///
/// True only when the answer has no match or hit of any kind
/// (`matches`, `hits`, `publicationHits`), is not sanctioned, blacklisted,
/// matched or case-requiring, carries no blocking `decision` and, when it
/// reports coverage, its `coverageStatus` is `COMPLETE`. A `PARTIAL` or
/// `DEGRADED` screening and a replay without coverage (`null`) are never
/// clear. A failed screening never reaches this function: the call fails.
/// Account and adverse-media screening report no coverage; for them the hits
/// and flags decide.
pub fn is_clear_screening(result: &Value) -> bool {
    let Some(object) = result.as_object() else {
        return false;
    };
    let non_empty_list = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|list| !list.is_empty())
    };
    if ["matches", "hits", "publicationHits"]
        .iter()
        .any(|key| non_empty_list(key))
    {
        return false;
    }
    if ["isSanctioned", "isBlacklisted", "matched", "caseRequired"]
        .iter()
        .any(|key| object.get(*key) == Some(&Value::Bool(true)))
    {
        return false;
    }
    let decision = object
        .get("decision")
        .and_then(Value::as_str)
        .map(str::to_lowercase)
        .unwrap_or_default();
    if BLOCKING_DECISIONS.contains(&decision.as_str()) {
        return false;
    }
    match object.get("coverageStatus") {
        Some(status) => status.as_str() == Some(sanction_coverage_status::COMPLETE),
        None => true,
    }
}
