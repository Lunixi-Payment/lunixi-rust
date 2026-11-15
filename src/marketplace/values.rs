//! Marketplace (sub-dealer split) values and the route table:
//! `/api/v1/marketplace/*` for dealers, commissions, payouts and the audit
//! log, `/api/v1/payments/marketplace/*` for the split charges.

use crate::route_table::Route;
use serde_json::Value;

/// Sub-dealer lifecycle (`status`). Only `ACTIVE` dealers can receive a split.
pub mod marketplace_dealer_status {
    pub const DRAFT: &str = "DRAFT";
    pub const PENDING_KYC: &str = "PENDING_KYC";
    pub const ACTIVE: &str = "ACTIVE";
    pub const SUSPENDED: &str = "SUSPENDED";
    pub const HOLD: &str = "HOLD";
    pub const CLOSED: &str = "CLOSED";
    /// Every status.
    pub const ALL: &[&str] = &[DRAFT, PENDING_KYC, ACTIVE, SUSPENDED, HOLD, CLOSED];
}

/// `dealerType` of a sub-dealer.
pub mod marketplace_dealer_type {
    pub const LEGAL_ENTITY: &str = "LEGAL_ENTITY";
    pub const INDIVIDUAL: &str = "INDIVIDUAL";
    /// Every type.
    pub const ALL: &[&str] = &[LEGAL_ENTITY, INDIVIDUAL];
}

/// `multiSellerPolicy` of a split charge: every seller leg must be eligible,
/// or the eligible legs may go through without the others.
pub mod marketplace_multi_seller_policy {
    pub const ALL_OR_NOTHING: &str = "ALL_OR_NOTHING";
    pub const PARTIAL_ALLOWED: &str = "PARTIAL_ALLOWED";
    /// Every policy.
    pub const ALL: &[&str] = &[ALL_OR_NOTHING, PARTIAL_ALLOWED];
}

/// Hosted dealer onboarding wizard ([`marketplace_onboarding_url`] when the
/// answer carries no `url`).
pub const MARKETPLACE_DEALER_ONBOARDING_HOSTED_BASE: &str = "https://altbayi.lunixi.com";

/// Fields of a seller leg: the aggregate amount of one sub-dealer.
pub const MARKETPLACE_SELLER_FIELDS: &[&str] = &["subDealerId", "amountMinor", "scenario"];

/// Fields a split card charge (`charge_marketplace_card2d` / `3d`) needs
/// besides the seller legs: the commercial context of a traceable card payment.
pub const MARKETPLACE_CHARGE_REQUIRED_FIELDS: &[&str] = &[
    "amount",
    "currency",
    "orderId",
    "card",
    "buyer",
    "billingAddress",
    "basketItems",
];

const fn charge(name: &'static str, path: &'static str) -> Route {
    Route::new(name, "POST", path)
        .with_step_up()
        .with_optional_key()
        .with_free_body()
}

