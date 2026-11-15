//! Public payment method codes (`paymentMethod`).
//!
//! The codes open for your account, with their providers and the fields each
//! provider asks for, come from
//! [`Payments::list_payment_methods`](crate::Payments::list_payment_methods).
//! `QR` is accepted as an alias of [`QR_CODE`].

use crate::{Error, Result};

/// `CARD`
pub const CARD: &str = "CARD";
/// `BNPL`
pub const BNPL: &str = "BNPL";
/// `SHOPPING_LOAN`
pub const SHOPPING_LOAN: &str = "SHOPPING_LOAN";
/// `PROMISSORY_NOTE`
pub const PROMISSORY_NOTE: &str = "PROMISSORY_NOTE";
/// `MEAL_CARD`
pub const MEAL_CARD: &str = "MEAL_CARD";
/// `GIFT_CARD`
pub const GIFT_CARD: &str = "GIFT_CARD";
/// `CASH`
pub const CASH: &str = "CASH";
/// `CASH_ON_DELIVERY`
pub const CASH_ON_DELIVERY: &str = "CASH_ON_DELIVERY";
/// `WALLET`
pub const WALLET: &str = "WALLET";
/// `BANK_TRANSFER` (Para Transferi: the buyer sends the money from their own bank).
pub const BANK_TRANSFER: &str = "BANK_TRANSFER";
/// `OPEN_BANKING`
pub const OPEN_BANKING: &str = "OPEN_BANKING";
/// `MOBILE_BILLING` (charge to the mobile phone bill).
pub const MOBILE_BILLING: &str = "MOBILE_BILLING";
/// `QR_CODE`
pub const QR_CODE: &str = "QR_CODE";
/// `CRYPTO`
pub const CRYPTO: &str = "CRYPTO";
/// `OTHER`
pub const OTHER: &str = "OTHER";
/// Alias of [`QR_CODE`]; sent as `QR_CODE`.
pub const QR: &str = "QR";

/// Every code in the documented order.
pub const ALL: &[&str] = &[
    CARD,
    BNPL,
    SHOPPING_LOAN,
    PROMISSORY_NOTE,
    MEAL_CARD,
    GIFT_CARD,
    CASH,
    CASH_ON_DELIVERY,
    WALLET,
    BANK_TRANSFER,
    OPEN_BANKING,
    MOBILE_BILLING,
    QR_CODE,
    CRYPTO,
    OTHER,
];

/// Canonical method code: `^[A-Z][A-Z0-9_]{0,31}$`, `QR` becomes `QR_CODE`.
/// A newer code than this SDK declares passes; the gateway answers an unknown
/// one with 400 `payment.method.unknown`.
pub fn normalize(code: &str) -> Result<&str> {
    let bytes = code.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.len() <= 32
        && bytes[0].is_ascii_uppercase()
        && bytes
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_');
    if !valid {
        return Err(Error::Invalid("payment method code (e.g. MOBILE_BILLING)"));
    }
    Ok(if code == QR { QR_CODE } else { code })
}

/// Name of the method's block in a request body: the camelCase form of the
/// code (`BANK_TRANSFER` → `bankTransfer`), also returned as `blockName` by
/// `list_payment_methods`.
pub fn block_name(code: &str) -> Result<String> {
    let lower = normalize(code)?.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut name = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let next = bytes.get(i + 1).copied();
        if bytes[i] == b'_' && next.is_some_and(|b| b.is_ascii_alphanumeric()) {
            name.push(char::from(bytes[i + 1].to_ascii_uppercase()));
            i += 2;
        } else {
            name.push(char::from(bytes[i]));
            i += 1;
        }
    }
    Ok(name)
}
