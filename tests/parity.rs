//! Parity with the shared SDK catalog (`contracts/products/*.json` at the
//! repository root). Every operation listed below is called in a function
//! that is compiled but never run, so a missing or renamed method fails the
//! build; the test then checks that the catalog's required operations of the
//! covered products are all listed, under their Rust name (snake case of the
//! catalog leaf, or the name in `contracts/sdk-names.json`). A copy of this
//! crate without the catalog (the published package) skips the check.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use lunixi::{
    BANK_TRANSFER_ROUTES, Client, CreatePaymentLinkRequest, DirectMethodPaymentRequest,
    IDENTITY_ROUTES, InstallmentOptionsRequest, IntentRequest, KYC_ROUTES, MARKETPLACE_ROUTES,
    MarketplaceIntentRequest, PaymentLinkActionOptions, PaymentLinkImageUpload,
    PaymentLinkSendRequest, Route, SANCTION_ROUTES, SUBSCRIPTION_ROUTES, StoreCardRequest,
    WALLET_ROUTES,
};
use serde_json::{Value, json};
use std::path::Path;

/// Products whose required operations this SDK carries.
/// (catalog file, operation prefix)
const PRODUCTS: &[(&str, &str)] = &[
    ("payments", "payments."),
    ("fraud", "fraud."),
    ("payment-links", "paymentLinks."),
    ("reference", "reference."),
    ("physical-pos", "physicalPos"),
    ("wallet", "wallet."),
    ("bank-connect", "bankConnect."),
    ("sanction", "sanction."),
    ("insurance", "insurance."),
    ("subscriptions", "subscriptions."),
    ("kyc", "kyc."),
    ("identity", "identity."),
    ("marketplace", "marketplace."),
];

macro_rules! operations {
    ($client:ident, $args:ident; $($op:literal => $facade:ident $(/ $sub:ident)? . $method:ident ( $($arg:expr),* );)*) => {
        /// (catalog operation, Rust method name)
        const OPERATIONS: &[(&str, &str)] = &[$(($op, stringify!($method))),*];
        #[allow(dead_code, unused_variables)]
        async fn compile_every_operation($client: &Client, $args: &Args) {
            $( let _ = $client.$facade()$(.$sub())?.$method($($arg),*).await; )*
        }
    };
}

#[allow(dead_code)]
struct Args {
    intent: IntentRequest,
    direct: DirectMethodPaymentRequest,
    installments: InstallmentOptionsRequest,
    card: StoreCardRequest,
    marketplace: MarketplaceIntentRequest,
    link: CreatePaymentLinkRequest,
    link_action: PaymentLinkActionOptions,
    link_send: PaymentLinkSendRequest,
    image: PaymentLinkImageUpload,
    filters: lunixi::BankTransferExceptionFilters,
    body: Value,
}

