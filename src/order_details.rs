//! Order details every payment endpoint requires: the buyer, the billing
//! address and at least one basket item; a basket with a `PHYSICAL` item also
//! requires the shipping address. Payment providers do not start a payment
//! without them.
//!
//! The gateway refuses a missing field with `payment.order.field_required`
//! and an invalid one with `payment.order.field_invalid`; `details.field` is
//! the dotted path of the first failing field, `details.reason` the reason and
//! `details.fields` every field failing for the same reason. The SDK checks
//! the same rule before sending and names the same paths
//! ([`Error::MissingOrderDetails`]).

use crate::{Error, Result};
use serde_json::{Map, Value};

/// Order objects required on every payment endpoint.
pub const ORDER_DETAILS_REQUIRED_FIELDS: &[&str] = &["buyer", "billingAddress", "basketItems"];

/// `buyer` fields that must be non-empty strings. `identityNumber`: Turkish
/// ID or passport number; `email`: ASCII; `ip`: the buyer's browser IP
/// (IPv4 or IPv6).
pub const BUYER_REQUIRED_FIELDS: &[&str] = &[
    "name",
    "surname",
    "identityNumber",
    "email",
    "gsmNumber",
    "city",
    "country",
    "zipCode",
    "ip",
];

/// `buyer` fields a checkout intent may leave out (`create_intent`,
/// `create_marketplace_intent`, marketplace `create_checkout_intent`): the
/// Lunixi form takes the IP from the buyer's browser. A given `ip` is still
/// sent and wins.
pub const CHECKOUT_INTENT_BUYER_OPTIONAL_FIELDS: &[&str] = &["ip"];

/// Fields of `billingAddress` and `shippingAddress` that must be non-empty strings.
pub const ADDRESS_REQUIRED_FIELDS: &[&str] =
    &["contactName", "address", "city", "country", "zipCode"];

/// Fields of each `basketItems[]` entry the gateway requires.
pub const BASKET_ITEM_REQUIRED_FIELDS: &[&str] = &["id", "name", "category1", "itemType", "price"];

/// Error codes of a missing or invalid order field.
pub mod order_field_error_code {
    /// A required order field is missing (`details.fields` lists the paths).
    pub const FIELD_REQUIRED: &str = "payment.order.field_required";
    /// An order field has the wrong type, format or length.
    pub const FIELD_INVALID: &str = "payment.order.field_invalid";
    /// Every code.
    pub const ALL: &[&str] = &[FIELD_REQUIRED, FIELD_INVALID];
}

/// `details.reason` of an order field error.
pub mod order_field_reason {
    /// The field is missing or empty.
    pub const REQUIRED: &str = "required";
    /// `shippingAddress` is missing while the basket has a `PHYSICAL` item.
    pub const REQUIRED_WHEN_PHYSICAL: &str = "required_when_physical";
    /// The field has the wrong JSON type.
    pub const INVALID_TYPE: &str = "invalid_type";
    /// The field does not have the expected format (e-mail, IP …).
    pub const INVALID_FORMAT: &str = "invalid_format";
    /// The field is longer than the gateway accepts.
    pub const TOO_LONG: &str = "too_long";
    /// `basketItems` is empty.
    pub const TOO_FEW_ITEMS: &str = "too_few_items";
    /// Every reason.
    pub const ALL: &[&str] = &[
        REQUIRED,
        REQUIRED_WHEN_PHYSICAL,
        INVALID_TYPE,
        INVALID_FORMAT,
        TOO_LONG,
        TOO_FEW_ITEMS,
    ];
}

/// Which order details a payment body must carry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum OrderDetailsScope {
    /// The buyer (with `ip`), the billing address and at least one basket
    /// item; `shippingAddress` when an item is `PHYSICAL`. Direct, split and
    /// marketplace card payments.
    #[default]
    Payment,
    /// As [`Payment`](Self::Payment), but the buyer may leave out
    /// [`CHECKOUT_INTENT_BUYER_OPTIONAL_FIELDS`]. Checkout intents.
    CheckoutIntent,
    /// The buyer and the billing address only. Card storage and the wallet
    /// card top-up.
    BuyerAndBillingAddress,
}

fn filled(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
}

fn given<'a>(body: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    body.get(key).filter(|value| !value.is_null())
}

fn missing_fields_of(
    value: Option<&Value>,
    fields: &[&str],
    skip: &[&str],
    path: &str,
    missing: &mut Vec<String>,
) {
    let Some(value) = value else {
        missing.push(path.to_owned());
        return;
    };
    let Some(object) = value.as_object() else {
        missing.push(path.to_owned());
        return;
    };
    for field in fields.iter().filter(|field| !skip.contains(field)) {
        if !filled(object.get(*field)) {
            missing.push(format!("{path}.{field}"));
        }
    }
}

