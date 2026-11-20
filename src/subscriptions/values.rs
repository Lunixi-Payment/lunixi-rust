//! Subscription values and the route table (`/api/v1/subscriptions`, and the
//! portal-link routes under `/api/panel/v1/subscriptions/portal`).

use crate::route_table::Route;
use serde_json::Value;

/// Subscription status (`status`).
pub mod subscription_status {
    /// `startsAt` is in the future.
    pub const PENDING: &str = "PENDING";
    /// In the trial period.
    pub const TRIALING: &str = "TRIALING";
    /// Billed each cycle.
    pub const ACTIVE: &str = "ACTIVE";
    /// A charge failed; dunning runs.
    pub const PAST_DUE: &str = "PAST_DUE";
    /// Dunning exhausted; grace period.
    pub const UNPAID: &str = "UNPAID";
    /// Paused by the merchant; no charges.
    pub const PAUSED: &str = "PAUSED";
    /// Replaced by a higher plan.
    pub const UPGRADED: &str = "UPGRADED";
    /// Replaced by a lower plan.
    pub const DOWNGRADED: &str = "DOWNGRADED";
    /// Canceled.
    pub const CANCELED: &str = "CANCELED";
    /// The recurrence ran out.
    pub const EXPIRED: &str = "EXPIRED";
    /// Every status.
    pub const ALL: &[&str] = &[
        PENDING, TRIALING, ACTIVE, PAST_DUE, UNPAID, PAUSED, UPGRADED, DOWNGRADED, CANCELED,
        EXPIRED,
    ];
    /// Live states: the subscription may be billed.
    pub const ACTIVE_STATES: &[&str] = &[TRIALING, ACTIVE, PAST_DUE];
    /// Terminal states: no further transition.
    pub const TERMINAL_STATES: &[&str] = &[UPGRADED, DOWNGRADED, CANCELED, EXPIRED];
}

/// Live states: the subscription may be billed (`TRIALING`, `ACTIVE`, `PAST_DUE`).
pub const SUBSCRIPTION_ACTIVE_STATES: &[&str] = subscription_status::ACTIVE_STATES;
/// Terminal states (`UPGRADED`, `DOWNGRADED`, `CANCELED`, `EXPIRED`).
pub const SUBSCRIPTION_TERMINAL_STATES: &[&str] = subscription_status::TERMINAL_STATES;

/// Errors of `customers().upsert`: a missing or invalid customer field
/// (`details.field`, `details.reason`, `details.fields`).
pub mod subscription_customer_error_code {
    /// A required customer field is missing.
    pub const FIELD_REQUIRED: &str = "subscription.customer.field_required";
    /// A customer field is invalid.
    pub const FIELD_INVALID: &str = "subscription.customer.field_invalid";
    /// Every code.
    pub const ALL: &[&str] = &[FIELD_REQUIRED, FIELD_INVALID];
}

/// Invoice status: `DRAFT` → `OPEN` → `PAID` | `UNCOLLECTIBLE` | `VOID`.
pub mod subscription_invoice_status {
    pub const DRAFT: &str = "DRAFT";
    pub const OPEN: &str = "OPEN";
    pub const PAID: &str = "PAID";
    pub const UNCOLLECTIBLE: &str = "UNCOLLECTIBLE";
    pub const VOID: &str = "VOID";
    /// Every status.
    pub const ALL: &[&str] = &[DRAFT, OPEN, PAID, UNCOLLECTIBLE, VOID];
}

/// How invoices are collected (`collectionMethod`).
pub mod subscription_collection_method {
    /// The saved card is charged each cycle (the default).
    pub const AUTO_CHARGE: &str = "AUTO_CHARGE";
    /// The invoice is finalised but not charged.
    pub const SEND_INVOICE: &str = "SEND_INVOICE";
    /// Collection is up to the merchant.
    pub const MANUAL: &str = "MANUAL";
    /// Every method.
    pub const ALL: &[&str] = &[AUTO_CHARGE, SEND_INVOICE, MANUAL];
}

/// `billing.frequency` of a plan.
pub mod subscription_billing_frequency {
    pub const DAILY: &str = "DAILY";
    pub const WEEKLY: &str = "WEEKLY";
    pub const MONTHLY: &str = "MONTHLY";
    pub const YEARLY: &str = "YEARLY";
    /// Every frequency.
    pub const ALL: &[&str] = &[DAILY, WEEKLY, MONTHLY, YEARLY];
}

