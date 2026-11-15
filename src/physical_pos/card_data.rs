//! Card data in physical POS requests: card-data field names and card
//! numbers in free-text values (the gateway's own rules, shared by every SDK
//! through the parity fixture). The card is read on the device, so a request
//! that carries card data is refused before anything is sent.

use serde_json::Value;

const CARD_NUMBER_TERMS: &[&str] = &[
    "pan",
    "pannumber",
    "cardpan",
    "cardnumber",
    "cardno",
    "cardnum",
    "cardnr",
    "ccnumber",
    "ccnum",
    "primaryaccountnumber",
];
const CARD_SECRET_TERMS: &[&str] = &[
    "cvv",
    "cvv2",
    "cvc",
    "cvc2",
    "cvn",
    "cvn2",
    "cav2",
    "cvvcode",
    "cvccode",
    "cardcode",
    "securitycode",
    "cardverificationcode",
    "cardverificationvalue",
    "track1",
    "track2",
    "track3",
    "tracki",
    "trackii",
    "trackiii",
    "trackdata",
    "magstripe",
    "magneticstripe",
    "pinblock",
    "pincode",
    "pinnumber",
    "pindata",
    "cardpin",
    "cardholderpin",
    "encryptedpin",
    "expdate",
    "expmonth",
    "expyear",
    "expmmyy",
    "expirydate",
    "expirymonth",
    "expiryyear",
    "expirymmyy",
    "expirationdate",
    "expirationmonth",
    "expirationyear",
    "expiremonth",
    "expireyear",
    "cardexpiry",
    "cardexpiration",
];
const CARD_DATA_SUFFIX_TERMS: &[&str] = &[
    "cardnumber",
    "pannumber",
    "primaryaccountnumber",
    "cvv",
    "cvv2",
    "cvc",
    "cvc2",
    "cvvcode",
    "securitycode",
    "track1",
    "track2",
    "trackdata",
    "pinblock",
    "expmonth",
    "expyear",
    "expirydate",
    "expirymonth",
    "expiryyear",
    "expirationdate",
    "cardexpiry",
];
const WHOLE_KEY_TERMS: &[&str] = &["track", "expiry", "expiration", "pin", "cid", "csc"];
const MASK_TOKENS: &[&str] = &["masked", "mask"];

/// Term lists of the card-data field-name rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PhysicalPosCardDataKeyRule {
    /// Card-number terms; a key with a mask token (`maskedPan`) is allowed.
    pub number_terms: &'static [&'static str],
    /// Security code, track, PIN and expiry terms.
    pub secret_terms: &'static [&'static str],
    /// Terms that make any key ending with them card data.
    pub suffix_terms: &'static [&'static str],
    /// Terms that are card data only as the whole key (`pin`, `track`).
    pub whole_key_terms: &'static [&'static str],
    /// Tokens that mark a key as a masked value.
    pub mask_tokens: &'static [&'static str],
}

/// The card-data field-name rule (the parity test compares it with the shared fixture).
pub const PHYSICAL_POS_CARD_DATA_KEY_RULE: PhysicalPosCardDataKeyRule =
    PhysicalPosCardDataKeyRule {
        number_terms: CARD_NUMBER_TERMS,
        secret_terms: CARD_SECRET_TERMS,
        suffix_terms: CARD_DATA_SUFFIX_TERMS,
        whole_key_terms: WHOLE_KEY_TERMS,
        mask_tokens: MASK_TOKENS,
    };

fn is_lower_or_digit(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit()
}

/// Inserts a space between every adjacent pair that matches `split`.
fn split_pairs(text: &str, split: impl Fn(char, char, Option<char>) -> bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 8);
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && split(chars[i - 1], *c, chars.get(i + 1).copied()) {
            out.push(' ');
        }
        out.push(*c);
    }
    out
}