operations! { client, a;
    "payments.createIntent" => payments.create_intent(&a.intent, "k");
    "payments.get" => payments.get("p");
    "payments.list" => payments.list(vec![]);
    "payments.listPaymentMethods" => payments.list_payment_methods(None);
    "payments.capture" => payments.capture("p", None, "k");
    "payments.refund" => payments.refund("p", None, None, "k");
    "payments.void" => payments.void("p", "k");
    "payments.directMethodPayment" => payments.direct_method_payment(&a.direct, "k");
    "payments.confirmPaymentAction" => payments.confirm_payment_action("p", "a", json!({}));
    "payments.resendPaymentAction" => payments.resend_payment_action("p", "a");
    "payments.getPromissoryNotes" => payments.get_promissory_notes("p");
    "payments.cancelPromissoryNotes" => payments.cancel_promissory_notes("p", None, None);
    "payments.downloadPromissoryNoteDocument" => payments.download_promissory_note_document("p", None);
    "payments.listBankTransferExceptions" => payments.list_bank_transfer_exceptions(&a.filters);
    "payments.acceptBankTransferException" => payments.accept_bank_transfer_exception("e");
    "payments.markBankTransferExceptionRefunded" => payments.mark_bank_transfer_exception_refunded("e");
    "payments.createSplitPayment" => payments.split(json!({}), "k");
    "payments.getSplitPayment" => payments.get_split("s");
    "payments.listSplitPayments" => payments.list_splits(vec![]);
    "payments.refundSplitPayment" => payments.refund_split("s", "1", None, "k");
    "payments.binInfo" => payments.bin_info("411111");
    "payments.installmentOptions" => payments.installment_options(&a.installments);
    "payments.cardTaxonomy" => payments.card_taxonomy();
    "payments.storeCard" => payments.store_card(&a.card, "k");
    "payments.listStoredCards" => payments.list_stored_cards("u");
    "payments.getStoredCard" => payments.get_stored_card("t");
    "payments.deactivateStoredCard" => payments.deactivate_stored_card("t", "k");
    "payments.createMarketplaceIntent" => payments.create_marketplace_intent(&a.marketplace, "k");
    "payments.failoverRecoveryAnalytics" => payments.failover_recovery_analytics(vec![]);
    "payments.providerCredentialAnalytics" => payments.provider_credential_analytics("c", vec![]);
    "fraud.evaluate" => fraud.evaluate(json!({}));
    "fraud.events.record" => fraud.record_event(json!({}), "k");
    "fraud.events.recordBatch" => fraud.record_batch(vec![], "k");
    "fraud.decisions.list" => fraud.decisions(vec![]);
    "fraud.decisions.get" => fraud.decision("t");
    "fraud.decisions.latestForPayment" => fraud.latest_for_payment("p");
    "paymentLinks.create" => payment_links.create(&a.link, "k");
    "paymentLinks.list" => payment_links.list(vec![]);
    "paymentLinks.get" => payment_links.get("l");
    "paymentLinks.update" => payment_links.update("l", 1, &a.body, None);
    "paymentLinks.pause" => payment_links.pause("l", &a.link_action, None);
    "paymentLinks.resume" => payment_links.resume("l", &a.link_action, None);
    "paymentLinks.deactivate" => payment_links.deactivate("l", &a.link_action, None);
    "paymentLinks.action" => payment_links.action("l", "pause", &a.link_action, None);
    "paymentLinks.send" => payment_links.send("l", &a.link_send, "k");
    "paymentLinks.qr" => payment_links.qr("l", None, None);
    "paymentLinks.listAttempts" => payment_links.list_attempts("l", vec![]);
    "paymentLinks.listPayments" => payment_links.list_payments("l", vec![]);
    "paymentLinks.createImageUpload" => payment_links.create_image_upload("a.png", "image/png", 1);
    "paymentLinks.confirmImageUpload" => payment_links.confirm_image_upload("a");
    "paymentLinks.uploadImage" => payment_links.upload_image("a.png", "image/png", &[1]);
    "reference.listProvinces" => reference.list_provinces();
    "reference.listDistricts" => reference.list_districts("06");
    "reference.listNeighborhoods" => reference.list_neighborhoods("1");
    "physicalPos.cancelPayment" => physical_pos.cancel_payment("x", None);
    "physicalPos.createPayment" => physical_pos.create_payment(&a.body, "k");
    "physicalPos.createReversal" => physical_pos.create_reversal("x", &a.body, "k");
    "physicalPos.getPayment" => physical_pos.get_payment("x");
    "physicalPos.getSalesSummary" => physical_pos.get_sales_summary(&a.body);
    "physicalPos.getSettlementRun" => physical_pos.get_settlement_run("x");
    "physicalPos.getTaxSummary" => physical_pos.get_tax_summary(&a.body);
    "physicalPos.getTransaction" => physical_pos.get_transaction("x");
    "physicalPos.listAmbiguousPayments" => physical_pos.list_ambiguous_payments(&a.body);
    "physicalPos.listPayments" => physical_pos.list_payments(&a.body);
    "physicalPos.listSettlementRuns" => physical_pos.list_settlement_runs(&a.body);
    "physicalPos.listSettlements" => physical_pos.list_settlements(&a.body);
    "physicalPos.listTransactions" => physical_pos.list_transactions(&a.body);
    "physicalPos.startSettlement" => physical_pos.start_settlement("x", None);
    "physicalPosManagement.addTerminalAcquirer" => physical_pos_management.add_terminal_acquirer("x", &a.body, None);
    "physicalPosManagement.addTerminalProvider" => physical_pos_management.add_terminal_provider("x", &a.body, None);
    "physicalPosManagement.archiveRegister" => physical_pos_management.archive_register("x", &a.body);
    "physicalPosManagement.archiveStore" => physical_pos_management.archive_store("x", &a.body);
    "physicalPosManagement.assignDevice" => physical_pos_management.assign_device("x", &a.body);
    "physicalPosManagement.changeDeviceLifecycle" => physical_pos_management.change_device_lifecycle("x", &a.body);
    "physicalPosManagement.changeTerminalDevice" => physical_pos_management.change_terminal_device("x", &a.body);
    "physicalPosManagement.clearAccountSettlementPolicy" => physical_pos_management.clear_account_settlement_policy(None);
    "physicalPosManagement.clearDeviceSettlementPolicy" => physical_pos_management.clear_device_settlement_policy("x", None);
    "physicalPosManagement.clearStoreSettlementPolicy" => physical_pos_management.clear_store_settlement_policy("x", None);
    "physicalPosManagement.closeDeviceAssignment" => physical_pos_management.close_device_assignment("x", &a.body, None);
    "physicalPosManagement.closeTerminal" => physical_pos_management.close_terminal("x", &a.body);
    "physicalPosManagement.closeTerminalDeviceLink" => physical_pos_management.close_terminal_device_link("x", &a.body, None);
    "physicalPosManagement.createDevice" => physical_pos_management.create_device(&a.body, None);
    "physicalPosManagement.createRegister" => physical_pos_management.create_register("x", &a.body, None);
    "physicalPosManagement.createStore" => physical_pos_management.create_store(&a.body, None);
    "physicalPosManagement.createTerminal" => physical_pos_management.create_terminal(&a.body, None);
    "physicalPosManagement.getAccountSettlementPolicy" => physical_pos_management.get_account_settlement_policy();
    "physicalPosManagement.getDevice" => physical_pos_management.get_device("x");
    "physicalPosManagement.getDeviceSettlementPolicy" => physical_pos_management.get_device_settlement_policy("x");
    "physicalPosManagement.getEffectiveSettlementPolicy" => physical_pos_management.get_effective_settlement_policy("x");
    "physicalPosManagement.getOverview" => physical_pos_management.get_overview();
    "physicalPosManagement.getStore" => physical_pos_management.get_store("x");
    "physicalPosManagement.getStoreSettlementPolicy" => physical_pos_management.get_store_settlement_policy("x");
    "physicalPosManagement.getTerminal" => physical_pos_management.get_terminal("x");
    "physicalPosManagement.listAcquirers" => physical_pos_management.list_acquirers();
    "physicalPosManagement.listDeviceAssignments" => physical_pos_management.list_device_assignments("x", &a.body);
    "physicalPosManagement.listDeviceAudit" => physical_pos_management.list_device_audit("x", &a.body);
    "physicalPosManagement.listDeviceKeys" => physical_pos_management.list_device_keys("x", &a.body);
    "physicalPosManagement.listDeviceModels" => physical_pos_management.list_device_models(&a.body);
    "physicalPosManagement.listDevices" => physical_pos_management.list_devices(&a.body);
    "physicalPosManagement.listRegisterAudit" => physical_pos_management.list_register_audit("x", &a.body);
    "physicalPosManagement.listRegisters" => physical_pos_management.list_registers("x", &a.body);
    "physicalPosManagement.listStoreAudit" => physical_pos_management.list_store_audit("x", &a.body);
    "physicalPosManagement.listStores" => physical_pos_management.list_stores(&a.body);
    "physicalPosManagement.listTerminalAcquirers" => physical_pos_management.list_terminal_acquirers("x", &a.body);
    "physicalPosManagement.listTerminalAudit" => physical_pos_management.list_terminal_audit("x", &a.body);
    "physicalPosManagement.listTerminalLinks" => physical_pos_management.list_terminal_links("x", &a.body);
    "physicalPosManagement.listTerminalProviders" => physical_pos_management.list_terminal_providers("x", &a.body);
    "physicalPosManagement.listTerminals" => physical_pos_management.list_terminals(&a.body);
    "physicalPosManagement.listVendors" => physical_pos_management.list_vendors();
    "physicalPosManagement.restoreRegister" => physical_pos_management.restore_register("x", &a.body);
    "physicalPosManagement.restoreStore" => physical_pos_management.restore_store("x", &a.body);
    "physicalPosManagement.retireDevice" => physical_pos_management.retire_device("x", &a.body, None);
    "physicalPosManagement.revertDeviceEnrollment" => physical_pos_management.revert_device_enrollment("x", "x", &a.body);
    "physicalPosManagement.revokeActivationCode" => physical_pos_management.revoke_activation_code("x", &a.body);
    "physicalPosManagement.revokeDeviceKey" => physical_pos_management.revoke_device_key("x", "x", &a.body);
    "physicalPosManagement.setAccountSettlementPolicy" => physical_pos_management.set_account_settlement_policy(&a.body);
    "physicalPosManagement.setDeviceSettlementPolicy" => physical_pos_management.set_device_settlement_policy("x", &a.body);
    "physicalPosManagement.setStoreSettlementPolicy" => physical_pos_management.set_store_settlement_policy("x", &a.body);
    "physicalPosManagement.updateDevice" => physical_pos_management.update_device("x", &a.body);
    "physicalPosManagement.updateRegister" => physical_pos_management.update_register("x", &a.body);
    "physicalPosManagement.updateStore" => physical_pos_management.update_store("x", &a.body);
    "physicalPosManagement.updateTerminal" => physical_pos_management.update_terminal("x", &a.body);
    "physicalPosManagement.updateTerminalAcquirer" => physical_pos_management.update_terminal_acquirer("x", &a.body);
    "physicalPosManagement.updateTerminalProvider" => physical_pos_management.update_terminal_provider("x", &a.body);
    "wallet.ping" => wallet.ping();
    "wallet.programs.listMine" => wallet/programs.list_mine();
    "wallet.programs.setCollectionAnchor" => wallet/programs.set_collection_anchor("x", &a.body);
    "wallet.endUsers.create" => wallet/end_users.create(&a.body, "k");
    "wallet.endUsers.list" => wallet/end_users.list(&a.body);
    "wallet.endUsers.get" => wallet/end_users.get("x");
    "wallet.endUsers.getByExternalId" => wallet/end_users.get_by_external_id("x", None);
    "wallet.endUsers.freeze" => wallet/end_users.freeze("x", None, "k");
    "wallet.endUsers.getBalances" => wallet/end_users.get_balances("x");
    "wallet.endUsers.getKycLevel" => wallet/end_users.get_kyc_level("x");
    "wallet.endUsers.updateKycLevel" => wallet/end_users.update_kyc_level("x", &a.body, "k");
    "wallet.endUsers.getLimits" => wallet/end_users.get_limits("x", &a.body);
    "wallet.endUsers.createWallet" => wallet/end_users.create_wallet("x", &a.body, "k");
    "wallet.wallets.createAccount" => wallet/wallets.create_account("x", "TRY", "k");
    "wallet.wallets.lookup" => wallet/wallets.lookup(&a.body);
    "wallet.deposits.listInstructions" => wallet/deposits.list_instructions("x");
    "wallet.deposits.createInstruction" => wallet/deposits.create_instruction("x", &a.body);
    "wallet.deposits.simulateInboundCredit" => wallet/deposits.simulate_inbound_credit(&a.body);
    "wallet.corporate.create" => wallet/corporate.create(&a.body, "k");
    "wallet.corporate.list" => wallet/corporate.list(&a.body);
    "wallet.corporate.importEmployees" => wallet/corporate.import_employees("x", &a.body, "k");
    "wallet.corporate.offboardEmployees" => wallet/corporate.offboard_employees("x", &a.body, None, "k");
    "wallet.fees.quote" => wallet/fees.quote(&a.body);
    "wallet.transfers.quoteW2W" => wallet/transfers.quote_w2w(&a.body);
    "wallet.transfers.initiateW2W" => wallet/transfers.initiate_w2w(&a.body, "k");
    "wallet.transfers.completeW2W" => wallet/transfers.complete_w2w("x", &a.body, "k");
    "wallet.transfers.quoteW2Iban" => wallet/transfers.quote_w2_iban(&a.body);
    "wallet.transfers.initiateW2Iban" => wallet/transfers.initiate_w2_iban(&a.body, "k");
    "wallet.transfers.completeW2Iban" => wallet/transfers.complete_w2_iban("x", &a.body, "k");
    "wallet.withdrawals.quote" => wallet/withdrawals.quote(&a.body);
    "wallet.withdrawals.initiate" => wallet/withdrawals.initiate(&a.body, "k");
    "wallet.withdrawals.complete" => wallet/withdrawals.complete("x", &a.body, "k");
    "wallet.withdrawals.get" => wallet/withdrawals.get("x");
    "wallet.operations.list" => wallet/operations.list(&a.body);
    "wallet.operations.get" => wallet/operations.get("x");
    "wallet.operations.getReceipt" => wallet/operations.get_receipt("x");
    "wallet.topups.startCard" => wallet/topups.start_card(&a.body, "k");
    "wallet.topups.get" => wallet/topups.get("x");
    "wallet.topups.refund" => wallet/topups.refund("x", &a.body);
    "wallet.topups.agent" => wallet/topups.agent(&a.body, "k");
    "wallet.payments.request" => wallet/payments.request(&a.body, "k");
    "wallet.payments.approve" => wallet/payments.approve("x", &a.body, "k");
    "wallet.payments.collect" => wallet/payments.collect(&a.body, "k");
    "wallet.payments.get" => wallet/payments.get("x");
    "wallet.payments.getCommission" => wallet/payments.get_commission("x");
    "wallet.qr.create" => wallet/qr.create(&a.body, "k");
    "wallet.qr.list" => wallet/qr.list(&a.body);
    "wallet.qr.get" => wallet/qr.get("x");
    "wallet.qr.image" => wallet/qr.image("x", None, None);
    "wallet.qr.revoke" => wallet/qr.revoke("x");
    "wallet.qr.parse" => wallet/qr.parse("x");
    "wallet.bulkPayouts.create" => wallet/bulk_payouts.create(&a.body, "k");
    "wallet.bulkPayouts.list" => wallet/bulk_payouts.list(&a.body);
    "wallet.bulkPayouts.get" => wallet/bulk_payouts.get("x");
    "wallet.bulkPayouts.listItems" => wallet/bulk_payouts.list_items("x", &a.body);
    "wallet.bulkPayouts.submit" => wallet/bulk_payouts.submit("x");
    "wallet.bulkPayouts.approve" => wallet/bulk_payouts.approve("x", true, None);
    "wallet.bulkPayouts.reject" => wallet/bulk_payouts.reject("x", None);
    "wallet.bulkPayouts.process" => wallet/bulk_payouts.process("x");
    "wallet.bulkPayouts.retryFailed" => wallet/bulk_payouts.retry_failed("x");
    "bankConnect.listBanks" => bank_connect.list_banks(&a.body);
    "bankConnect.listConnections" => bank_connect.list_connections(&a.body);
    "bankConnect.getConnection" => bank_connect.get_connection("x");
    "bankConnect.listAccounts" => bank_connect.list_accounts(&a.body);
    "bankConnect.getAccount" => bank_connect.get_account("x");
    "bankConnect.requestSync" => bank_connect.request_sync("x");
    "bankConnect.listBalances" => bank_connect.list_balances(&a.body);
    "bankConnect.listTransactions" => bank_connect.list_transactions(&a.body);
    "bankConnect.getTransaction" => bank_connect.get_transaction("x");
    "bankConnect.transfers.listPurposes" => bank_connect/transfers.list_purposes("x");
    "bankConnect.transfers.create" => bank_connect/transfers.create(&a.body, None);
    "bankConnect.transfers.list" => bank_connect/transfers.list(&a.body);
    "bankConnect.transfers.get" => bank_connect/transfers.get("x");
    "bankConnect.fx.listRates" => bank_connect/fx.list_rates("x");
    "bankConnect.fx.createQuote" => bank_connect/fx.create_quote(&a.body);
    "bankConnect.fx.createOrder" => bank_connect/fx.create_order(&a.body, None);
    "bankConnect.fx.listOrders" => bank_connect/fx.list_orders(&a.body);
    "bankConnect.fx.getOrder" => bank_connect/fx.get_order("x");
    "sanction.screenName" => sanction.screen_name(&a.body);
    "sanction.screenWallet" => sanction.screen_wallet(&a.body);
    "sanction.screenAccount" => sanction.screen_account(&a.body);
    "sanction.screenTransaction" => sanction.screen_transaction(&a.body);
    "sanction.screenPublicProcurementBan" => sanction.screen_public_procurement_ban(&a.body);
    "sanction.screenAdverseMedia" => sanction.screen_adverse_media(&a.body);
    "sanction.evaluateBehavioralAlert" => sanction.evaluate_behavioral_alert(&a.body);
    "sanction.listScreeningHistory" => sanction.list_screening_history(&a.body);
    "sanction.registerMonitoring" => sanction.register_monitoring(&a.body);
    "sanction.listMonitoringAlerts" => sanction.list_monitoring_alerts(&a.body);
    "sanction.listMonitoredEntities" => sanction.list_monitored_entities(&a.body);
    "sanction.listMonitoringMatches" => sanction.list_monitoring_matches(&a.body);
    "sanction.getDatasetFreshness" => sanction.get_dataset_freshness();
    "sanction.searchEntities" => sanction.search_entities(&a.body);
    "sanction.getEntityProfile" => sanction.get_entity_profile("x", None);
    "sanction.getEntityGraph" => sanction.get_entity_graph("x", None);
    "sanction.getIndirectExposure" => sanction.get_indirect_exposure("x", None, &[]);
    "sanction.getOwnershipAggregation" => sanction.get_ownership_aggregation("x", None, &[]);
    "sanction.listBlacklistEntries" => sanction.list_blacklist_entries(&a.body);
    "insurance.listProducts" => insurance.list_products(&a.body);
    "insurance.searchProducts" => insurance.search_products(&a.body);
    "insurance.listReferenceValues" => insurance.list_reference_values("CITY", &a.body);
    "insurance.createQuote" => insurance.create_quote(&a.body, None);
    "insurance.getQuote" => insurance.get_quote("x");
    "insurance.createOrder" => insurance.create_order(&a.body, "k");
    "insurance.listOrders" => insurance.list_orders(&a.body);
    "insurance.getOrder" => insurance.get_order("x");
    "insurance.bindPayment" => insurance.bind_payment("x", "p", None);
    "insurance.confirmPayment" => insurance.confirm_payment("x", &a.body, "k");
    "insurance.issuePolicy" => insurance.issue_policy("x", "k");
    "insurance.cancelOrder" => insurance.cancel_order("x", "k");
    "insurance.refundPremium" => insurance.refund_premium("x", &a.body, "k");
    "insurance.getPolicyDocument" => insurance.get_policy_document("x");
    "insurance.getPolicyReceipt" => insurance.get_policy_receipt("x");
    "insurance.getPolicyCertificate" => insurance.get_policy_certificate("x");
    "insurance.getReconciliationSummary" => insurance.get_reconciliation_summary(&a.body);
    "insurance.listReconciliationOrders" => insurance.list_reconciliation_orders(&a.body);
    "insurance.exportReconciliation" => insurance.export_reconciliation(&a.body);
    "insurance.createLead" => insurance.create_lead(&a.body, "k");
    "insurance.listLeads" => insurance.list_leads(&a.body);
    "insurance.getLead" => insurance.get_lead("x");
    "subscriptions.create" => subscriptions.create(&a.body, None);
    "subscriptions.get" => subscriptions.get("x");
    "subscriptions.list" => subscriptions.list(&a.body);
    "subscriptions.cancel" => subscriptions.cancel("x", true, None);
    "subscriptions.pause" => subscriptions.pause("x", None);
    "subscriptions.resume" => subscriptions.resume("x");
    "subscriptions.changePlan" => subscriptions.change_plan("x", &a.body, None);
    "subscriptions.auditLogs" => subscriptions.audit_logs("x", &a.body);
    "subscriptions.plans.create" => subscriptions/plans.create(&a.body, None);
    "subscriptions.plans.get" => subscriptions/plans.get("x");
    "subscriptions.plans.list" => subscriptions/plans.list(&a.body);
    "subscriptions.plans.update" => subscriptions/plans.update("x", &a.body);
    "subscriptions.plans.archive" => subscriptions/plans.archive("x");
    "subscriptions.plans.translations" => subscriptions/plans.translations("x");
    "subscriptions.plans.upsertTranslation" => subscriptions/plans.upsert_translation("x", "en", &a.body);
    "subscriptions.customers.upsert" => subscriptions/customers.upsert(&a.body, None);
    "subscriptions.customers.get" => subscriptions/customers.get("x");
    "subscriptions.customers.getByExternalId" => subscriptions/customers.get_by_external_id("x");
    "subscriptions.customers.list" => subscriptions/customers.list(&a.body);
    "subscriptions.customers.creditBalance" => subscriptions/customers.credit_balance("x");
    "subscriptions.customers.adjustCredit" => subscriptions/customers.adjust_credit("x", &a.body);
    "subscriptions.customers.archive" => subscriptions/customers.archive("x");
    "subscriptions.cards.add" => subscriptions/cards.add(&a.body, None);
    "subscriptions.cards.get" => subscriptions/cards.get("x");
    "subscriptions.cards.list" => subscriptions/cards.list("x", &a.body);
    "subscriptions.cards.setDefault" => subscriptions/cards.set_default("x");
    "subscriptions.cards.deactivate" => subscriptions/cards.deactivate("x", None);
    "subscriptions.cards.verify" => subscriptions/cards.verify("x", &a.body);
    "subscriptions.cards.update" => subscriptions/cards.update("x", &a.body);
    "subscriptions.invoices.list" => subscriptions/invoices.list(&a.body);
    "subscriptions.invoices.get" => subscriptions/invoices.get("x");
    "subscriptions.invoices.void" => subscriptions/invoices.void("x", None);
    "subscriptions.invoices.retryCharge" => subscriptions/invoices.retry_charge("x", None);
    "subscriptions.portal.createSdkSession" => subscriptions/portal.create_sdk_session("c", "https://shop.example", None, None);
    "subscriptions.portal.createPortalLink" => subscriptions/portal.create_portal_link("c", &a.body);
    "subscriptions.portal.listLinks" => subscriptions/portal.list_links(&a.body);
    "subscriptions.portal.getLink" => subscriptions/portal.get_link("x");
    "subscriptions.portal.updateLink" => subscriptions/portal.update_link("x", &a.body);
    "subscriptions.portal.revokeLink" => subscriptions/portal.revoke_link("x", None);
    "subscriptions.portal.linkActivity" => subscriptions/portal.link_activity("x");
    "subscriptions.portal.sendInvite" => subscriptions/portal.send_invite(&a.body);
    "subscriptions.portal.getSettings" => subscriptions/portal.get_settings();
    "subscriptions.portal.updateSettings" => subscriptions/portal.update_settings(&a.body);
    "subscriptions.products.create" => subscriptions/products.create(&a.body);
    "subscriptions.products.list" => subscriptions/products.list(&a.body);
    "subscriptions.products.get" => subscriptions/products.get("x");
    "subscriptions.products.update" => subscriptions/products.update("x", &a.body);
    "subscriptions.products.archive" => subscriptions/products.archive("x");
    "subscriptions.products.translations" => subscriptions/products.translations("x");
    "subscriptions.products.upsertTranslation" => subscriptions/products.upsert_translation("x", "en", &a.body);
    "subscriptions.features.create" => subscriptions/features.create(&a.body);
    "subscriptions.features.list" => subscriptions/features.list(&a.body);
    "subscriptions.features.get" => subscriptions/features.get("x");
    "subscriptions.features.update" => subscriptions/features.update("x", &a.body);
    "subscriptions.features.delete" => subscriptions/features.delete("x");
    "subscriptions.features.translations" => subscriptions/features.translations("x");
    "subscriptions.features.upsertTranslation" => subscriptions/features.upsert_translation("x", "en", &a.body);
    "subscriptions.addons.create" => subscriptions/addons.create(&a.body);
    "subscriptions.addons.list" => subscriptions/addons.list(&a.body);
    "subscriptions.addons.update" => subscriptions/addons.update("x", &a.body);
    "subscriptions.addons.deactivate" => subscriptions/addons.deactivate("x");
    "subscriptions.usage.record" => subscriptions/usage.record(&a.body);
    "subscriptions.usage.checkEntitlement" => subscriptions/usage.check_entitlement(&a.body);
    "kyc.createSession" => kyc.create_session(&a.body, None);
    "kyc.getSession" => kyc.get_session("x");
    "kyc.listSessions" => kyc.list_sessions(&a.body);
    "kyc.setJurisdiction" => kyc.set_jurisdiction("x", "TR");
    "kyc.setPrimaryDocument" => kyc.set_primary_document("x", "PASSPORT");
    "kyc.uploadDocument" => kyc.upload_document("x", &a.body);
    "kyc.timeline" => kyc.timeline("x");
    "kyc.submit" => kyc.submit("x");
    "kyc.manualReview" => kyc.manual_review("x", None, None);
    "kyc.evidenceRequest" => kyc.evidence_request(&a.body, None);
    "kyc.merchantPolicy" => kyc.merchant_policy();
    "kyc.updateMerchantPolicy" => kyc.update_merchant_policy(&a.body);
    "kyc.rotateWebhookSecret" => kyc.rotate_webhook_secret();
    "kyc.webhookOutbox" => kyc.webhook_outbox(&a.body);
    "kyc.retryWebhookEvent" => kyc.retry_webhook_event("x");
    "kyc.erasureRequests" => kyc.erasure_requests(&a.body);
    "identity.analyzeSync" => identity.analyze_sync(&a.body);
    "identity.analyze" => identity.analyze(&a.body);
    "identity.accountLinks" => identity.account_links("x");
    "identity.getSession" => identity.get_session("x");
    "identity.relatedSessions" => identity.related_sessions("x", None);
    "identity.timeline" => identity.timeline("x");
    "identity.topSources" => identity.top_sources(&a.body);
    "identity.listSessions" => identity.list_sessions(&a.body);
    "marketplace.createCheckoutIntent" => marketplace.create_checkout_intent(&a.body, &a.body, None, "k");
    "marketplace.chargeMarketplaceCard" => marketplace.charge_marketplace_card(&a.body, false, "k");
    "marketplace.chargeMarketplaceCard2d" => marketplace.charge_marketplace_card2d(&a.body, &a.body, None, "k");
    "marketplace.chargeMarketplaceCard3d" => marketplace.charge_marketplace_card3d(&a.body, &a.body, None, "k");
    "marketplace.createDealer" => marketplace.create_dealer(&a.body);
    "marketplace.bulkCreateDealers" => marketplace.bulk_create_dealers(std::slice::from_ref(&a.body));
    "marketplace.listDealers" => marketplace.list_dealers(&a.body);
    "marketplace.getDealer" => marketplace.get_dealer("x");
    "marketplace.updateDealerStatus" => marketplace.update_dealer_status("x", "ACTIVE");
    "marketplace.onboardingToken" => marketplace.onboarding_token("x");
    "marketplace.dealerApplication" => marketplace.dealer_application("x");
    "marketplace.submitDealerApplication" => marketplace.submit_dealer_application("x", &a.body);
    "marketplace.setDealerTerms" => marketplace.set_dealer_terms("x", &a.body);
    "marketplace.signDealerContract" => marketplace.sign_dealer_contract("x", &a.body);
    "marketplace.activateDealer" => marketplace.activate_dealer("x");
    "marketplace.recordDealerKycResult" => marketplace.record_dealer_kyc_result("x", &a.body);
    "marketplace.payoutLedger" => marketplace.payout_ledger(&a.body);
    "marketplace.transactions" => marketplace.transactions(&a.body);
    "marketplace.commissionRules" => marketplace.commission_rules(&a.body);
    "marketplace.upsertCommissionRule" => marketplace.upsert_commission_rule(&a.body);
    "marketplace.previewCommissionRule" => marketplace.preview_commission_rule(&a.body);
    "marketplace.resolveCommission" => marketplace.resolve_commission("x", None, None);
    "marketplace.createPayout" => marketplace.create_payout(&a.body);
    "marketplace.markPayoutPaid" => marketplace.mark_payout_paid("x", &a.body);
    "marketplace.bulkMarkPayoutsPaid" => marketplace.bulk_mark_payouts_paid(&["x"], &a.body);
    "marketplace.auditLog" => marketplace.audit_log(&a.body);
    "marketplace.verifyAuditLog" => marketplace.verify_audit_log();
}