/// `verificationMode` of a saved card.
pub mod subscription_card_verification_mode {
    pub const NONE: &str = "NONE";
    pub const ON_ADD: &str = "ON_ADD";
    pub const FIRST_CHARGE: &str = "FIRST_CHARGE";
    /// Every mode.
    pub const ALL: &[&str] = &[NONE, ON_ADD, FIRST_CHARGE];
}

/// `behavior` of a plan change: now (prorated) or at the next period.
pub mod subscription_plan_change_behavior {
    pub const IMMEDIATE: &str = "IMMEDIATE";
    pub const NEXT_PERIOD: &str = "NEXT_PERIOD";
    /// Every behavior.
    pub const ALL: &[&str] = &[IMMEDIATE, NEXT_PERIOD];
}

/// `sdkMode` of a portal SDK session.
pub mod subscription_portal_sdk_mode {
    pub const FULL: &str = "FULL";
    pub const HEADLESS: &str = "HEADLESS";
    pub const HYBRID: &str = "HYBRID";
    /// Every mode.
    pub const ALL: &[&str] = &[FULL, HEADLESS, HYBRID];
}

/// Hosted customer portal; a portal link's `token` opens it
/// ([`subscription_portal_url`]).
pub const SUBSCRIPTION_PORTAL_HOSTED_BASE: &str = "https://portal.lunixi.com";

/// Webhook event types (the same signed envelope as payment events; `data`
/// amounts are minor-unit text).
pub mod subscription_events {
    pub const CREATED: &str = "subscription.created";
    pub const ACTIVATED: &str = "subscription.activated";
    pub const TRIAL_WILL_END: &str = "subscription.trial_will_end";
    pub const PAUSED: &str = "subscription.paused";
    pub const RESUMED: &str = "subscription.resumed";
    pub const CANCEL_SCHEDULED: &str = "subscription.cancel_scheduled";
    pub const CANCELED: &str = "subscription.canceled";
    pub const UPGRADED: &str = "subscription.upgraded";
    pub const DOWNGRADED: &str = "subscription.downgraded";
    pub const PLAN_CHANGE_SCHEDULED: &str = "subscription.plan_change_scheduled";
    pub const PENDING_CHANGE_APPLIED: &str = "subscription.pending_change_applied";
    pub const PAYMENT_CARD_SWAPPED: &str = "subscription.payment_card_swapped";
    pub const PAYMENT_METHOD_LOST: &str = "subscription.payment_method_lost";
    pub const INVOICE_CREATED: &str = "invoice.created";
    pub const INVOICE_PAID: &str = "invoice.paid";
    pub const INVOICE_PAYMENT_FAILED: &str = "invoice.payment_failed";
    pub const INVOICE_VOIDED: &str = "invoice.voided";
    pub const CARD_ADDED: &str = "card.added";
    pub const CARD_SET_DEFAULT: &str = "card.set_default";
    pub const CARD_UPDATED: &str = "card.updated";
    pub const CARD_DEACTIVATED: &str = "card.deactivated";
    pub const CARD_EXPIRING_SOON: &str = "card.expiring_soon";
    pub const CARD_EXPIRED: &str = "card.expired";
    /// Every event type.
    pub const ALL: &[&str] = &[
        CREATED,
        ACTIVATED,
        TRIAL_WILL_END,
        PAUSED,
        RESUMED,
        CANCEL_SCHEDULED,
        CANCELED,
        UPGRADED,
        DOWNGRADED,
        PLAN_CHANGE_SCHEDULED,
        PENDING_CHANGE_APPLIED,
        PAYMENT_CARD_SWAPPED,
        PAYMENT_METHOD_LOST,
        INVOICE_CREATED,
        INVOICE_PAID,
        INVOICE_PAYMENT_FAILED,
        INVOICE_VOIDED,
        CARD_ADDED,
        CARD_SET_DEFAULT,
        CARD_UPDATED,
        CARD_DEACTIVATED,
        CARD_EXPIRING_SOON,
        CARD_EXPIRED,
    ];
}

/// Every subscription webhook event type.
pub const SUBSCRIPTION_EVENT_TYPES: &[&str] = subscription_events::ALL;

