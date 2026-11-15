//! Bank accounts (`/api/v1/bank-connect`): connected accounts, balances,
//! transactions, outgoing transfers and currency exchange.
//!
//! * [`BankConnect`] (`client.bank_connect()`) — banks, connections,
//!   accounts, balances, transactions and `request_sync`.
//! * [`BankConnectTransfers`] (`bank_connect().transfers()`) and
//!   [`BankConnectFx`] (`bank_connect().fx()`) — checked against
//!   [`BANK_TRANSFER_ROUTES`].
//! * [`BankAccountTransactionReceived`], [`BankTransferWebhookData`] —
//!   verified webhook data.

mod client;
mod values;
mod webhook;

pub use client::{BankConnect, BankConnectFx, BankConnectTransfers};
pub use values::*;
pub use webhook::{BankAccountTransactionReceived, BankTransferWebhookData};