/// Word tokens of a field name: `track2Data` → `track`, `2`, `data`.
fn key_tokens(key: &str) -> Vec<String> {
    let text = split_pairs(key, |a, b, _| {
        is_lower_or_digit(a) && b.is_ascii_uppercase()
    });
    let text = split_pairs(&text, |a, b, next| {
        a.is_ascii_uppercase()
            && b.is_ascii_uppercase()
            && next.is_some_and(|n| n.is_ascii_lowercase())
    });
    let text = split_pairs(&text, |a, b, _| {
        a.is_ascii_alphabetic() && b.is_ascii_digit()
    });
    let text = split_pairs(&text, |a, b, _| {
        a.is_ascii_digit() && b.is_ascii_alphabetic()
    });
    text.to_lowercase()
        .split(|c: char| !is_lower_or_digit(c))
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

/// True when a field name is card data (`pan`, `cardNumber`, `cvv`,
/// `track2Data`, `expiryDate` …). A masked number (`maskedPan`) is not.
pub fn physical_pos_is_card_data_key(key: &str) -> bool {
    let tokens = key_tokens(key);
    if tokens.is_empty() {
        return false;
    }
    if WHOLE_KEY_TERMS.contains(&tokens.concat().as_str()) {
        return true;
    }
    let masked = tokens.iter().any(|t| MASK_TOKENS.contains(&t.as_str()));
    let number = |term: &str| masked && CARD_NUMBER_TERMS.contains(&term);
    for start in 0..tokens.len() {
        let mut joined = String::new();
        for token in &tokens[start..] {
            joined.push_str(token);
            let exact = CARD_NUMBER_TERMS.contains(&joined.as_str())
                || CARD_SECRET_TERMS.contains(&joined.as_str());
            if exact && !number(&joined) {
                return true;
            }
            if CARD_DATA_SUFFIX_TERMS
                .iter()
                .any(|term| joined.ends_with(term) && !number(term))
            {
                return true;
            }
        }
    }
    false
}

/// A run of 13 or more digits with at most one space or dash between digits.
fn has_digit_run(key: &str) -> bool {
    let bytes = key.as_bytes();
    for start in 0..bytes.len() {
        if !bytes[start].is_ascii_digit() {
            continue;
        }
        let (mut count, mut i) = (1, start + 1);
        loop {
            if bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            } else if bytes.get(i).is_some_and(|b| *b == b' ' || *b == b'-')
                && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)
            {
                i += 2;
            } else {
                break;
            }
            count += 1;
            if count >= 13 {
                return true;
            }
        }
    }
    false
}