/// Fields of a plan's `billing` rule.
pub const SUBSCRIPTION_BILLING_RULE_FIELDS: &[&str] = &[
    "frequency",
    "interval",
    "byMonthDay",
    "byWeekDay",
    "timezone",
    "lastDayOfMonthFix",
    "count",
];

const fn get(name: &'static str, path: &'static str) -> Route {
    Route::new(name, "GET", path)
}

/// Every subscription route the SDK calls: method, path, the
/// `Idempotency-Key` rule and the body / query fields. The surface is
/// bearer-only on the gateway (`step_up` is never set); names are the
/// catalog's operation names (`plans.create`).
pub const SUBSCRIPTION_ROUTES: &[Route] = &[
    Route::new("create", "POST", "/api/v1/subscriptions/subscriptions")
        .with_key()
        .with_body(
            &["customerId", "planId", "idempotencyKey"],
            &[
                "paymentCardId",
                "paymentMethodId",
                "collectionMethod",
                "priceUpdatePolicy",
                "entitlementPolicy",
                "billingTimezone",
                "startsAt",
                "chargeOnCreate",
                "billingCycleAnchorOverride",
                "skipTrial",
                "metadata",
            ],
        ),
    get("get", "/api/v1/subscriptions/subscriptions/:subscriptionId"),
    get("list", "/api/v1/subscriptions/subscriptions").with_query(&[
        "customerId",
        "status",
        "pageSize",
        "pageToken",
    ]),
    Route::new(
        "cancel",
        "POST",
        "/api/v1/subscriptions/subscriptions/:subscriptionId/cancel",
    )
    .with_body(&["cancelAtPeriodEnd"], &["reason"]),
    Route::new(
        "pause",
        "POST",
        "/api/v1/subscriptions/subscriptions/:subscriptionId/pause",
    )
    .with_body(&[], &["resumeAt"])
    .with_optional_body(),
    Route::new(
        "resume",
        "POST",
        "/api/v1/subscriptions/subscriptions/:subscriptionId/resume",
    ),
    Route::new(
        "changePlan",
        "POST",
        "/api/v1/subscriptions/subscriptions/:subscriptionId/change-plan",
    )
    .with_optional_key()
    .with_body(&["newPlanId"], &["behavior", "reason", "idempotencyKey"]),
    get(
        "auditLogs",
        "/api/v1/subscriptions/subscriptions/:subscriptionId/audit-logs",
    )
    .with_query(&["limit", "pageToken"]),
    // Plans
    Route::new("plans.create", "POST", "/api/v1/subscriptions/plans")
        .with_optional_key()
        .with_body(
            &["productId", "code", "name", "price", "currency", "billing"],
            &[
                "description",
                "trialPeriodDays",
                "maxDunningAttempts",
                "dunningScheduleDays",
                "gracePeriodDays",
                "metadata",
            ],
        ),
    get("plans.get", "/api/v1/subscriptions/plans/:planId"),
    get("plans.list", "/api/v1/subscriptions/plans").with_query(&[
        "productId",
        "includeArchived",
        "pageSize",
        "pageToken",
    ]),
    Route::new(
        "plans.update",
        "PATCH",
        "/api/v1/subscriptions/plans/:planId",
    )
    .with_free_body(),
    Route::new(
        "plans.archive",
        "DELETE",
        "/api/v1/subscriptions/plans/:planId",
    ),
    get(
        "plans.translations",
        "/api/v1/subscriptions/plans/:planId/translations",
    ),
    Route::new(
        "plans.upsertTranslation",
        "PATCH",
        "/api/v1/subscriptions/plans/:planId/translations/:locale",
    )
    .with_free_body(),
    // Customers
    // Renewals take the buyer from this record: the identity, contact and
    // billing address are required (400 `subscription.customer.field_required`).
    Route::new("customers.upsert", "PUT", "/api/v1/subscriptions/customers")
        .with_optional_key()
        .with_body(
            &[
                "externalId",
                "name",
                "surname",
                "email",
                "gsmNumber",
                "identityNumber",
                "billingAddress",
                "billingCity",
                "billingCountry",
                "billingZipCode",
            ],
            &[
                "billingContactName",
                "shippingContactName",
                "shippingAddress",
                "shippingCity",
                "shippingCountry",
                "shippingZipCode",
            ],
        ),
    get(
        "customers.get",
        "/api/v1/subscriptions/customers/:customerId",
    ),
    get(
        "customers.getByExternalId",
        "/api/v1/subscriptions/customers/by-external/:externalId",
    ),
    get("customers.list", "/api/v1/subscriptions/customers").with_query(&[
        "search",
        "includeArchived",
        "pageSize",
        "pageToken",
    ]),
    get(
        "customers.creditBalance",
        "/api/v1/subscriptions/customers/:customerId/credit-balance",
    ),
    Route::new(
        "customers.adjustCredit",
        "POST",
        "/api/v1/subscriptions/customers/:customerId/credit-balance/adjust",
    )
    .with_free_body(),
    Route::new(
        "customers.archive",
        "DELETE",
        "/api/v1/subscriptions/customers/:customerId",
    ),
    // Cards
    Route::new("cards.add", "POST", "/api/v1/subscriptions/cards")
        .with_optional_key()
        .with_body(
            &["customerId", "paymentMethodToken"],
            &[
                "brand",
                "last4",
                "bin",
                "expireMonth",
                "expireYear",
                "cardholderName",
                "isDefault",
                "nickname",
                "verificationMode",
                "supportedCurrencies",
            ],
        ),
    get("cards.get", "/api/v1/subscriptions/cards/:cardId"),
    get("cards.list", "/api/v1/subscriptions/cards").with_query(&[
        "customerId",
        "includeInactive",
        "pageSize",
        "pageToken",
    ]),
    Route::new(
        "cards.setDefault",
        "POST",
        "/api/v1/subscriptions/cards/:cardId/set-default",
    ),
    Route::new(
        "cards.deactivate",
        "DELETE",
        "/api/v1/subscriptions/cards/:cardId",
    )
    .with_body(&[], &["reason"])
    .with_optional_body(),
    Route::new(
        "cards.verify",
        "POST",
        "/api/v1/subscriptions/cards/:cardId/verify",
    )
    .with_free_body()
    .with_optional_body(),
    Route::new(
        "cards.update",
        "PATCH",
        "/api/v1/subscriptions/cards/:cardId",
    )
    .with_free_body(),
    // Invoices
    get("invoices.list", "/api/v1/subscriptions/invoices").with_query(&[
        "subscriptionId",
        "status",
        "pageSize",
        "pageToken",
    ]),
    get("invoices.get", "/api/v1/subscriptions/invoices/:invoiceId"),
    Route::new(
        "invoices.void",
        "POST",
        "/api/v1/subscriptions/invoices/:invoiceId/void",
    )
    .with_body(&[], &["reason"])
    .with_optional_body(),
    Route::new(
        "invoices.retryCharge",
        "POST",
        "/api/v1/subscriptions/invoices/:invoiceId/retry-charge",
    )
    .with_optional_key(),
    // Portal
    Route::new(
        "portal.createSdkSession",
        "POST",
        "/api/v1/subscriptions/portal/sdk-session",
    )
    .with_body(&["customerId", "origin"], &["sdkMode", "portalLinkId"]),
    Route::new(
        "portal.createPortalLink",
        "POST",
        "/api/panel/v1/subscriptions/portal/links",
    )
    .with_free_body(),
    get(
        "portal.listLinks",
        "/api/panel/v1/subscriptions/portal/links",
    )
    .with_query(&["customerId", "status", "limit"]),
    get(
        "portal.getLink",
        "/api/panel/v1/subscriptions/portal/links/:linkId",
    ),
    Route::new(
        "portal.updateLink",
        "PATCH",
        "/api/panel/v1/subscriptions/portal/links/:linkId",
    )
    .with_free_body(),
    Route::new(
        "portal.revokeLink",
        "DELETE",
        "/api/panel/v1/subscriptions/portal/links/:linkId",
    )
    .with_body(&[], &["reason"])
    .with_optional_body(),
    get(
        "portal.linkActivity",
        "/api/panel/v1/subscriptions/portal/links/:linkId/activity",
    ),
    Route::new(
        "portal.sendInvite",
        "POST",
        "/api/panel/v1/subscriptions/portal/invite",
    )
    .with_free_body(),
    get(
        "portal.getSettings",
        "/api/panel/v1/subscriptions/portal/settings",
    ),
    Route::new(
        "portal.updateSettings",
        "PUT",
        "/api/panel/v1/subscriptions/portal/settings",
    )
    .with_free_body(),
    // Catalog products
    Route::new("products.create", "POST", "/api/v1/subscriptions/products").with_free_body(),
    get("products.list", "/api/v1/subscriptions/products")
        .with_query(&["limit", "cursor", "active"]),
    get("products.get", "/api/v1/subscriptions/products/:productId"),
    Route::new(
        "products.update",
        "PATCH",
        "/api/v1/subscriptions/products/:productId",
    )
    .with_free_body(),
    Route::new(
        "products.archive",
        "DELETE",
        "/api/v1/subscriptions/products/:productId",
    ),
    get(
        "products.translations",
        "/api/v1/subscriptions/products/:productId/translations",
    ),
    Route::new(
        "products.upsertTranslation",
        "PATCH",
        "/api/v1/subscriptions/products/:productId/translations/:locale",
    )
    .with_free_body(),
    // Features
    Route::new("features.create", "POST", "/api/v1/subscriptions/features").with_free_body(),
    get("features.list", "/api/v1/subscriptions/features").with_any_query(),
    get("features.get", "/api/v1/subscriptions/features/:featureId"),
    Route::new(
        "features.update",
        "PATCH",
        "/api/v1/subscriptions/features/:featureId",
    )
    .with_free_body(),
    Route::new(
        "features.delete",
        "DELETE",
        "/api/v1/subscriptions/features/:featureId",
    ),
    get(
        "features.translations",
        "/api/v1/subscriptions/features/:featureId/translations",
    ),
    Route::new(
        "features.upsertTranslation",
        "PATCH",
        "/api/v1/subscriptions/features/:featureId/translations/:locale",
    )
    .with_free_body(),
    // Add-ons
    Route::new("addons.create", "POST", "/api/v1/subscriptions/addons").with_free_body(),
    get("addons.list", "/api/v1/subscriptions/addons").with_any_query(),
    Route::new(
        "addons.update",
        "PATCH",
        "/api/v1/subscriptions/addons/:addonId",
    )
    .with_free_body(),
    Route::new(
        "addons.deactivate",
        "DELETE",
        "/api/v1/subscriptions/addons/:addonId",
    ),
    // Usage
    Route::new("usage.record", "POST", "/api/v1/subscriptions/usage/record").with_free_body(),
    Route::new(
        "usage.checkEntitlement",
        "POST",
        "/api/v1/subscriptions/usage/check-entitlement",
    )
    .with_free_body(),
];

