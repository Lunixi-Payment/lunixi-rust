//! Wallet values and the route table (`/api/v1/wallet`).

use crate::route_table::{NestedFields, Route};

/// End-user account types (`accountType`).
pub mod wallet_account_type {
    /// A person.
    pub const PERSONAL: &str = "PERSONAL";
    /// A business.
    pub const BUSINESS: &str = "BUSINESS";
    /// Every type.
    pub const ALL: &[&str] = &[PERSONAL, BUSINESS];
}

/// End-user statuses (`status`).
pub mod wallet_end_user_status {
    /// Active.
    pub const ACTIVE: &str = "ACTIVE";
    /// Frozen: no money movement.
    pub const FROZEN: &str = "FROZEN";
    /// Closed.
    pub const CLOSED: &str = "CLOSED";
    /// Every status.
    pub const ALL: &[&str] = &[ACTIVE, FROZEN, CLOSED];
}

/// KYC levels (`kycLevel`).
pub mod wallet_kyc_level {
    /// No verification.
    pub const NONE: &str = "NONE";
    /// Low.
    pub const LOW: &str = "LOW";
    /// Substantial.
    pub const SUBSTANTIAL: &str = "SUBSTANTIAL";
    /// High.
    pub const HIGH: &str = "HIGH";
    /// Every level.
    pub const ALL: &[&str] = &[NONE, LOW, SUBSTANTIAL, HIGH];
}

/// OTP channels (`otpChannel`).
pub mod wallet_otp_channel {
    /// E-mail.
    pub const EMAIL: &str = "EMAIL";
    /// SMS.
    pub const SMS: &str = "SMS";
    /// Push notification.
    pub const PUSH: &str = "PUSH";
    /// Every channel.
    pub const ALL: &[&str] = &[EMAIL, SMS, PUSH];
}

/// QR code types (`qrType`). Static codes carry no amount; dynamic ones require it.
pub mod wallet_qr_type {
    /// Person-to-person, no amount.
    pub const P2P_STATIC: &str = "P2P_STATIC";
    /// Person-to-person with an amount.
    pub const P2P_DYNAMIC: &str = "P2P_DYNAMIC";
    /// Merchant, no amount.
    pub const MERCHANT_STATIC: &str = "MERCHANT_STATIC";
    /// Merchant with an amount.
    pub const MERCHANT_DYNAMIC: &str = "MERCHANT_DYNAMIC";
    /// Every type.
    pub const ALL: &[&str] = &[P2P_STATIC, P2P_DYNAMIC, MERCHANT_STATIC, MERCHANT_DYNAMIC];
}

/// QR image formats (`format` of `qr().image`).
pub mod wallet_qr_image_format {
    /// SVG (the default).
    pub const SVG: &str = "svg";
    /// PNG.
    pub const PNG: &str = "png";
    /// Every format.
    pub const ALL: &[&str] = &[SVG, PNG];
}

/// Bulk payout target types (`items[].targetType`).
pub mod wallet_bulk_target_type {
    /// Another wallet.
    pub const WALLET: &str = "WALLET";
    /// A bank account.
    pub const IBAN: &str = "IBAN";
    /// Every type.
    pub const ALL: &[&str] = &[WALLET, IBAN];
}

/// Corporate account statuses (`status`).
pub mod wallet_corporate_account_status {
    /// Active.
    pub const ACTIVE: &str = "ACTIVE";
    /// Archived.
    pub const ARCHIVED: &str = "ARCHIVED";
    /// Every status.
    pub const ALL: &[&str] = &[ACTIVE, ARCHIVED];
}

/// Minor units as a decimal string: `"12550"` is 125.50 TRY. Never a number.
pub const WALLET_AMOUNT_PATTERN: &str = r"^\d{1,18}$";
/// Most digits of a wallet amount.
pub const WALLET_AMOUNT_MAX_DIGITS: usize = 18;
/// Longest wallet `Idempotency-Key` (the gateway trims it and accepts 1–128 characters).
pub const WALLET_IDEMPOTENCY_KEY_MAX: usize = 128;
/// Body fields that carry a minor-unit amount string.
pub const WALLET_AMOUNT_FIELDS: &[&str] = &["amount"];

const COMPLETE: &[&str] = &["endUserId", "operationId", "integrityHash", "nonce"];
const COMPLETE_OPTIONAL: &[&str] = &["otpCode", "trustedActionToken"];
const IBAN_REQUIRED: &[&str] = &[
    "endUserId",
    "sourceWalletAccountId",
    "beneficiaryName",
    "iban",
    "amount",
    "currency",
];
const IBAN_INITIATE_OPTIONAL: &[&str] = &[
    "bankCode",
    "paymentPurpose",
    "scheduleVersionId",
    "otpDestination",
    "otpChannel",
    "locale",
];

