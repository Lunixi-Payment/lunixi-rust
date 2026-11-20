//! `client.wallet()`: the merchant server-to-server wallet surface.

use super::values::{
    WALLET_AMOUNT_FIELDS, WALLET_AMOUNT_MAX_DIGITS, WALLET_NESTED_FIELDS, WALLET_ROUTES,
    wallet_qr_image_format,
};
use crate::{
    Client, Error, Request, Response, Result,
    route_table::{
        CursorPage, Route, check_fields, fill_path, find, minor_text, object_of, query_pairs,
        to_page, unwrap, valid_wallet_key,
    },
};
use serde_json::{Map, Value, json};
use std::fmt;

const IMAGE_ACCEPT: &str = "image/svg+xml, image/png, application/json";

/// Wallet — `/api/v1/wallet/*`, the merchant server-to-server surface of the
/// closed-loop wallet (`client.wallet()`).
///
/// Every route is signed, reads included; the merchant, its programs and its
/// end users come from the signing key. The key needs the `WALLET_SERVICE`
/// product (403 `PRODUCT_NOT_ENABLED`) and the wallet permission behind the
/// route (403 `WALLET_INSUFFICIENT_SCOPE`).
///
/// Bodies and filters are JSON objects checked against
/// [`WALLET_ROUTES`](crate::WALLET_ROUTES) before anything is sent: an
/// unknown field, a missing required field, an amount that is not
/// minor-unit text (`"12550"` = 125.50 TRY) or an invalid `Idempotency-Key`
/// (1–128 printable ASCII characters, trimmed) is `Error::Invalid`.
/// Answers are the envelope's `data`; lists are [`CursorPage`].
///
/// Money moves in two steps: `initiate_*` reserves the limit and returns
/// `{operationId, integrityHash, nonce, otpRequired, …}`; `complete_*` with
/// the same `integrityHash` and `nonce` (and the OTP when `otpRequired`)
/// commits it. The complete calls put `operationId` in the body themselves.
pub struct Wallet<'a> {
    client: &'a Client,
}
impl Client {
    /// Returns the wallet facade; no independent HTTP client is constructed.
    pub fn wallet(&self) -> Wallet<'_> {
        Wallet { client: self }
    }
}

/// A QR code image (`qr().image`): SVG or PNG bytes.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WalletQrImage {
    /// `image/svg+xml` or `image/png`.
    pub content_type: Option<String>,
    /// The image.
    pub bytes: Vec<u8>,
}
impl fmt::Debug for WalletQrImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WalletQrImage")
            .field("content_type", &self.content_type)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

macro_rules! sub_facades {
    ($($(#[$doc:meta])* $name:ident => $accessor:ident;)*) => {
        $(
            $(#[$doc])*
            pub struct $name<'a> {
                client: &'a Client,
            }
        )*
        impl<'a> Wallet<'a> {
            $(
                $(#[$doc])*
                pub fn $accessor(&self) -> $name<'a> {
                    $name { client: self.client }
                }
            )*
        }
    };
}

sub_facades! {
    /// Wallet programs of the signing merchant.
    WalletPrograms => programs;
    /// End users and their wallets, balances, KYC level and limits.
    WalletEndUsers => end_users;
    /// Currency accounts of a wallet and wallet lookup.
    WalletAccounts => wallets;
    /// Bank-transfer deposit instructions.
    WalletDeposits => deposits;
    /// Corporate accounts and their employees.
    WalletCorporate => corporate;
    /// Fee quotes.
    WalletFees => fees;
    /// Wallet-to-wallet and wallet-to-IBAN transfers (two steps).
    WalletTransfers => transfers;
    /// Withdrawals to the end user's own bank account (two steps).
    WalletWithdrawals => withdrawals;
    /// Wallet operations and receipts.
    WalletOperations => operations;
    /// Card and agent top-ups.
    WalletTopups => topups;
    /// Payments to the merchant (wallet terminal of the signing key).
    WalletPayments => payments;
    /// QR codes.
    WalletQr => qr;
    /// Bulk payouts (maker-checker: the approver is a different key).
    WalletBulkPayouts => bulk_payouts;
}

impl Wallet<'_> {
    /// Checks the key reaches the wallet (signature, product, a
    /// `wallet:*:manage` permission) (`GET /ping`).
    pub async fn ping(&self) -> Result<Value> {
        data(self.client, "ping", &[], &Value::Null, None, None).await
    }
}

impl WalletPrograms<'_> {
    /// The wallet programs of the signing merchant (`GET /programs/me`).
    pub async fn list_mine(&self) -> Result<CursorPage> {
        page(self.client, "listMyPrograms", &[], &Value::Null).await
    }
    /// Sets the end user whose wallet collects the program's merchant
    /// payments (`PUT /programs/{programId}/collection-anchor`):
    /// `{collectionEndUserId?, reason?, expectedVersion?}`; leaving
    /// `collectionEndUserId` out clears the anchor.
    pub async fn set_collection_anchor(&self, program_id: &str, request: &Value) -> Result<Value> {
        let body = body_or_empty(request);
        data(
            self.client,
            "setCollectionAnchor",
            &[program_id],
            &Value::Null,
            Some(&body),
            None,
        )
        .await
    }
}

