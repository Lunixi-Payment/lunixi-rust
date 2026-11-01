use crate::{Error, Result};
use serde_json::Value;

pub(crate) fn parse_minor(value: &str, positive: bool, signed: bool) -> Result<i64> {
    let digits = if signed {
        value.strip_prefix('-').unwrap_or(value)
    } else {
        value
    };
    if digits.is_empty()
        || !digits.bytes().all(|c| c.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
        || value == "-0"
        || (positive && digits == "0")
    {
        return Err(Error::Invalid("canonical minor-unit string"));
    }
    value
        .parse::<i64>()
        .map_err(|_| Error::Invalid("signed 64-bit fiat amount"))
}
/// Converts decimal text using the actual supported currency exponent, without floating point.
pub fn decimal_to_minor(value: &str, currency: &str) -> Result<String> {
    let exponent = match currency {
        "JPY" => 0,
        "KWD" => 3,
        "TRY" | "USD" | "EUR" | "GBP" | "RUB" | "AED" | "SAR" | "QAR" | "CHF" | "CAD" | "AUD"
        | "CNY" | "AZN" => 2,
        _ => return Err(Error::Invalid("unsupported currency")),
    };
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || (whole.len() > 1 && whole.starts_with('0'))
        || !fraction.bytes().all(|c| c.is_ascii_digit())
        || (value.contains('.') && fraction.is_empty())
    {
        return Err(Error::Invalid("decimal text"));
    }
    if fraction.len() > exponent && fraction[exponent..].bytes().any(|c| c != b'0') {
        return Err(Error::Invalid("currency precision"));
    }
    let used = &fraction[..fraction.len().min(exponent)];
    let text = format!("{}{}{}", whole, used, "0".repeat(exponent - used.len()));
    let text = text.trim_start_matches('0');
    let text = if text.is_empty() { "0" } else { text };
    parse_minor(text, false, false)?;
    Ok(text.to_owned())
}
pub(crate) fn validate_fields(value: &Value, response: bool) -> Result<()> {
    match value {
        Value::Array(list) => {
            for entry in list {
                validate_fields(entry, response)?;
            }
        }
        Value::Object(map) => {
            for (key, entry) in map {
                if entry.is_null()
                    || ["metadata", "fields", "inputFields", "insuranceData"]
                        .contains(&key.as_str())
                {
                    continue;
                }
                let money = [
                    "amount",
                    "amountMinor",
                    "authorizedAmount",
                    "availableBalance",
                    "balanceMinor",
                    "buyerTotalCharge",
                    "cancelledPremiumAmount",
                    "capturedAmount",
                    "capturedAmountMinor",
                    "chargeAmount",
                    "collectedAmountMinor",
                    "collectedPremiumAmount",
                    "compensatedAmount",
                    "disputedAmount",
                    "feeTotal",
                    "fixedFee",
                    "inForcePremiumAmount",
                    "insuranceValueAmount",
                    "legAmount",
                    "legsAmount",
                    "lineTotalMinor",
                    "maxAmount",
                    "maxAmountMinor",
                    "minAmount",
                    "minAmountMinor",
                    "nonRefundableAmount",
                    "paidAmount",
                    "paidPrice",
                    "pendingBalance",
                    "premiumAmount",
                    "price",
                    "refundAmount",
                    "refundOpenPremiumAmount",
                    "refundableAmount",
                    "refundedAmount",
                    "refundedAmountMinor",
                    "refundedPremiumAmount",
                    "requestedAmount",
                    "reservedBalance",
                    "sourceAmountMinor",
                    "targetAmountMinor",
                    "taxAmountMinor",
                    "taxTotal",
                    "totalAmount",
                    "unitPriceMinor",
                ]
                .contains(&key.as_str());
                if money {
                    let text = entry
                        .as_str()
                        .ok_or(Error::Invalid("canonical minor-unit string required"))?;
                    parse_minor(
                        text,
                        !response && ["amount", "price", "totalAmount"].contains(&key.as_str()),
                        key.to_lowercase().contains("balance"),
                    )?;
                } else if [
                    "data",
                    "details",
                    "items",
                    "rows",
                    "results",
                    "payments",
                    "operations",
                    "balances",
                    "wallets",
                    "legs",
                    "aggregate",
                    "refund",
                    "refunds",
                    "totals",
                    "charge",
                    "fees",
                    "fee",
                    "balance",
                    "splits",
                    "sellers",
                    "basketItems",
                    "promissoryNote",
                    "schedule",
                    "notes",
                    "lines",
                    "offers",
                    "policies",
                    "reconciliation",
                    "quotes",
                    "payment",
                    "transactions",
                    "accounts",
                    "settlements",
                    "payouts",
                    "limits",
                ]
                .contains(&key.as_str())
                {
                    validate_fields(entry, response)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn canonical_money_strings_are_exact() {
        let large = "9007199254740993";
        for (decimal, currency) in [
            (large, "JPY"),
            ("90071992547409.93", "TRY"),
            ("9007199254740.993", "KWD"),
        ] {
            assert_eq!(decimal_to_minor(decimal, currency).unwrap(), large);
        }
        assert!(parse_minor("-9007199254740993", false, true).is_ok());
        for value in [
            "0",
            "-0",
            "01",
            "+1",
            " 1",
            "1.0",
            "1e3",
            "9223372036854775808",
        ] {
            assert!(parse_minor(value, true, false).is_err());
        }
        assert!(validate_fields(&json!({"data":{"amount":1}}), true).is_err());
        assert!(validate_fields(&json!({"amount":large,"metadata":{"amount":1},"providerPayload":{"amount":1},"rate":1.5}), false).is_ok());
        assert!(decimal_to_minor("1.1", "JPY").is_err());
        assert!(decimal_to_minor("1", "ZZZ").is_err());
    }
}

pub(crate) fn fiat_route(path: &str) -> bool {
    [
        "/api/v1/payments",
        "/api/v1/wallet",
        "/api/v1/insurance",
        "/api/v1/bank-connect",
        "/api/v1/fraud",
    ]
    .iter()
    .any(|base| path == *base || path.starts_with(&format!("{base}/")))
}
