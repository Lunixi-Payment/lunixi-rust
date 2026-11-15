//! Wallet: the closed-loop wallet's merchant surface (`/api/v1/wallet`).
//!
//! [`Wallet`] (`client.wallet()`) with the sub-facades `programs()`,
//! `end_users()`, `wallets()`, `deposits()`, `corporate()`, `fees()`,
//! `transfers()`, `withdrawals()`, `operations()`, `topups()`,
//! `payments()`, `qr()` and `bulk_payouts()`. Every call is checked against
//! [`WALLET_ROUTES`] before sending.

mod client;
mod values;

pub use client::{
    Wallet, WalletAccounts, WalletBulkPayouts, WalletCorporate, WalletDeposits, WalletEndUsers,
    WalletFees, WalletOperations, WalletPayments, WalletPrograms, WalletQr, WalletQrImage,
    WalletTopups, WalletTransfers, WalletWithdrawals,
};
pub use values::*;