fn identifier(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn child_path(path: &str, key: &str) -> String {
    if !identifier(key) {
        format!("{path}[?]")
    } else if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// Path of the first card-data field name in a body or filter object
/// (`items[1].cvv`), or `None`. A key holding a 13+ digit run stops at the
/// parent path (`root` at the top level, e.g. `(body)`). Values are not read.
pub fn physical_pos_find_card_data_field(value: &Value, root: &str) -> Option<String> {
    let mut stack: Vec<(&Value, String)> = vec![(value, String::new())];
    while let Some((node, path)) = stack.pop() {
        match node {
            Value::Array(list) => {
                for (i, entry) in list.iter().enumerate().rev() {
                    stack.push((entry, format!("{path}[{i}]")));
                }
            }
            Value::Object(map) => {
                for key in map.keys() {
                    if has_digit_run(key) {
                        return Some(if path.is_empty() {
                            root.to_owned()
                        } else {
                            path
                        });
                    }
                    if physical_pos_is_card_data_key(key) {
                        return Some(child_path(&path, key));
                    }
                }
                for (key, entry) in map.iter().rev() {
                    stack.push((entry, child_path(&path, key)));
                }
            }
            _ => {}
        }
    }
    None
}

fn letter_or_number(c: Option<char>) -> bool {
    c.is_some_and(char::is_alphanumeric)
}

fn luhn_valid(digits: &[u8]) -> bool {
    let mut sum = 0u32;
    for (i, b) in digits.iter().rev().enumerate() {
        let mut digit = u32::from(b - b'0');
        if i % 2 == 1 {
            digit *= 2;
            if digit > 9 {
                digit -= 9;
            }
        }
        sum += digit;
    }
    sum.is_multiple_of(10)
}

fn gtin_valid(digits: &[u8]) -> bool {
    if !(digits.len() == 8 || (12..=14).contains(&digits.len())) {
        return false;
    }
    let (check, body) = digits.split_last().unwrap_or((&b'0', &[]));
    let mut sum = 0u32;
    for (i, b) in body.iter().rev().enumerate() {
        let weight = if i % 2 == 0 { 3 } else { 1 };
        sum += u32::from(b - b'0') * weight;
    }
    (10 - sum % 10) % 10 == u32::from(check - b'0')
}

/// The candidate starting at `start` (an ASCII digit that is not glued to a
/// letter or number): the longest run of 13–19 digits, single spaces or
/// dashes between them, that is not followed by a letter, number, or a dash
/// glued to one. Returns the end index.
fn candidate_end(chars: &[char], start: usize) -> Option<usize> {
    let before = start.checked_sub(1).map(|i| chars[i]);
    let before_dash = start.checked_sub(2).map(|i| chars[i]);
    if !chars[start].is_ascii_digit()
        || letter_or_number(before)
        || (before == Some('-') && letter_or_number(before_dash))
    {
        return None;
    }
    let mut ends = vec![];
    let mut i = start + 1;
    while ends.len() < 18 {
        if chars.get(i).is_some_and(char::is_ascii_digit) {
            i += 1;
        } else if chars.get(i).is_some_and(|c| *c == ' ' || *c == '-')
            && chars.get(i + 1).is_some_and(char::is_ascii_digit)
        {
            i += 2;
        } else {
            break;
        }
        ends.push(i);
    }
    ends.iter().skip(11).rev().copied().find(|&end| {
        let next = chars.get(end).copied();
        !letter_or_number(next)
            && !(next == Some('-') && letter_or_number(chars.get(end + 1).copied()))
    })
}

/// True when the text holds a stand-alone, Luhn-valid 13–19 digit number
/// (plain, space or dash separated). With `item_text` (item names only), an
/// unseparated 13 or 14 digit number with a valid GTIN check digit is a
/// barcode, not a card.
pub fn physical_pos_contains_pan_like_value(text: &str, item_text: bool) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        let Some(end) = candidate_end(&chars, start) else {
            start += 1;
            continue;
        };
        let matched = &chars[start..end];
        let digits: Vec<u8> = matched
            .iter()
            .filter(|c| c.is_ascii_digit())
            .map(|c| *c as u8)
            .collect();
        let separated = digits.len() != matched.len();
        let barcode = item_text && !separated && digits.len() <= 14 && gtin_valid(&digits);
        if (13..=19).contains(&digits.len()) && !barcode && luhn_valid(&digits) {
            return true;
        }
        start = end;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn card_data_keys() {
        for key in [
            "pan",
            "cardNumber",
            "CVVCode",
            "track2Data",
            "expiryDate",
            "cid",
        ] {
            assert!(physical_pos_is_card_data_key(key), "{key}");
        }
        for key in [
            "maskedPan",
            "panel",
            "expiresAt",
            "pinKeyKcv",
            "card_sec_code",
        ] {
            assert!(!physical_pos_is_card_data_key(key), "{key}");
        }
    }

    #[test]
    fn card_data_paths() {
        let body =
            json!({"operation": "SALE", "items": [{"name": "a"}, {"name": "b", "cvv": "1"}]});
        assert_eq!(
            physical_pos_find_card_data_field(&body, "(body)").as_deref(),
            Some("items[1].cvv")
        );
        let body = json!({"4111111111111111": "x"});
        assert_eq!(
            physical_pos_find_card_data_field(&body, "(body)").as_deref(),
            Some("(body)")
        );
    }

    #[test]
    fn pan_values() {
        assert!(physical_pos_contains_pan_like_value(
            "4111 1111 1111 1111",
            false
        ));
        assert!(physical_pos_contains_pan_like_value(
            "ref:4111111111111111.",
            false
        ));
        assert!(!physical_pos_contains_pan_like_value(
            "ORD-4111111111111111",
            false
        ));
        assert!(!physical_pos_contains_pan_like_value(
            "4111111111111111-A",
            false
        ));
        assert!(!physical_pos_contains_pan_like_value("8691234567012", true));
        assert!(physical_pos_contains_pan_like_value("8690 000000067", true));
    }
}
