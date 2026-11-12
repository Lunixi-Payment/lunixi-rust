//! Lunixi product category codes for [`BasketItem::product_category`](crate::BasketItem::product_category).
//!
//! Used by payment methods with legal installment limits (e.g. shopping loan)
//! and other platform features. The dictionary only grows by appending, so the
//! field stays a plain `String` and a newer code can be sent before this SDK
//! declares a constant for it. Unknown codes are rejected by the API (HTTP 400).

/// `MOBILE_PHONE`
pub const MOBILE_PHONE: &str = "MOBILE_PHONE";
/// `TABLET`
pub const TABLET: &str = "TABLET";
/// `COMPUTER`
pub const COMPUTER: &str = "COMPUTER";
/// `TV_AUDIO`
pub const TV_AUDIO: &str = "TV_AUDIO";
/// `WHITE_GOODS`
pub const WHITE_GOODS: &str = "WHITE_GOODS";
/// `SMALL_APPLIANCE`
pub const SMALL_APPLIANCE: &str = "SMALL_APPLIANCE";
/// `CAMERA`
pub const CAMERA: &str = "CAMERA";
/// `GAME_CONSOLE`
pub const GAME_CONSOLE: &str = "GAME_CONSOLE";
/// `WEARABLE`
pub const WEARABLE: &str = "WEARABLE";
/// `ELECTRONICS_ACCESSORY`
pub const ELECTRONICS_ACCESSORY: &str = "ELECTRONICS_ACCESSORY";
/// `ELECTRONICS_OTHER`
pub const ELECTRONICS_OTHER: &str = "ELECTRONICS_OTHER";
/// `GOLD`
pub const GOLD: &str = "GOLD";
/// `JEWELRY`
pub const JEWELRY: &str = "JEWELRY";
/// `WATCH`
pub const WATCH: &str = "WATCH";
/// `APPAREL`
pub const APPAREL: &str = "APPAREL";
/// `SHOES_BAGS`
pub const SHOES_BAGS: &str = "SHOES_BAGS";
/// `COSMETICS`
pub const COSMETICS: &str = "COSMETICS";
/// `FURNITURE`
pub const FURNITURE: &str = "FURNITURE";
/// `HOME_LIVING`
pub const HOME_LIVING: &str = "HOME_LIVING";
/// `GROCERY`
pub const GROCERY: &str = "GROCERY";
/// `BOOKS_MEDIA`
pub const BOOKS_MEDIA: &str = "BOOKS_MEDIA";
/// `TOYS`
pub const TOYS: &str = "TOYS";
/// `SPORTS_OUTDOOR`
pub const SPORTS_OUTDOOR: &str = "SPORTS_OUTDOOR";
/// `PET_SUPPLIES`
pub const PET_SUPPLIES: &str = "PET_SUPPLIES";
/// `AUTO_PARTS`
pub const AUTO_PARTS: &str = "AUTO_PARTS";
/// `MUSICAL_INSTRUMENT`
pub const MUSICAL_INSTRUMENT: &str = "MUSICAL_INSTRUMENT";
/// `MICROMOBILITY`
pub const MICROMOBILITY: &str = "MICROMOBILITY";
/// `VEHICLE`
pub const VEHICLE: &str = "VEHICLE";
/// `TRANSPORT_TICKET`
pub const TRANSPORT_TICKET: &str = "TRANSPORT_TICKET";
/// `EVENT_TICKET`
pub const EVENT_TICKET: &str = "EVENT_TICKET";
/// `ACCOMMODATION`
pub const ACCOMMODATION: &str = "ACCOMMODATION";
/// `TRAVEL_PACKAGE`
pub const TRAVEL_PACKAGE: &str = "TRAVEL_PACKAGE";
/// `HEALTH_SERVICE`
pub const HEALTH_SERVICE: &str = "HEALTH_SERVICE";
/// `EDUCATION`
pub const EDUCATION: &str = "EDUCATION";
/// `SHIPPING_SERVICE`
pub const SHIPPING_SERVICE: &str = "SHIPPING_SERVICE";
/// `DIGITAL_GOODS`
pub const DIGITAL_GOODS: &str = "DIGITAL_GOODS";
/// `GIFT_CARD`
pub const GIFT_CARD: &str = "GIFT_CARD";
/// `SUBSCRIPTION`
pub const SUBSCRIPTION: &str = "SUBSCRIPTION";
/// `SERVICE_OTHER`
pub const SERVICE_OTHER: &str = "SERVICE_OTHER";
/// `OTHER`
pub const OTHER: &str = "OTHER";

/// The codes known to this SDK version, in dictionary order.
pub const ALL: &[&str] = &[
    MOBILE_PHONE,
    TABLET,
    COMPUTER,
    TV_AUDIO,
    WHITE_GOODS,
    SMALL_APPLIANCE,
    CAMERA,
    GAME_CONSOLE,
    WEARABLE,
    ELECTRONICS_ACCESSORY,
    ELECTRONICS_OTHER,
    GOLD,
    JEWELRY,
    WATCH,
    APPAREL,
    SHOES_BAGS,
    COSMETICS,
    FURNITURE,
    HOME_LIVING,
    GROCERY,
    BOOKS_MEDIA,
    TOYS,
    SPORTS_OUTDOOR,
    PET_SUPPLIES,
    AUTO_PARTS,
    MUSICAL_INSTRUMENT,
    MICROMOBILITY,
    VEHICLE,
    TRANSPORT_TICKET,
    EVENT_TICKET,
    ACCOMMODATION,
    TRAVEL_PACKAGE,
    HEALTH_SERVICE,
    EDUCATION,
    SHIPPING_SERVICE,
    DIGITAL_GOODS,
    GIFT_CARD,
    SUBSCRIPTION,
    SERVICE_OTHER,
    OTHER,
];