impl WalletEndUsers<'_> {
    /// Creates an end user and their default wallet (`POST /end-users`; key required).
    pub async fn create(&self, request: &Value, idempotency_key: &str) -> Result<Value> {
        let body = Some(request);
        data(
            self.client,
            "createEndUser",
            &[],
            &Value::Null,
            body,
            Some(idempotency_key),
        )
        .await
    }
    /// End users (`GET /end-users`): `programId`, `accountType`, `status`,
    /// `corporateAccountId`, `merchantCategoryState`, `limit`, `cursor`.
    pub async fn list(&self, filters: &Value) -> Result<CursorPage> {
        page(self.client, "listEndUsers", &[], filters).await
    }
    /// One end user (`GET /end-users/{endUserId}`).
    pub async fn get(&self, end_user_id: &str) -> Result<Value> {
        data(
            self.client,
            "getEndUser",
            &[end_user_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
    /// Finds an end user by your own customer id
    /// (`GET /end-users/by-external/{externalCustomerId}`).
    pub async fn get_by_external_id(
        &self,
        external_customer_id: &str,
        program_id: Option<&str>,
    ) -> Result<Value> {
        let query = json!({ "programId": program_id });
        data(
            self.client,
            "getEndUserByExternalId",
            &[external_customer_id],
            &query,
            None,
            None,
        )
        .await
    }
    /// Freezes an end user (`POST /end-users/{endUserId}/freeze`; key required).
    pub async fn freeze(
        &self,
        end_user_id: &str,
        reason: Option<&str>,
        idempotency_key: &str,
    ) -> Result<Value> {
        let body = json!({ "reason": reason });
        data(
            self.client,
            "freezeEndUser",
            &[end_user_id],
            &Value::Null,
            Some(&body),
            Some(idempotency_key),
        )
        .await
    }
    /// Balances of every account of the end user (`GET /end-users/{endUserId}/balances`).
    pub async fn get_balances(&self, end_user_id: &str) -> Result<Value> {
        data(
            self.client,
            "getBalances",
            &[end_user_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
    /// The end user's KYC level (`GET /end-users/{endUserId}/kyc-level`).
    pub async fn get_kyc_level(&self, end_user_id: &str) -> Result<Value> {
        data(
            self.client,
            "getKycLevel",
            &[end_user_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
    /// Records the KYC level from your own verification
    /// (`PUT /end-users/{endUserId}/kyc-level`; `sourceRef` = your KYC
    /// session id; key required).
    pub async fn update_kyc_level(
        &self,
        end_user_id: &str,
        request: &Value,
        idempotency_key: &str,
    ) -> Result<Value> {
        data(
            self.client,
            "updateKycLevel",
            &[end_user_id],
            &Value::Null,
            Some(request),
            Some(idempotency_key),
        )
        .await
    }
    /// Limit usage per operation type (`GET /end-users/{endUserId}/limits`);
    /// `operationType` and `currency` narrow the answer.
    pub async fn get_limits(&self, end_user_id: &str, filters: &Value) -> Result<Value> {
        data(
            self.client,
            "getLimits",
            &[end_user_id],
            filters,
            None,
            None,
        )
        .await
    }
    /// Opens another wallet for the end user
    /// (`POST /end-users/{endUserId}/wallets`: `{name?, currencies?}`; key required).
    pub async fn create_wallet(
        &self,
        end_user_id: &str,
        request: &Value,
        idempotency_key: &str,
    ) -> Result<Value> {
        let body = body_or_empty(request);
        data(
            self.client,
            "createWallet",
            &[end_user_id],
            &Value::Null,
            Some(&body),
            Some(idempotency_key),
        )
        .await
    }
}

impl WalletAccounts<'_> {
    /// Adds a currency account to a wallet (`POST /wallets/{walletId}/accounts`; key required).
    pub async fn create_account(
        &self,
        wallet_id: &str,
        currency: &str,
        idempotency_key: &str,
    ) -> Result<Value> {
        let body = json!({ "currency": currency });
        data(
            self.client,
            "createWalletAccount",
            &[wallet_id],
            &Value::Null,
            Some(&body),
            Some(idempotency_key),
        )
        .await
    }
    /// Resolves a wallet number or phone to `{exists, walletNo, maskedName}`
    /// before a transfer (`POST /wallets/lookup`).
    pub async fn lookup(&self, request: &Value) -> Result<Value> {
        data(
            self.client,
            "lookupWallet",
            &[],
            &Value::Null,
            Some(request),
            None,
        )
        .await
    }
}

impl WalletDeposits<'_> {
    /// Bank transfer details the end user pays into to top up
    /// (`GET /end-users/{endUserId}/deposit-instructions`).
    pub async fn list_instructions(&self, end_user_id: &str) -> Result<Value> {
        data(
            self.client,
            "listDepositInstructions",
            &[end_user_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
    /// Creates a deposit instruction (`{walletAccountId, currency?}`).
    pub async fn create_instruction(&self, end_user_id: &str, request: &Value) -> Result<Value> {
        data(
            self.client,
            "createDepositInstruction",
            &[end_user_id],
            &Value::Null,
            Some(request),
            None,
        )
        .await
    }
    /// Test only: books an incoming bank transfer as the bank rail would
    /// (`POST /bank-integrations/inbound-credits`).
    pub async fn simulate_inbound_credit(&self, request: &Value) -> Result<Value> {
        data(
            self.client,
            "simulateInboundCredit",
            &[],
            &Value::Null,
            Some(request),
            None,
        )
        .await
    }
}

impl WalletCorporate<'_> {
    /// Creates a corporate account (`POST /corporate-accounts`; key required).
    pub async fn create(&self, request: &Value, idempotency_key: &str) -> Result<Value> {
        data(
            self.client,
            "createCorporateAccount",
            &[],
            &Value::Null,
            Some(request),
            Some(idempotency_key),
        )
        .await
    }
    /// Corporate accounts (`programId`, `status`, `limit`, `cursor`).
    pub async fn list(&self, filters: &Value) -> Result<CursorPage> {
        page(self.client, "listCorporateAccounts", &[], filters).await
    }
    /// Imports up to 1000 employees at once, all or nothing
    /// (`rows: [{phone, displayName?, externalCustomerId?, kycRef?}]`; key required).
    pub async fn import_employees(
        &self,
        corporate_account_id: &str,
        rows: &Value,
        idempotency_key: &str,
    ) -> Result<Value> {
        let body = json!({ "rows": rows });
        data(
            self.client,
            "importCorporateEmployees",
            &[corporate_account_id],
            &Value::Null,
            Some(&body),
            Some(idempotency_key),
        )
        .await
    }
    /// Offboards employees (`rows: [{endUserId? | externalCustomerId?}]`; key required).
    pub async fn offboard_employees(
        &self,
        corporate_account_id: &str,
        rows: &Value,
        reason: Option<&str>,
        idempotency_key: &str,
    ) -> Result<Value> {
        let body = json!({ "rows": rows, "reason": reason });
        data(
            self.client,
            "offboardCorporateEmployees",
            &[corporate_account_id],
            &Value::Null,
            Some(&body),
            Some(idempotency_key),
        )
        .await
    }
}

impl WalletFees<'_> {
    /// The fee, tax and total charge an operation would cost (`POST /fees/quote`).
    pub async fn quote(&self, request: &Value) -> Result<Value> {
        data(
            self.client,
            "quoteFees",
            &[],
            &Value::Null,
            Some(request),
            None,
        )
        .await
    }
}

impl WalletTransfers<'_> {
    /// Quotes a wallet-to-wallet transfer (`POST /transfers/w2w/quote`).
    pub async fn quote_w2w(&self, request: &Value) -> Result<Value> {
        data(
            self.client,
            "quoteW2W",
            &[],
            &Value::Null,
            Some(request),
            None,
        )
        .await
    }
    /// Starts a wallet-to-wallet transfer (`POST /transfers/w2w`; key required).
    pub async fn initiate_w2w(&self, request: &Value, idempotency_key: &str) -> Result<Value> {
        data(
            self.client,
            "initiateW2W",
            &[],
            &Value::Null,
            Some(request),
            Some(idempotency_key),
        )
        .await
    }
    /// Commits a wallet-to-wallet transfer
    /// (`POST /transfers/w2w/{operationId}/complete`; key required).
    pub async fn complete_w2w(
        &self,
        operation_id: &str,
        request: &Value,
        idempotency_key: &str,
    ) -> Result<Value> {
        complete(
            self.client,
            "completeW2W",
            operation_id,
            request,
            idempotency_key,
        )
        .await
    }
    /// Quotes a transfer to an IBAN (`POST /transfers/w2iban/quote`).
    pub async fn quote_w2_iban(&self, request: &Value) -> Result<Value> {
        data(
            self.client,
            "quoteW2Iban",
            &[],
            &Value::Null,
            Some(request),
            None,
        )
        .await
    }
    /// Starts a transfer to an IBAN (`POST /transfers/w2iban`; key required).
    pub async fn initiate_w2_iban(&self, request: &Value, idempotency_key: &str) -> Result<Value> {
        data(
            self.client,
            "initiateW2Iban",
            &[],
            &Value::Null,
            Some(request),
            Some(idempotency_key),
        )
        .await
    }
    /// Commits a transfer to an IBAN
    /// (`POST /transfers/w2iban/{operationId}/complete`; key required).
    pub async fn complete_w2_iban(
        &self,
        operation_id: &str,
        request: &Value,
        idempotency_key: &str,
    ) -> Result<Value> {
        complete(
            self.client,
            "completeW2Iban",
            operation_id,
            request,
            idempotency_key,
        )
        .await
    }
}

impl WalletWithdrawals<'_> {
    /// Quotes a withdrawal (`POST /withdrawals/bank/quote`).
    pub async fn quote(&self, request: &Value) -> Result<Value> {
        data(
            self.client,
            "quoteBankWithdrawal",
            &[],
            &Value::Null,
            Some(request),
            None,
        )
        .await
    }
    /// Starts a withdrawal (`POST /withdrawals/bank`; key required).
    pub async fn initiate(&self, request: &Value, idempotency_key: &str) -> Result<Value> {
        data(
            self.client,
            "initiateBankWithdrawal",
            &[],
            &Value::Null,
            Some(request),
            Some(idempotency_key),
        )
        .await
    }
    /// Commits a withdrawal (`POST /withdrawals/bank/{operationId}/complete`; key required).
    pub async fn complete(
        &self,
        operation_id: &str,
        request: &Value,
        idempotency_key: &str,
    ) -> Result<Value> {
        complete(
            self.client,
            "completeBankWithdrawal",
            operation_id,
            request,
            idempotency_key,
        )
        .await
    }
    /// `{operation, history}` (`GET /withdrawals/{operationId}`).
    pub async fn get(&self, operation_id: &str) -> Result<Value> {
        data(
            self.client,
            "getWithdrawal",
            &[operation_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
}

impl WalletOperations<'_> {
    /// Operations (`endUserId`, `walletAccountId`, `status`, `operationType`,
    /// `destinationWalletAccountId`, `limit`, `cursor`).
    pub async fn list(&self, filters: &Value) -> Result<CursorPage> {
        page(self.client, "listOperations", &[], filters).await
    }
    /// `{operation, history}` (`GET /operations/{operationId}`).
    pub async fn get(&self, operation_id: &str) -> Result<Value> {
        data(
            self.client,
            "getOperation",
            &[operation_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
    /// The operation's receipt (`GET /operations/{operationId}/receipt`).
    pub async fn get_receipt(&self, operation_id: &str) -> Result<Value> {
        data(
            self.client,
            "getOperationReceipt",
            &[operation_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
}

impl WalletTopups<'_> {
    /// Starts a card top-up (`POST /topups/card`; key required). Render
    /// `checkoutFormContent` (or follow `redirectUrl` / `threeDsHtml`); the
    /// wallet is credited when the payment completes (`wallet.topup.completed`).
    /// `buyer` and `billingAddress` are required (`shippingAddress` optional);
    /// a missing field fails before sending with
    /// [`Error::MissingOrderDetails`](crate::Error::MissingOrderDetails).
    pub async fn start_card(&self, request: &Value, idempotency_key: &str) -> Result<Value> {
        crate::order_details::assert_order_details(
            request,
            "A wallet card top-up",
            crate::OrderDetailsScope::BuyerAndBillingAddress,
        )?;
        data(
            self.client,
            "startCardTopup",
            &[],
            &Value::Null,
            Some(request),
            Some(idempotency_key),
        )
        .await
    }
    /// `{operation, history}` (`GET /topups/{operationId}`).
    pub async fn get(&self, operation_id: &str) -> Result<Value> {
        data(
            self.client,
            "getTopup",
            &[operation_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
    /// Refunds a card top-up to the card (`POST /topups/{operationId}/refund`).
    /// Four eyes: `approverPrincipal` is required and must differ from
    /// `requesterPrincipal`.
    pub async fn refund(&self, operation_id: &str, request: &Value) -> Result<Value> {
        data(
            self.client,
            "refundTopup",
            &[operation_id],
            &Value::Null,
            Some(request),
            None,
        )
        .await
    }
    /// Cash top-up through an agent of the program (`POST /agent-topups`; key required).
    pub async fn agent(&self, request: &Value, idempotency_key: &str) -> Result<Value> {
        data(
            self.client,
            "agentTopup",
            &[],
            &Value::Null,
            Some(request),
            Some(idempotency_key),
        )
        .await
    }
}

impl WalletPayments<'_> {
    /// Asks the buyer (by wallet number or phone) to pay (`POST /payments/buy`;
    /// key required); approve it with `approve`. A `businessAccountId` in
    /// the body is refused: the creditor is the signing key's wallet terminal.
    pub async fn request(&self, request: &Value, idempotency_key: &str) -> Result<Value> {
        data(
            self.client,
            "requestPayment",
            &[],
            &Value::Null,
            Some(request),
            Some(idempotency_key),
        )
        .await
    }
    /// Approves a requested payment for the buyer
    /// (`POST /payments/{operationId}/approve`; key required).
    pub async fn approve(
        &self,
        operation_id: &str,
        request: &Value,
        idempotency_key: &str,
    ) -> Result<Value> {
        data(
            self.client,
            "approvePayment",
            &[operation_id],
            &Value::Null,
            Some(request),
            Some(idempotency_key),
        )
        .await
    }
    /// Collects with the payment code the buyer shows (`POST /collect`; key required).
    pub async fn collect(&self, request: &Value, idempotency_key: &str) -> Result<Value> {
        data(
            self.client,
            "collectByCode",
            &[],
            &Value::Null,
            Some(request),
            Some(idempotency_key),
        )
        .await
    }
    /// `{operation, history}` (`GET /payments/{operationId}`).
    pub async fn get(&self, operation_id: &str) -> Result<Value> {
        data(
            self.client,
            "getPayment",
            &[operation_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
    /// The commission of a business account
    /// (`GET /business-accounts/{businessAccountId}/commission`).
    pub async fn get_commission(&self, business_account_id: &str) -> Result<Value> {
        data(
            self.client,
            "getBusinessCommission",
            &[business_account_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
}

impl WalletQr<'_> {
    /// Creates a QR code (`POST /qr`; key required). Static codes carry no
    /// amount; dynamic ones require it.
    pub async fn create(&self, request: &Value, idempotency_key: &str) -> Result<Value> {
        data(
            self.client,
            "createQr",
            &[],
            &Value::Null,
            Some(request),
            Some(idempotency_key),
        )
        .await
    }
    /// QR codes (`programId`, `qrType`, `status`, `targetEndUserId`, `limit`, `cursor`).
    pub async fn list(&self, filters: &Value) -> Result<CursorPage> {
        page(self.client, "listQr", &[], filters).await
    }
    /// One QR code (`GET /qr/{qrId}`).
    pub async fn get(&self, qr_id: &str) -> Result<Value> {
        data(self.client, "getQr", &[qr_id], &Value::Null, None, None).await
    }
    /// The QR as an image (`GET /qr/{qrId}/image`): `format`
    /// [`wallet_qr_image_format`] (`svg` by default, or `png`), `scale` 1–20.
    pub async fn image(
        &self,
        qr_id: &str,
        format: Option<&str>,
        scale: Option<u8>,
    ) -> Result<WalletQrImage> {
        if format.is_some_and(|f| !wallet_qr_image_format::ALL.contains(&f)) {
            return Err(Error::Invalid("QR image format (svg or png)"));
        }
        if scale.is_some_and(|s| !(1..=20).contains(&s)) {
            return Err(Error::Invalid("QR image scale (an integer 1-20)"));
        }
        let query = json!({ "format": format, "scale": scale });
        let response = call(self.client, "getQrImage", &[qr_id], &query, None, None).await?;
        if response.envelope.is_some() || response.bytes.is_empty() {
            return Err(Error::Response("QR image"));
        }
        Ok(WalletQrImage {
            content_type: response.content_type,
            bytes: response.bytes,
        })
    }
    /// Revokes a QR code (`POST /qr/{qrId}/revoke`).
    pub async fn revoke(&self, qr_id: &str) -> Result<Value> {
        data(self.client, "revokeQr", &[qr_id], &Value::Null, None, None).await
    }
    /// Decodes a scanned QR payload (`POST /qr/parse`).
    pub async fn parse(&self, payload: &str) -> Result<Value> {
        let body = json!({ "payload": payload });
        data(self.client, "parseQr", &[], &Value::Null, Some(&body), None).await
    }
}

impl WalletBulkPayouts<'_> {
    /// Creates a bulk payout (`POST /bulk-payouts`; key required). Items are
    /// `{targetType, targetRef, amount, beneficiaryName?}`.
    pub async fn create(&self, request: &Value, idempotency_key: &str) -> Result<Value> {
        data(
            self.client,
            "createBulkPayout",
            &[],
            &Value::Null,
            Some(request),
            Some(idempotency_key),
        )
        .await
    }
    /// Bulk payouts (`programId`, `status`, `limit`).
    pub async fn list(&self, filters: &Value) -> Result<CursorPage> {
        page(self.client, "listBulkPayouts", &[], filters).await
    }
    /// One bulk payout (`GET /bulk-payouts/{batchId}`).
    pub async fn get(&self, batch_id: &str) -> Result<Value> {
        data(
            self.client,
            "getBulkPayout",
            &[batch_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
    /// The items of a bulk payout (`status`, `limit`).
    pub async fn list_items(&self, batch_id: &str, filters: &Value) -> Result<CursorPage> {
        page(self.client, "listBulkPayoutItems", &[batch_id], filters).await
    }
    /// Submits the batch for approval (`POST /bulk-payouts/{batchId}/submit`).
    pub async fn submit(&self, batch_id: &str) -> Result<Value> {
        data(
            self.client,
            "submitBulkPayout",
            &[batch_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
    /// Approves (`approve: true`) or declines (`false`, with `reason`) the
    /// batch (`POST /bulk-payouts/{batchId}/approve`). The approver must be a
    /// different key than the creator.
    pub async fn approve(
        &self,
        batch_id: &str,
        approve: bool,
        reason: Option<&str>,
    ) -> Result<Value> {
        let body = json!({ "approve": approve, "reason": reason });
        data(
            self.client,
            "approveBulkPayout",
            &[batch_id],
            &Value::Null,
            Some(&body),
            None,
        )
        .await
    }
    /// Rejects the batch (`POST /bulk-payouts/{batchId}/reject`).
    pub async fn reject(&self, batch_id: &str, reason: Option<&str>) -> Result<Value> {
        let body = json!({ "reason": reason });
        data(
            self.client,
            "rejectBulkPayout",
            &[batch_id],
            &Value::Null,
            Some(&body),
            None,
        )
        .await
    }
    /// Processes an approved batch (`POST /bulk-payouts/{batchId}/process`).
    pub async fn process(&self, batch_id: &str) -> Result<Value> {
        data(
            self.client,
            "processBulkPayout",
            &[batch_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
    /// Retries the failed items (`POST /bulk-payouts/{batchId}/retry-failed`).
    pub async fn retry_failed(&self, batch_id: &str) -> Result<Value> {
        data(
            self.client,
            "retryFailedBulkPayout",
            &[batch_id],
            &Value::Null,
            None,
            None,
        )
        .await
    }
}

// ── Plumbing ────────────────────────────────────────────────────────────────

fn body_or_empty(request: &Value) -> Value {
    if request.is_null() {
        json!({})
    } else {
        request.clone()
    }
}

/// The complete DTOs require `operationId` in the body as well; the path wins.
async fn complete(
    client: &Client,
    route: &str,
    operation_id: &str,
    request: &Value,
    key: &str,
) -> Result<Value> {
    let mut body = object_of(
        &body_or_empty(request),
        "wallet request must be a JSON object",
    )?
    .clone();
    if body
        .get("operationId")
        .is_some_and(|given| !given.is_null() && given.as_str() != Some(operation_id))
    {
        return Err(Error::Invalid(
            "wallet operationId in the body must match the operation being completed",
        ));
    }
    body.insert("operationId".to_owned(), Value::from(operation_id));
    data(
        client,
        route,
        &[operation_id],
        &Value::Null,
        Some(&Value::Object(body)),
        Some(key),
    )
    .await
}

fn amount_of(value: &Value) -> Result<String> {
    let text = minor_text(
        value,
        true,
        "wallet amount: minor units as digit text (\"12550\" for 125.50)",
    )?;
    if text.len() > WALLET_AMOUNT_MAX_DIGITS {
        return Err(Error::Invalid("wallet amount: at most 18 digits"));
    }
    Ok(text)
}

fn checked_object(
    value: &Value,
    required: &[&str],
    optional: &[&str],
) -> Result<Map<String, Value>> {
    let object = object_of(value, "wallet request must be a JSON object")?;
    let mut plain = check_fields(object, required, optional)?;
    for field in WALLET_AMOUNT_FIELDS {
        if let Some(amount) = plain.get(*field) {
            let text = amount_of(amount)?;
            plain.insert((*field).to_owned(), Value::String(text));
        }
    }
    Ok(plain)
}

/// Checks a body against its route: known fields only, required fields
/// present, amounts as minor-unit text, nested rows element by element.
fn check_body(route: &Route, body: &Value) -> Result<Value> {
    let mut plain = checked_object(body, route.required, route.optional)?;
    if let Some((_, list, fields)) = WALLET_NESTED_FIELDS
        .iter()
        .find(|(name, _, _)| *name == route.name)
    {
        let rows = plain
            .get(*list)
            .and_then(Value::as_array)
            .filter(|rows| !rows.is_empty())
            .ok_or(Error::Invalid("wallet rows / items: a non-empty array"))?;
        let checked = rows
            .iter()
            .map(|row| checked_object(row, fields.required, fields.optional).map(Value::Object))
            .collect::<Result<Vec<_>>>()?;
        plain.insert((*list).to_owned(), Value::Array(checked));
    }
    Ok(Value::Object(plain))
}

/// Sends one route of `WALLET_ROUTES`, signed.
async fn call(
    client: &Client,
    name: &str,
    params: &[&str],
    filters: &Value,
    body: Option<&Value>,
    key: Option<&str>,
) -> Result<Response> {
    let route = find(WALLET_ROUTES, name)?;
    let mut request = Request::new(route.http_method(), fill_path(route, params)?);
    request.query = query_pairs(route.query, filters)?;
    if route.has_body() {
        request.body = Some(check_body(route, body.unwrap_or(&json!({})))?);
    } else if body.is_some_and(|b| !b.is_null()) {
        return Err(Error::Invalid("this wallet route takes no body"));
    }
    match (route.idempotency_key, key) {
        (true, Some(key)) => {
            let key = key.trim();
            if !valid_wallet_key(key) {
                return Err(Error::Invalid(
                    "wallet Idempotency-Key: 1-128 printable ASCII characters",
                ));
            }
            request.idempotency_key = Some(key.to_owned());
        }
        (true, None) => {
            return Err(Error::Invalid(
                "stable Idempotency-Key required for this wallet route",
            ));
        }
        (false, Some(_)) => {
            return Err(Error::Invalid(
                "this wallet route does not take an Idempotency-Key",
            ));
        }
        (false, None) => {}
    }
    if route.binary {
        client.send_binary(request, IMAGE_ACCEPT).await
    } else {
        client.send(request).await
    }
}

async fn data(
    client: &Client,
    name: &str,
    params: &[&str],
    filters: &Value,
    body: Option<&Value>,
    key: Option<&str>,
) -> Result<Value> {
    Ok(unwrap(
        &call(client, name, params, filters, body, key).await?,
    ))
}

async fn page(client: &Client, name: &str, params: &[&str], filters: &Value) -> Result<CursorPage> {
    Ok(to_page(
        &call(client, name, params, filters, None, None).await?,
    ))
}
