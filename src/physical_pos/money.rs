//! Declared physical POS money fields in answers and webhook payloads:
//! canonical minor-unit text. Counts and opaque metadata stay untouched.

use crate::{Error, Result, money::parse_minor};
use serde_json::Value;

const FIELDS: &[&str] = &[
    "amountFromMinor",
    "amountMinor",
    "amountToMinor",
    "approvedAmountMinor",
    "baseMinor",
    "capturedMinor",
    "lineTotalMinor",
    "netBaseMinor",
    "netMinor",
    "netTaxMinor",
    "refundBaseMinor",
    "refundMinor",
    "refundTaxMinor",
    "refundedAmountMinor",
    "refundedMinor",
    "reservedAmountMinor",
    "reservedMinor",
    "reversalReservedMinor",
    "reversedMinor",
    "saleBaseMinor",
    "saleMinor",
    "saleTaxMinor",
    "unitAmountMinor",
    "unitPriceMinor",
    "voidBaseMinor",
    "voidMinor",
    "voidTaxMinor",
];
const CONTAINERS: &[&str] = &[
    "data",
    "payment",
    "payments",
    "items",
    "refundItems",
    "taxes",
    "rows",
    "transaction",
    "transactions",
    "root",
    "children",
    "itemSummary",
    "amountTotals",
    "reported",
    "system",
    "difference",
    "totals",
    "lines",
];
const NULLABLE: &[&str] = &["approvedAmountMinor", "unitAmountMinor"];

/// Checks every declared money field below `value`. Values under
/// `difference` and `net…` fields may be negative.
pub(crate) fn validate_response_money(value: &Value, difference: bool) -> Result<()> {
    match value {
        Value::Array(list) => {
            for entry in list {
                validate_response_money(entry, difference)?;
            }
        }
        Value::Object(map) => {
            for (key, entry) in map {
                if FIELDS.contains(&key.as_str()) {
                    if entry.is_null() && NULLABLE.contains(&key.as_str()) {
                        continue;
                    }
                    let text = entry
                        .as_str()
                        .ok_or(Error::Response("physical POS minor-unit amount"))?;
                    parse_minor(text, false, difference || key.starts_with("net"))
                        .map_err(|_| Error::Response("physical POS minor-unit amount"))?;
                } else if CONTAINERS.contains(&key.as_str()) {
                    validate_response_money(entry, difference || key == "difference")?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
