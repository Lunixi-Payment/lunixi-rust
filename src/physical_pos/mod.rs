//! Physical POS: card payments on in-store devices, their records, end-of-day
//! and management (`/api/v1/physical-pos`).
//!
//! * [`PhysicalPos`] (`client.physical_pos()`) — payments (start, read,
//!   list, cancel, reverse), transactions, reports and end-of-day.
//! * [`PhysicalPosManagement`] (`client.physical_pos_management()`) —
//!   overview, catalog, stores, registers, logical terminals, devices and
//!   the end-of-day policy.
//! * [`PhysicalPosWebhookEvent`] — verified `physical_pos.*` webhooks.

mod card_data;
mod management;
mod money;
mod payments;
mod request;
mod results;
mod values;
mod webhook;

pub use card_data::{
    PHYSICAL_POS_CARD_DATA_KEY_RULE, PhysicalPosCardDataKeyRule,
    physical_pos_contains_pan_like_value, physical_pos_find_card_data_field,
    physical_pos_is_card_data_key,
};
pub use management::PhysicalPosManagement;
pub use payments::PhysicalPos;
pub(crate) use request::valid_idempotency_key;
pub use results::{PhysicalPosPage, PhysicalPosWriteResult};
pub use values::*;
pub use webhook::{
    PhysicalPosCashier, PhysicalPosOriginal, PhysicalPosWebhookEvent, physical_pos_category_of,
    physical_pos_event_category,
};

/// True when `key` is a valid physical POS `Idempotency-Key`: 8–200
/// printable ASCII characters without spaces. It is sent exactly as given.
pub fn physical_pos_is_valid_idempotency_key(key: &str) -> bool {
    valid_idempotency_key(key)
}

/// Root of every physical POS route.
pub(crate) const BASE: &str = "/api/v1/physical-pos";

/// Physical POS routes use their own `Idempotency-Key` rule (8–200 characters).
pub(crate) fn is_physical_pos_path(path: &str) -> bool {
    path.strip_prefix(BASE)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}