/// Every marketplace route the SDK calls. The three split charges carry the
/// catalog's `step_up` (signed per request) and accept an
/// `Idempotency-Key`; dealer, commission, payout and audit routes are
/// bearer-only on the gateway.
pub const MARKETPLACE_ROUTES: &[Route] = &[
    charge(
        "createCheckoutIntent",
        "/api/v1/payments/marketplace/intents",
    ),
    charge("chargeMarketplaceCard2d", "/api/v1/payments/marketplace/2d"),
    charge("chargeMarketplaceCard3d", "/api/v1/payments/marketplace/3d"),
    Route::new("createDealer", "POST", "/api/v1/marketplace/dealers").with_body(
        &["legalName"],
        &[
            "taxId",
            "dealerType",
            "categoryCode",
            "email",
            "externalRef",
        ],
    ),
    Route::new(
        "bulkCreateDealers",
        "POST",
        "/api/v1/marketplace/dealers/bulk",
    )
    .with_body(&["dealers"], &[]),
    Route::new("listDealers", "GET", "/api/v1/marketplace/dealers")
        .with_query(&["status", "search", "limit", "offset"]),
    Route::new("getDealer", "GET", "/api/v1/marketplace/dealers/:dealerId"),
    Route::new(
        "updateDealerStatus",
        "PUT",
        "/api/v1/marketplace/dealers/:dealerId/status",
    )
    .with_body(&["status"], &[]),
    Route::new(
        "onboardingToken",
        "POST",
        "/api/v1/marketplace/dealers/:dealerId/onboarding-token",
    ),
    Route::new(
        "dealerApplication",
        "GET",
        "/api/v1/marketplace/dealers/:dealerId/application",
    ),
    Route::new(
        "submitDealerApplication",
        "POST",
        "/api/v1/marketplace/dealers/:dealerId/submit",
    )
    .with_free_body(),
    Route::new(
        "setDealerTerms",
        "POST",
        "/api/v1/marketplace/dealers/:dealerId/terms",
    )
    .with_free_body(),
    Route::new(
        "signDealerContract",
        "POST",
        "/api/v1/marketplace/dealers/:dealerId/sign",
    )
    .with_free_body(),
    Route::new(
        "activateDealer",
        "POST",
        "/api/v1/marketplace/dealers/:dealerId/activate",
    ),
    Route::new(
        "recordDealerKycResult",
        "POST",
        "/api/v1/marketplace/dealers/:dealerId/kyc-result",
    )
    .with_free_body(),
    Route::new("payoutLedger", "GET", "/api/v1/marketplace/payout-ledger").with_query(&[
        "status",
        "subDealerId",
        "days",
        "limit",
    ]),
    Route::new("transactions", "GET", "/api/v1/marketplace/transactions").with_query(&[
        "status",
        "subDealerId",
        "limit",
    ]),
    Route::new(
        "commissionRules",
        "GET",
        "/api/v1/marketplace/commission-rules",
    )
    .with_query(&["limit"]),
    Route::new(
        "upsertCommissionRule",
        "POST",
        "/api/v1/marketplace/commission-rules",
    )
    .with_free_body(),
    Route::new(
        "previewCommissionRule",
        "POST",
        "/api/v1/marketplace/commission-rules/preview",
    )
    .with_free_body(),
    Route::new(
        "resolveCommission",
        "GET",
        "/api/v1/marketplace/commission/resolve",
    )
    .with_query(&["subDealerId", "categoryCode", "mtdVolumeMinor"]),
    Route::new("createPayout", "POST", "/api/v1/marketplace/payouts").with_free_body(),
    Route::new(
        "markPayoutPaid",
        "POST",
        "/api/v1/marketplace/payouts/:payoutId/mark-paid",
    )
    .with_free_body(),
    Route::new(
        "bulkMarkPayoutsPaid",
        "POST",
        "/api/v1/marketplace/payouts/bulk-mark-paid",
    )
    .with_free_body(),
    Route::new("auditLog", "GET", "/api/v1/marketplace/audit-log").with_query(&["limit", "type"]),
    Route::new(
        "verifyAuditLog",
        "GET",
        "/api/v1/marketplace/audit-log/verify",
    ),
];

/// Onboarding URL of an `onboarding_token` answer: its `url`, else the
/// hosted wizard with its `token`; `base` defaults to
/// [`MARKETPLACE_DEALER_ONBOARDING_HOSTED_BASE`].
pub fn marketplace_onboarding_url(link: &Value, base: Option<&str>) -> String {
    if let Some(url) = link
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
    {
        return url.to_owned();
    }
    let token = link.get("token").and_then(Value::as_str).unwrap_or("");
    let base = base.unwrap_or(MARKETPLACE_DEALER_ONBOARDING_HOSTED_BASE);
    format!(
        "{}/m/{}",
        base.trim_end_matches('/'),
        crate::subscriptions::encode_component(token)
    )
}