/// Every wallet route the SDK calls: method, path, `Idempotency-Key` rule and
/// the body / query fields the gateway accepts (an unknown body field is a
/// 400). Every route is signed, reads included.
pub const WALLET_ROUTES: &[Route] = &[
    Route::new("ping", "GET", "/api/v1/wallet/ping"),
    // Programs
    Route::new("listMyPrograms", "GET", "/api/v1/wallet/programs/me"),
    Route::new(
        "setCollectionAnchor",
        "PUT",
        "/api/v1/wallet/programs/:programId/collection-anchor",
    )
    .with_body(&[], &["collectionEndUserId", "reason", "expectedVersion"]),
    // End users
    Route::new("createEndUser", "POST", "/api/v1/wallet/end-users")
        .with_key()
        .with_body(
            &["programId", "accountType", "phone"],
            &[
                "externalCustomerId",
                "kycRef",
                "kycLevelCache",
                "displayName",
                "corporateAccountId",
                "defaultCurrency",
            ],
        ),
    Route::new("listEndUsers", "GET", "/api/v1/wallet/end-users").with_query(&[
        "programId",
        "accountType",
        "status",
        "corporateAccountId",
        "merchantCategoryState",
        "limit",
        "cursor",
    ]),
    Route::new(
        "getEndUserByExternalId",
        "GET",
        "/api/v1/wallet/end-users/by-external/:externalCustomerId",
    )
    .with_query(&["programId"]),
    Route::new("getEndUser", "GET", "/api/v1/wallet/end-users/:endUserId"),
    Route::new(
        "freezeEndUser",
        "POST",
        "/api/v1/wallet/end-users/:endUserId/freeze",
    )
    .with_key()
    .with_body(&[], &["reason"]),
    Route::new(
        "getBalances",
        "GET",
        "/api/v1/wallet/end-users/:endUserId/balances",
    ),
    Route::new(
        "getKycLevel",
        "GET",
        "/api/v1/wallet/end-users/:endUserId/kyc-level",
    ),
    Route::new(
        "updateKycLevel",
        "PUT",
        "/api/v1/wallet/end-users/:endUserId/kyc-level",
    )
    .with_key()
    .with_body(
        &["kycLevel", "sourceRef"],
        &[
            "externalCustomerId",
            "programId",
            "reason",
            "effectiveAt",
            "expectedCurrentLevel",
        ],
    ),
    Route::new(
        "getLimits",
        "GET",
        "/api/v1/wallet/end-users/:endUserId/limits",
    )
    .with_query(&["operationType", "currency"]),
    Route::new(
        "createWallet",
        "POST",
        "/api/v1/wallet/end-users/:endUserId/wallets",
    )
    .with_key()
    .with_body(&[], &["name", "currencies"]),
    // Wallets and accounts
    Route::new(
        "createWalletAccount",
        "POST",
        "/api/v1/wallet/wallets/:walletId/accounts",
    )
    .with_key()
    .with_body(&["currency"], &[]),
    Route::new("lookupWallet", "POST", "/api/v1/wallet/wallets/lookup")
        .with_body(&[], &["programId", "phone", "walletNo"]),
    // Deposits (bank transfer in)
    Route::new(
        "listDepositInstructions",
        "GET",
        "/api/v1/wallet/end-users/:endUserId/deposit-instructions",
    ),
    Route::new(
        "createDepositInstruction",
        "POST",
        "/api/v1/wallet/end-users/:endUserId/deposit-instructions",
    )
    .with_body(&["walletAccountId"], &["currency"]),
    Route::new(
        "simulateInboundCredit",
        "POST",
        "/api/v1/wallet/bank-integrations/inbound-credits",
    )
    .with_body(
        &["programId", "railId", "amount", "currency", "rawReference"],
        &["senderName", "senderIban", "creditedIban", "receiptNumber"],
    ),
    // Corporate accounts
    Route::new(
        "createCorporateAccount",
        "POST",
        "/api/v1/wallet/corporate-accounts",
    )
    .with_key()
    .with_body(
        &["programId", "code", "name", "ownerEndUserId"],
        &["fundingCurrency"],
    ),
    Route::new(
        "listCorporateAccounts",
        "GET",
        "/api/v1/wallet/corporate-accounts",
    )
    .with_query(&["programId", "status", "limit", "cursor"]),
    Route::new(
        "importCorporateEmployees",
        "POST",
        "/api/v1/wallet/corporate-accounts/:corporateAccountId/employees/import",
    )
    .with_key()
    .with_body(&["rows"], &[]),
    Route::new(
        "offboardCorporateEmployees",
        "POST",
        "/api/v1/wallet/corporate-accounts/:corporateAccountId/employees/offboard",
    )
    .with_key()
    .with_body(&["rows"], &["reason"]),
    // Fees
    Route::new("quoteFees", "POST", "/api/v1/wallet/fees/quote").with_body(
        &["programId", "operationType", "currency", "amount"],
        &["at"],
    ),
    // Wallet-to-wallet transfers
    Route::new("quoteW2W", "POST", "/api/v1/wallet/transfers/w2w/quote").with_body(
        &["endUserId", "sourceWalletAccountId", "amount", "currency"],
        &["destinationWalletNo", "destinationPhone", "paymentPurpose"],
    ),
    Route::new("initiateW2W", "POST", "/api/v1/wallet/transfers/w2w")
        .with_key()
        .with_body(
            &["endUserId", "sourceWalletAccountId", "amount", "currency"],
            &[
                "destinationWalletNo",
                "destinationPhone",
                "paymentPurpose",
                "scheduleVersionId",
                "otpDestination",
                "otpChannel",
                "locale",
            ],
        ),
    Route::new(
        "completeW2W",
        "POST",
        "/api/v1/wallet/transfers/w2w/:operationId/complete",
    )
    .with_key()
    .with_body(COMPLETE, COMPLETE_OPTIONAL),
    // Transfers to an IBAN
    Route::new(
        "quoteW2Iban",
        "POST",
        "/api/v1/wallet/transfers/w2iban/quote",
    )
    .with_body(IBAN_REQUIRED, &["bankCode", "paymentPurpose"]),
    Route::new("initiateW2Iban", "POST", "/api/v1/wallet/transfers/w2iban")
        .with_key()
        .with_body(IBAN_REQUIRED, IBAN_INITIATE_OPTIONAL),
    Route::new(
        "completeW2Iban",
        "POST",
        "/api/v1/wallet/transfers/w2iban/:operationId/complete",
    )
    .with_key()
    .with_body(COMPLETE, COMPLETE_OPTIONAL),
    // Withdrawals to the end user's own bank account
    Route::new(
        "quoteBankWithdrawal",
        "POST",
        "/api/v1/wallet/withdrawals/bank/quote",
    )
    .with_body(IBAN_REQUIRED, &["bankCode", "paymentPurpose"]),
    Route::new(
        "initiateBankWithdrawal",
        "POST",
        "/api/v1/wallet/withdrawals/bank",
    )
    .with_key()
    .with_body(IBAN_REQUIRED, IBAN_INITIATE_OPTIONAL),
    Route::new(
        "completeBankWithdrawal",
        "POST",
        "/api/v1/wallet/withdrawals/bank/:operationId/complete",
    )
    .with_key()
    .with_body(COMPLETE, COMPLETE_OPTIONAL),
    Route::new(
        "getWithdrawal",
        "GET",
        "/api/v1/wallet/withdrawals/:operationId",
    ),
    // Operations
    Route::new("listOperations", "GET", "/api/v1/wallet/operations").with_query(&[
        "endUserId",
        "walletAccountId",
        "status",
        "operationType",
        "destinationWalletAccountId",
        "limit",
        "cursor",
    ]),
    Route::new(
        "getOperation",
        "GET",
        "/api/v1/wallet/operations/:operationId",
    ),
    Route::new(
        "getOperationReceipt",
        "GET",
        "/api/v1/wallet/operations/:operationId/receipt",
    ),
    // Top-ups. The payment provider requires the buyer and the billing
    // address (`buyer.ip`: the user's browser IP); a missing field is 400
    // `payment.order.field_required` with the paths in `details.fields`.
    Route::new("startCardTopup", "POST", "/api/v1/wallet/topups/card")
        .with_key()
        .with_body(
            &[
                "creditWalletAccountId",
                "amount",
                "currency",
                "buyer",
                "billingAddress",
            ],
            &[
                "fundingMerchantId",
                "savedCardUserKey",
                "savedCardToken",
                "savedConsumerToken",
                "savedPublicCardStorageToken",
                "returnUrl",
                "shippingAddress",
            ],
        ),
    Route::new("getTopup", "GET", "/api/v1/wallet/topups/:operationId"),
    // The gateway DTO marks approverPrincipal optional, but the wallet service
    // refuses the refund without it: it is required in practice.
    Route::new(
        "refundTopup",
        "POST",
        "/api/v1/wallet/topups/:operationId/refund",
    )
    .with_body(&["reason", "requesterPrincipal", "approverPrincipal"], &[]),
    Route::new("agentTopup", "POST", "/api/v1/wallet/agent-topups")
        .with_key()
        .with_body(
            &["agentId", "creditWalletAccountId", "amount", "currency"],
            &[],
        ),
    // Payments to the merchant
    Route::new("requestPayment", "POST", "/api/v1/wallet/payments/buy")
        .with_key()
        .with_body(
            &["amount", "currency", "paymentPurpose"],
            &[
                "buyerWalletNo",
                "buyerPhone",
                "orderRef",
                "cashierRef",
                "otpDestination",
                "otpChannel",
                "locale",
            ],
        ),
    Route::new(
        "approvePayment",
        "POST",
        "/api/v1/wallet/payments/:operationId/approve",
    )
    .with_key()
    .with_body(
        &["buyerEndUserId", "integrityHash", "nonce"],
        COMPLETE_OPTIONAL,
    ),
    Route::new("collectByCode", "POST", "/api/v1/wallet/collect")
        .with_key()
        .with_body(
            &["code", "amount", "currency"],
            &["orderRef", "paymentPurpose", "locale"],
        ),
    Route::new("getPayment", "GET", "/api/v1/wallet/payments/:operationId"),
    Route::new(
        "getBusinessCommission",
        "GET",
        "/api/v1/wallet/business-accounts/:businessAccountId/commission",
    ),
    // QR codes
    Route::new("createQr", "POST", "/api/v1/wallet/qr")
        .with_key()
        .with_body(
            &["qrType", "currency"],
            &[
                "targetEndUserId",
                "targetExternalCustomerId",
                "programId",
                "amount",
                "expiresInSeconds",
                "metadata",
            ],
        ),
    Route::new("listQr", "GET", "/api/v1/wallet/qr").with_query(&[
        "programId",
        "qrType",
        "status",
        "targetEndUserId",
        "limit",
        "cursor",
    ]),
    Route::new("getQr", "GET", "/api/v1/wallet/qr/:qrId"),
    Route::new("getQrImage", "GET", "/api/v1/wallet/qr/:qrId/image")
        .with_binary()
        .with_query(&["format", "scale"]),
    Route::new("revokeQr", "POST", "/api/v1/wallet/qr/:qrId/revoke"),
    Route::new("parseQr", "POST", "/api/v1/wallet/qr/parse").with_body(&["payload"], &[]),
    // Bulk payouts
    Route::new("createBulkPayout", "POST", "/api/v1/wallet/bulk-payouts")
        .with_key()
        .with_body(
            &["programId", "sourceWalletAccountId", "currency", "items"],
            &["reason"],
        ),
    Route::new("listBulkPayouts", "GET", "/api/v1/wallet/bulk-payouts").with_query(&[
        "programId",
        "status",
        "limit",
    ]),
    Route::new(
        "getBulkPayout",
        "GET",
        "/api/v1/wallet/bulk-payouts/:batchId",
    ),
    Route::new(
        "listBulkPayoutItems",
        "GET",
        "/api/v1/wallet/bulk-payouts/:batchId/items",
    )
    .with_query(&["status", "limit"]),
    Route::new(
        "submitBulkPayout",
        "POST",
        "/api/v1/wallet/bulk-payouts/:batchId/submit",
    ),
    Route::new(
        "approveBulkPayout",
        "POST",
        "/api/v1/wallet/bulk-payouts/:batchId/approve",
    )
    .with_body(&["approve"], &["reason"]),
    Route::new(
        "rejectBulkPayout",
        "POST",
        "/api/v1/wallet/bulk-payouts/:batchId/reject",
    )
    .with_body(&[], &["reason"]),
    Route::new(
        "processBulkPayout",
        "POST",
        "/api/v1/wallet/bulk-payouts/:batchId/process",
    ),
    Route::new(
        "retryFailedBulkPayout",
        "POST",
        "/api/v1/wallet/bulk-payouts/:batchId/retry-failed",
    ),
];

/// Fields of the nested rows / items the gateway validates element by
/// element: `(route, list field, element fields)`.
pub const WALLET_NESTED_FIELDS: &[(&str, &str, NestedFields)] = &[
    (
        "importCorporateEmployees",
        "rows",
        NestedFields::new(&["phone"], &["displayName", "externalCustomerId", "kycRef"]),
    ),
    (
        "offboardCorporateEmployees",
        "rows",
        NestedFields::new(&[], &["endUserId", "externalCustomerId"]),
    ),
    (
        "createBulkPayout",
        "items",
        NestedFields::new(&["targetType", "targetRef", "amount"], &["beneficiaryName"]),
    ),
];

/// One route of [`WALLET_ROUTES`] by name.
pub fn wallet_route(name: &str) -> Option<&'static Route> {
    WALLET_ROUTES.iter().find(|route| route.name == name)
}