fn status_of(subscription: &Value) -> &str {
    match subscription {
        Value::String(status) => status,
        _ => subscription
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or(""),
    }
}

/// `true` while the subscription may be billed (`TRIALING`, `ACTIVE`,
/// `PAST_DUE`). Takes a subscription object or its status text.
pub fn is_subscription_active(subscription: &Value) -> bool {
    SUBSCRIPTION_ACTIVE_STATES.contains(&status_of(subscription))
}

/// `true` for `UPGRADED`, `DOWNGRADED`, `CANCELED`, `EXPIRED`. Takes a
/// subscription object or its status text.
pub fn is_subscription_terminal(subscription: &Value) -> bool {
    SUBSCRIPTION_TERMINAL_STATES.contains(&status_of(subscription))
}

/// Hosted portal URL of a portal link (the `portal().create_portal_link`
/// answer, or its token as a JSON string); `base` defaults to
/// [`SUBSCRIPTION_PORTAL_HOSTED_BASE`].
pub fn subscription_portal_url(link: &Value, base: Option<&str>) -> String {
    let token = match link {
        Value::String(token) => token.as_str(),
        _ => link.get("token").and_then(Value::as_str).unwrap_or(""),
    };
    let base = base.unwrap_or(SUBSCRIPTION_PORTAL_HOSTED_BASE);
    format!(
        "{}/?token={}",
        base.trim_end_matches('/'),
        encode_component(token)
    )
}

/// `encodeURIComponent`: everything but `A–Z a–z 0–9 - _ . ! ~ * ' ( )`
/// is percent-encoded (UTF-8).
pub(crate) fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