/// The presigned PUT is a separate step of `uploadImage` in this SDK.
#[allow(dead_code)]
async fn compile_image_put(client: &Client, a: &Args) {
    let _ = client
        .payment_links()
        .put_image_upload(&a.image, &[1])
        .await;
}

/// Same rule as the catalog tooling: `_` between a lowercase and an
/// uppercase letter, and before an uppercase + lowercase pair that follows
/// an uppercase letter or a digit (`latestForPayment` → `latest_for_payment`,
/// `quoteW2W` → `quote_w2w`, `quoteW2Iban` → `quote_w2_iban`).
fn snake(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            let previous = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(char::is_ascii_lowercase);
            if previous.is_ascii_lowercase()
                || ((previous.is_ascii_uppercase() || previous.is_ascii_digit()) && next_lower)
            {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn read(path: &Path) -> Option<Value> {
    let raw = std::fs::read(path).ok()?;
    Some(serde_json::from_slice(&raw).expect("catalog JSON"))
}

#[test]
fn snake_case_matches_the_catalog_tooling() {
    assert_eq!(snake("latestForPayment"), "latest_for_payment");
    assert_eq!(snake("binInfo"), "bin_info");
    assert_eq!(
        snake("downloadPromissoryNoteDocument"),
        "download_promissory_note_document"
    );
    assert_eq!(snake("force3D"), "force3d");
    assert_eq!(snake("quoteW2W"), "quote_w2w");
    assert_eq!(snake("completeW2Iban"), "complete_w2_iban");
    assert_eq!(snake("parseHTTPResponse"), "parse_http_response");
}

#[test]
fn required_catalog_operations_exist() {
    let contracts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts");
    let names = read(&contracts.join("sdk-names.json"))
        .and_then(|all| all.get("rust").cloned())
        .unwrap_or(Value::Null);
    let mut checked = 0;
    let mut every_product = true;
    for (product, prefix) in PRODUCTS {
        let Some(catalog) = read(&contracts.join(format!("products/{product}.json"))) else {
            eprintln!("contracts/products/{product}.json not found; parity check skipped");
            every_product = false;
            continue;
        };
        let operations = catalog["operations"].as_object().expect("operations");
        for (op, meta) in operations {
            if meta["status"] != "required" {
                continue;
            }
            let expected = match names.get(op) {
                Some(Value::Null) => continue,
                Some(Value::String(name)) => name.clone(),
                _ => snake(op.rsplit('.').next().unwrap()),
            };
            assert!(
                OPERATIONS.contains(&(op.as_str(), expected.as_str())),
                "required operation {op} is not implemented as `{expected}`"
            );
            checked += 1;
        }
        for (op, _) in OPERATIONS.iter().filter(|(op, _)| op.starts_with(prefix)) {
            assert!(
                operations.contains_key(*op),
                "{op} is listed here but not in the {product} catalog"
            );
        }
    }
    if every_product {
        assert_eq!(
            checked,
            OPERATIONS.len(),
            "every listed operation is required"
        );
    }
}

fn group(product: &str, name: &str) -> Option<Value> {
    let contracts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts");
    let catalog = read(&contracts.join(format!("products/{product}.json")))?;
    catalog["groups"].get(name).cloned()
}

fn names(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// Each route table matches the catalog's `routes` group row for row: the
/// same route names in order, and per route the method, the path, the key
/// rule, the screening and binary flags and the body / query field lists.
#[test]
fn route_tables_match_the_catalog() {
    let tables: &[(&str, &str, &[Route])] = &[
        ("wallet", "WALLET_ROUTES", WALLET_ROUTES),
        ("sanction", "SANCTION_ROUTES", SANCTION_ROUTES),
        ("bank-connect", "BANK_TRANSFER_ROUTES", BANK_TRANSFER_ROUTES),
    ];
    for (product, name, table) in tables {
        let Some(group) = group(product, name) else {
            eprintln!("{product} {name} not found; route parity skipped");
            continue;
        };
        assert_eq!(group["kind"], "routes", "{name}");
        let rows = group["values"].as_object().expect("routes");
        let expected: Vec<&str> = rows.keys().map(String::as_str).collect();
        let actual: Vec<&str> = table.iter().map(|route| route.name).collect();
        assert_eq!(actual, expected, "{name} route names");
        for route in *table {
            let row = &rows[route.name];
            let label = format!("{name}.{}", route.name);
            assert_eq!(row["method"], route.method, "{label} method");
            assert_eq!(row["path"], route.path, "{label} path");
            assert_eq!(
                row["idempotencyKey"].as_bool().unwrap_or(false),
                route.idempotency_key,
                "{label} idempotencyKey"
            );
            assert_eq!(
                row["screening"].as_bool().unwrap_or(false),
                route.screening,
                "{label} screening"
            );
            assert_eq!(
                row["binary"].as_bool().unwrap_or(false),
                route.binary,
                "{label} binary"
            );
            assert_eq!(names(&row["required"]), route.required, "{label} required");
            assert_eq!(names(&row["optional"]), route.optional, "{label} optional");
            assert_eq!(names(&row["anyOf"]), route.any_of, "{label} anyOf");
            assert_eq!(names(&row["query"]), route.query, "{label} query");
        }
    }
}

/// The subscription, KYC, identity and marketplace tables match the
/// catalog's `routes` groups row for row: names in order, and per route the
/// method, the path, `stepUp`, the `Idempotency-Key` rule, the `free`,
/// `optionalBody` and `anyQuery` flags and the body / query field lists.
#[test]
fn product_route_tables_match_the_catalog() {
    let tables: &[(&str, &str, &[Route])] = &[
        ("subscriptions", "SUBSCRIPTION_ROUTES", SUBSCRIPTION_ROUTES),
        ("kyc", "KYC_ROUTES", KYC_ROUTES),
        ("identity", "IDENTITY_ROUTES", IDENTITY_ROUTES),
        ("marketplace", "MARKETPLACE_ROUTES", MARKETPLACE_ROUTES),
    ];
    for (product, name, table) in tables {
        let Some(group) = group(product, name) else {
            eprintln!("{product} {name} not found; route parity skipped");
            continue;
        };
        assert_eq!(group["kind"], "routes", "{name}");
        let rows = group["values"].as_object().expect("routes");
        let expected: Vec<&str> = rows.keys().map(String::as_str).collect();
        let actual: Vec<&str> = table.iter().map(|route| route.name).collect();
        assert_eq!(actual, expected, "{name} route names");
        let flag = |row: &Value, key: &str| row[key].as_bool().unwrap_or(false);
        for route in *table {
            let row = &rows[route.name];
            let label = format!("{name}.{}", route.name);
            assert_eq!(row["method"], route.method, "{label} method");
            assert_eq!(row["path"], route.path, "{label} path");
            assert_eq!(flag(row, "stepUp"), route.step_up, "{label} stepUp");
            assert_eq!(
                row["idempotencyKey"].as_str().unwrap_or("none"),
                route.idempotency_key_rule(),
                "{label} idempotencyKey"
            );
            assert_eq!(flag(row, "free"), route.free_body, "{label} free");
            assert_eq!(
                flag(row, "optionalBody"),
                route.optional_body,
                "{label} optionalBody"
            );
            assert_eq!(flag(row, "anyQuery"), route.any_query, "{label} anyQuery");
            assert_eq!(names(&row["required"]), route.required, "{label} required");
            assert_eq!(names(&row["optional"]), route.optional, "{label} optional");
            assert_eq!(names(&row["query"]), route.query, "{label} query");
        }
    }
}

/// The value modules match the catalog's `enum` groups (the values, in order).
#[test]
fn value_sets_match_the_catalog_enums() {
    use lunixi::*;
    let enums: &[(&str, &str, &[&str])] = &[
        (
            "subscriptions",
            "SubscriptionStatus",
            subscription_status::ALL,
        ),
        (
            "subscriptions",
            "SubscriptionInvoiceStatus",
            subscription_invoice_status::ALL,
        ),
        (
            "subscriptions",
            "SubscriptionCollectionMethod",
            subscription_collection_method::ALL,
        ),
        (
            "subscriptions",
            "SubscriptionBillingFrequency",
            subscription_billing_frequency::ALL,
        ),
        (
            "subscriptions",
            "SubscriptionCardVerificationMode",
            subscription_card_verification_mode::ALL,
        ),
        (
            "subscriptions",
            "SubscriptionPlanChangeBehavior",
            subscription_plan_change_behavior::ALL,
        ),
        (
            "subscriptions",
            "SubscriptionPortalSdkMode",
            subscription_portal_sdk_mode::ALL,
        ),
        (
            "subscriptions",
            "SubscriptionEvents",
            subscription_events::ALL,
        ),
        (
            "subscriptions",
            "PortalLink",
            &[SUBSCRIPTION_PORTAL_HOSTED_BASE],
        ),
        ("kyc", "KycSessionType", kyc_session_type::ALL),
        ("kyc", "KycSessionStatus", kyc_session_status::ALL),
        ("kyc", "KycEvidencePurpose", kyc_evidence_purpose::ALL),
        ("kyc", "KycAssuranceLevel", kyc_assurance_level::ALL),
        ("kyc", "KycAssuranceDomain", kyc_assurance_domain::ALL),
        ("kyc", "KycEvents", kyc_events::ALL),
        (
            "marketplace",
            "MarketplaceDealerStatus",
            marketplace_dealer_status::ALL,
        ),
        (
            "marketplace",
            "MarketplaceDealerType",
            marketplace_dealer_type::ALL,
        ),
        (
            "marketplace",
            "MarketplaceMultiSellerPolicy",
            marketplace_multi_seller_policy::ALL,
        ),
        (
            "marketplace",
            "DealerOnboardingLink",
            &[MARKETPLACE_DEALER_ONBOARDING_HOSTED_BASE],
        ),
        (
            "subscriptions",
            "SubscriptionCustomerErrorCode",
            subscription_customer_error_code::ALL,
        ),
        (
            "payments",
            "OrderFieldErrorCode",
            order_field_error_code::ALL,
        ),
        ("payments", "OrderFieldReason", order_field_reason::ALL),
        (
            "payment-links",
            "BuyerFieldRequirement",
            buyer_field_requirement::ALL,
        ),
    ];
    for (product, name, values) in enums {
        let Some(group) = group(product, name) else {
            continue;
        };
        assert_eq!(group["kind"], "enum", "{product} {name}");
        let expected: Vec<&str> = group["values"]
            .as_object()
            .expect("enum values")
            .values()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(expected, *values, "{product} {name}");
    }
}

/// Value sets, field lists, filters and event names match the catalog's
/// `list` groups.
#[test]
fn value_lists_match_the_catalog() {
    use lunixi::*;
    let mut lists: Vec<(&str, String, &[&str])> = vec![
        (
            "wallet",
            "WALLET_AMOUNT_FIELDS".into(),
            WALLET_AMOUNT_FIELDS,
        ),
        (
            "wallet",
            "WalletAccountType.ALL".into(),
            wallet_account_type::ALL,
        ),
        (
            "wallet",
            "WalletEndUserStatus.ALL".into(),
            wallet_end_user_status::ALL,
        ),
        ("wallet", "WalletKycLevel.ALL".into(), wallet_kyc_level::ALL),
        (
            "wallet",
            "WalletOtpChannel.ALL".into(),
            wallet_otp_channel::ALL,
        ),
        ("wallet", "WalletQrType.ALL".into(), wallet_qr_type::ALL),
        (
            "wallet",
            "WalletQrImageFormat.ALL".into(),
            wallet_qr_image_format::ALL,
        ),
        (
            "wallet",
            "WalletBulkTargetType.ALL".into(),
            wallet_bulk_target_type::ALL,
        ),
        (
            "wallet",
            "WalletCorporateAccountStatus.ALL".into(),
            wallet_corporate_account_status::ALL,
        ),
        (
            "bank-connect",
            "BANK_CONNECT_EVENT_TYPES".into(),
            BANK_CONNECT_EVENT_TYPES,
        ),
        (
            "bank-connect",
            "BANK_TRANSFER_EVENT_TYPES".into(),
            BANK_TRANSFER_EVENT_TYPES,
        ),
        (
            "bank-connect",
            "BankConnectionStatus.ALL".into(),
            bank_connection_status::ALL,
        ),
        (
            "bank-connect",
            "BankAccountStatus.ALL".into(),
            bank_account_status::ALL,
        ),
        (
            "bank-connect",
            "BankTransactionDirection.ALL".into(),
            bank_transaction_direction::ALL,
        ),
        (
            "bank-connect",
            "BankTransactionType.ALL".into(),
            bank_transaction_type::ALL,
        ),
        (
            "bank-connect",
            "BankAvailability.ALL".into(),
            bank_availability::ALL,
        ),
        (
            "bank-connect",
            "BankCapability.ALL".into(),
            bank_capability::ALL,
        ),
        (
            "bank-connect",
            "BankSyncStatus.ALL".into(),
            bank_sync_status::ALL,
        ),
        (
            "bank-connect",
            "BankTransferStatus.ALL".into(),
            bank_transfer_status::ALL,
        ),
        (
            "bank-connect",
            "BankTransferRail.ALL".into(),
            bank_transfer_rail::ALL,
        ),
        (
            "bank-connect",
            "BankTransferInitiatedVia.ALL".into(),
            bank_transfer_initiated_via::ALL,
        ),
        (
            "bank-connect",
            "BankFxOrderStatus.ALL".into(),
            bank_fx_order_status::ALL,
        ),
        (
            "bank-connect",
            "BankFxRequestedSide.ALL".into(),
            bank_fx_requested_side::ALL,
        ),
        (
            "bank-connect",
            "BankConnectEvents.ALL".into(),
            BANK_CONNECT_EVENT_TYPES,
        ),
        (
            "bank-connect",
            "BankTransferEvents.ALL".into(),
            BANK_TRANSFER_EVENT_TYPES,
        ),
        (
            "sanction",
            "RiskDomain.ALL".into(),
            sanction_risk_domain::ALL,
        ),
        (
            "sanction",
            "ScreenMode.ALL".into(),
            sanction_screen_mode::ALL,
        ),
        (
            "sanction",
            "WalletScreenMode.ALL".into(),
            sanction_wallet_screen_mode::ALL,
        ),
        (
            "sanction",
            "SubjectType.ALL".into(),
            sanction_subject_type::ALL,
        ),
        (
            "sanction",
            "CoverageStatus.ALL".into(),
            sanction_coverage_status::ALL,
        ),
        ("sanction", "HitType.ALL".into(), sanction_hit_type::ALL),
        (
            "insurance",
            "INSURANCE_QUOTE_FIELDS".into(),
            INSURANCE_QUOTE_FIELDS,
        ),
        (
            "insurance",
            "INSURANCE_LEAD_FIELDS".into(),
            INSURANCE_LEAD_FIELDS,
        ),
        (
            "insurance",
            "INSURANCE_PRODUCT_SEARCH_FIELDS".into(),
            INSURANCE_PRODUCT_SEARCH_FIELDS,
        ),
        (
            "insurance",
            "INSURANCE_ORDER_FIELDS".into(),
            INSURANCE_ORDER_FIELDS,
        ),
        (
            "insurance",
            "INSURANCE_CONSENT_FIELDS".into(),
            INSURANCE_CONSENT_FIELDS,
        ),
        (
            "insurance",
            "INSURANCE_EXTERNAL_PAYMENT_FIELDS".into(),
            INSURANCE_EXTERNAL_PAYMENT_FIELDS,
        ),
        (
            "insurance",
            "INSURANCE_EXTERNAL_REFUND_FIELDS".into(),
            INSURANCE_EXTERNAL_REFUND_FIELDS,
        ),
        (
            "insurance",
            "INSURANCE_EVENT_TYPES".into(),
            INSURANCE_EVENT_TYPES,
        ),
        (
            "insurance",
            "InsuranceEvents.ORDER_EVENTS".into(),
            insurance_events::ORDER_EVENTS,
        ),
        (
            "insurance",
            "InsuranceEvents.LEAD_EVENTS".into(),
            insurance_events::LEAD_EVENTS,
        ),
        (
            "insurance",
            "InsurancePremiumCollectedBy.ALL".into(),
            insurance_premium_collected_by::ALL,
        ),
        (
            "insurance",
            "InsuranceQuoteStatus.ALL".into(),
            insurance_quote_status::ALL,
        ),
        (
            "insurance",
            "InsuranceOrderStatus.ALL".into(),
            insurance_order_status::ALL,
        ),
        (
            "insurance",
            "InsuranceCollectionStatus.ALL".into(),
            insurance_collection_status::ALL,
        ),
        (
            "insurance",
            "InsurancePolicyStatus.ALL".into(),
            insurance_policy_status::ALL,
        ),
        (
            "insurance",
            "InsuranceRefundStatus.ALL".into(),
            insurance_refund_status::ALL,
        ),
        (
            "insurance",
            "InsuranceDocumentStatus.ALL".into(),
            insurance_document_status::ALL,
        ),
        (
            "insurance",
            "InsurancePaymentMode.ALL".into(),
            insurance_payment_mode::ALL,
        ),
        (
            "insurance",
            "InsuranceOrderSurface.ALL".into(),
            insurance_order_surface::ALL,
        ),
        (
            "insurance",
            "InsuranceLeadStatus.ALL".into(),
            insurance_lead_status::ALL,
        ),
        (
            "insurance",
            "InsuranceReferenceKind.ALL".into(),
            insurance_reference_kind::ALL,
        ),
    ];
    let routed: &[(&str, &str, &[&str])] = &[
        (
            "subscriptions",
            "CollectionMethod.ALL",
            subscription_collection_method::ALL,
        ),
        (
            "subscriptions",
            "SUBSCRIPTION_ACTIVE_STATES",
            SUBSCRIPTION_ACTIVE_STATES,
        ),
        (
            "subscriptions",
            "SUBSCRIPTION_TERMINAL_STATES",
            SUBSCRIPTION_TERMINAL_STATES,
        ),
        (
            "subscriptions",
            "SubscriptionStatus.ACTIVE_STATES",
            subscription_status::ACTIVE_STATES,
        ),
        (
            "subscriptions",
            "SubscriptionStatus.TERMINAL_STATES",
            subscription_status::TERMINAL_STATES,
        ),
        (
            "subscriptions",
            "SUBSCRIPTION_BILLING_RULE_FIELDS",
            SUBSCRIPTION_BILLING_RULE_FIELDS,
        ),
        (
            "subscriptions",
            "SUBSCRIPTION_EVENT_TYPES",
            SUBSCRIPTION_EVENT_TYPES,
        ),
        (
            "kyc",
            "AchievedAssurance.DOMAINS",
            kyc_assurance_domain::ALL,
        ),
        ("kyc", "EvidencePurpose.ALL", kyc_evidence_purpose::ALL),
        ("kyc", "KYC_EVENT_TYPES", KYC_EVENT_TYPES),
        (
            "kyc",
            "KYC_SESSION_TERMINAL_STATES",
            KYC_SESSION_TERMINAL_STATES,
        ),
        (
            "kyc",
            "SessionStatus.TERMINAL_STATES",
            kyc_session_status::TERMINAL_STATES,
        ),
        ("kyc", "SessionType.ALL", kyc_session_type::ALL),
        (
            "identity",
            "IDENTITY_ANALYZE_FIELDS",
            IDENTITY_ANALYZE_FIELDS,
        ),
        (
            "marketplace",
            "MARKETPLACE_CHARGE_REQUIRED_FIELDS",
            MARKETPLACE_CHARGE_REQUIRED_FIELDS,
        ),
        (
            "marketplace",
            "MARKETPLACE_SELLER_FIELDS",
            MARKETPLACE_SELLER_FIELDS,
        ),
        (
            "payments",
            "ORDER_DETAILS_REQUIRED_FIELDS",
            ORDER_DETAILS_REQUIRED_FIELDS,
        ),
        ("payments", "BUYER_REQUIRED_FIELDS", BUYER_REQUIRED_FIELDS),
        (
            "payments",
            "CHECKOUT_INTENT_BUYER_OPTIONAL_FIELDS",
            CHECKOUT_INTENT_BUYER_OPTIONAL_FIELDS,
        ),
        (
            "payments",
            "ADDRESS_REQUIRED_FIELDS",
            ADDRESS_REQUIRED_FIELDS,
        ),
        (
            "payments",
            "BASKET_ITEM_REQUIRED_FIELDS",
            BASKET_ITEM_REQUIRED_FIELDS,
        ),
        ("payments", "SPLIT_PAYMENT_FIELDS", SPLIT_PAYMENT_FIELDS),
    ];
    for (product, name, values) in routed {
        lists.push((product, (*name).into(), values));
    }
    for (route, filters) in BANK_CONNECT_FILTERS {
        lists.push((
            "bank-connect",
            format!("BANK_CONNECT_FILTERS.{route}"),
            filters,
        ));
    }
    for (route, filters) in INSURANCE_FILTERS {
        lists.push(("insurance", format!("INSURANCE_FILTERS.{route}"), filters));
    }
    for (field, values) in SANCTION_VALUE_SETS {
        lists.push(("sanction", format!("SANCTION_VALUE_SETS.{field}"), values));
    }
    let mut compared = 0;
    for (product, name, values) in &lists {
        let Some(group) = group(product, name) else {
            continue;
        };
        assert_eq!(names(&group["values"]), *values, "{product} {name}");
        compared += 1;
    }
    let catalog = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/products/wallet.json");
    if catalog.exists() {
        assert!(compared > 0, "no catalog list was compared");
    }
}

/// The key and code patterns match the catalog's `$pattern` groups.
#[test]
fn patterns_match_the_catalog() {
    use lunixi::*;
    let patterns = [
        ("wallet", "AMOUNT_PATTERN", WALLET_AMOUNT_PATTERN),
        (
            "bank-connect",
            "BANK_TRANSFER_IDEMPOTENCY_KEY_PATTERN",
            BANK_TRANSFER_IDEMPOTENCY_KEY_PATTERN,
        ),
        (
            "bank-connect",
            "BANK_TRANSFER_PAYMENT_PURPOSE_PATTERN",
            BANK_TRANSFER_PAYMENT_PURPOSE_PATTERN,
        ),
        (
            "insurance",
            "INSURANCE_IDEMPOTENCY_KEY_PATTERN",
            INSURANCE_IDEMPOTENCY_KEY_PATTERN,
        ),
    ];
    for (product, name, pattern) in patterns {
        if let Some(group) = group(product, name) {
            assert_eq!(group["values"]["$pattern"], pattern, "{product} {name}");
        }
    }
}