/// Paths of the missing order fields of a payment body, in the gateway's
/// format (`buyer`, `buyer.email`, `billingAddress.city`,
/// `basketItems.0.category1`, `shippingAddress`). An empty list means the
/// order details are complete for `scope`.
pub fn missing_order_details(body: &Value, scope: OrderDetailsScope) -> Vec<String> {
    let empty = Map::new();
    let body = body.as_object().unwrap_or(&empty);
    let buyer_skip: &[&str] = if scope == OrderDetailsScope::CheckoutIntent {
        CHECKOUT_INTENT_BUYER_OPTIONAL_FIELDS
    } else {
        &[]
    };
    let mut missing = Vec::new();
    missing_fields_of(
        given(body, "buyer"),
        BUYER_REQUIRED_FIELDS,
        buyer_skip,
        "buyer",
        &mut missing,
    );
    missing_fields_of(
        given(body, "billingAddress"),
        ADDRESS_REQUIRED_FIELDS,
        &[],
        "billingAddress",
        &mut missing,
    );
    if scope == OrderDetailsScope::BuyerAndBillingAddress {
        return missing;
    }
    let Some(items) = given(body, "basketItems")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
    else {
        missing.push("basketItems".to_owned());
        return missing;
    };
    let mut physical = false;
    for (index, item) in items.iter().enumerate() {
        let Some(item) = item.as_object() else {
            missing.push(format!("basketItems.{index}"));
            continue;
        };
        for field in BASKET_ITEM_REQUIRED_FIELDS {
            let present = match item.get(*field) {
                None | Some(Value::Null) => false,
                Some(Value::String(text)) => !text.trim().is_empty(),
                Some(_) => true,
            };
            if !present {
                missing.push(format!("basketItems.{index}.{field}"));
            }
        }
        physical = physical || item.get("itemType").and_then(Value::as_str) == Some("PHYSICAL");
    }
    let shipping = given(body, "shippingAddress");
    if physical || shipping.is_some() {
        missing_fields_of(
            shipping,
            ADDRESS_REQUIRED_FIELDS,
            &[],
            "shippingAddress",
            &mut missing,
        );
    }
    missing
}

/// [`Error::MissingOrderDetails`] naming the missing order fields of `body`.
pub(crate) fn assert_order_details(
    body: &Value,
    operation: &'static str,
    scope: OrderDetailsScope,
) -> Result<()> {
    let fields = missing_order_details(body, scope);
    if fields.is_empty() {
        Ok(())
    } else {
        Err(Error::MissingOrderDetails { operation, fields })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn complete() -> Value {
        json!({
            "buyer": {
                "name": "Ada", "surname": "Yilmaz", "identityNumber": "11111111111",
                "email": "ada@example.com", "gsmNumber": "+905350000000",
                "city": "Istanbul", "country": "Turkey", "zipCode": "34000", "ip": "203.0.113.7"
            },
            "billingAddress": {
                "contactName": "Ada Yilmaz", "address": "Example Street 1",
                "city": "Istanbul", "country": "Turkey", "zipCode": "34000"
            },
            "basketItems": [
                {"id": "SKU-1", "name": "Gift card", "category1": "Digital", "itemType": "VIRTUAL", "price": "1990"}
            ]
        })
    }

    #[test]
    fn complete_order_details_have_no_missing_paths() {
        assert!(missing_order_details(&complete(), OrderDetailsScope::Payment).is_empty());
    }

    #[test]
    fn missing_objects_are_named_by_their_key() {
        assert_eq!(
            missing_order_details(&json!({}), OrderDetailsScope::Payment),
            ["buyer", "billingAddress", "basketItems"]
        );
        assert_eq!(
            missing_order_details(
                &json!({"buyer": null, "basketItems": []}),
                OrderDetailsScope::Payment
            ),
            ["buyer", "billingAddress", "basketItems"]
        );
        assert_eq!(
            missing_order_details(&json!({}), OrderDetailsScope::BuyerAndBillingAddress),
            ["buyer", "billingAddress"]
        );
    }

    #[test]
    fn missing_fields_are_named_by_their_path() {
        let mut body = complete();
        body["buyer"]["email"] = json!(" ");
        body["billingAddress"]
            .as_object_mut()
            .map(|address| address.remove("city"));
        body["basketItems"][0]
            .as_object_mut()
            .map(|item| item.remove("category1"));
        assert_eq!(
            missing_order_details(&body, OrderDetailsScope::Payment),
            [
                "buyer.email",
                "billingAddress.city",
                "basketItems.0.category1"
            ]
        );
    }

    #[test]
    fn buyer_ip_is_optional_on_checkout_intents_only() {
        let mut body = complete();
        body["buyer"]
            .as_object_mut()
            .map(|buyer| buyer.remove("ip"));
        assert!(missing_order_details(&body, OrderDetailsScope::CheckoutIntent).is_empty());
        assert_eq!(
            missing_order_details(&body, OrderDetailsScope::Payment),
            ["buyer.ip"]
        );
        assert_eq!(
            missing_order_details(&body, OrderDetailsScope::BuyerAndBillingAddress),
            ["buyer.ip"]
        );
    }

    #[test]
    fn physical_item_requires_the_shipping_address() {
        let mut body = complete();
        body["basketItems"][0]["itemType"] = json!("PHYSICAL");
        assert_eq!(
            missing_order_details(&body, OrderDetailsScope::Payment),
            ["shippingAddress"]
        );
        body["shippingAddress"] =
            json!({"contactName": "Ada Yilmaz", "address": "Example Street 1"});
        assert_eq!(
            missing_order_details(&body, OrderDetailsScope::Payment),
            [
                "shippingAddress.city",
                "shippingAddress.country",
                "shippingAddress.zipCode"
            ]
        );
    }

    #[test]
    fn non_object_entries_are_named_by_index() {
        let mut body = complete();
        body["basketItems"] = json!([body["basketItems"][0].clone(), "line"]);
        assert_eq!(
            missing_order_details(&body, OrderDetailsScope::Payment),
            ["basketItems.1"]
        );
    }

    #[test]
    fn the_error_names_the_operation_and_the_paths() {
        let error =
            assert_order_details(&json!({}), "A payment intent", OrderDetailsScope::Payment).err();
        assert_eq!(
            error.as_ref().map(ToString::to_string).as_deref(),
            Some(
                "lunixi: A payment intent requires the order details: buyer, billingAddress, basketItems"
            )
        );
        assert_eq!(
            order_field_error_code::ALL,
            [
                "payment.order.field_required",
                "payment.order.field_invalid"
            ]
        );
    }
}
