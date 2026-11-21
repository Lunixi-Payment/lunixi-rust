//! RFC 3339 timestamps and calendar dates without a date-time dependency.
//!
//! Only what the SDK needs: format the current UTC time for request signing,
//! parse a signed RFC 3339 timestamp into a [`SystemTime`], and check a
//! proleptic Gregorian calendar date.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SECONDS_PER_DAY: i64 = 86_400;

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Proleptic Gregorian date for days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = (if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    }) as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Whether `year-month-day` exists in the proleptic Gregorian calendar.
pub(crate) fn is_valid_date(year: i64, month: u32, day: u32) -> bool {
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days_in_month).contains(&day)
}

/// Seconds and nanoseconds relative to the Unix epoch (seconds may be negative).
fn unix_parts(time: SystemTime) -> (i64, u32) {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => (after.as_secs() as i64, after.subsec_nanos()),
        Err(before) => {
            let before = before.duration();
            let secs = -(before.as_secs() as i64);
            match before.subsec_nanos() {
                0 => (secs, 0),
                nanos => (secs - 1, 1_000_000_000 - nanos),
            }
        }
    }
}

fn from_unix_parts(secs: i64, nanos: u32) -> Option<SystemTime> {
    if secs >= 0 {
        UNIX_EPOCH.checked_add(Duration::new(secs as u64, nanos))
    } else {
        UNIX_EPOCH
            .checked_sub(Duration::from_secs(secs.unsigned_abs()))?
            .checked_add(Duration::new(0, nanos))
    }
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` in UTC, the format of the signed `X-Lunixi-Date` header.
pub(crate) fn format_rfc3339_millis(time: SystemTime) -> String {
    let (secs, nanos) = unix_parts(time);
    let (year, month, day) = civil_from_days(secs.div_euclid(SECONDS_PER_DAY));
    let second_of_day = secs.rem_euclid(SECONDS_PER_DAY);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        second_of_day / 3600,
        second_of_day / 60 % 60,
        second_of_day % 60,
        nanos / 1_000_000
    )
}

fn digits(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    bytes.iter().try_fold(0u32, |acc, b| {
        acc.checked_mul(10)?.checked_add(u32::from(b - b'0'))
    })
}

/// Parses an RFC 3339 date-time: `YYYY-MM-DD`, `T` (or `t` / space), `HH:MM:SS`,
/// an optional fraction, then `Z` or a `±HH:MM` offset. Leap seconds are rejected.
pub(crate) fn parse_rfc3339(text: &str) -> Option<SystemTime> {
    let bytes = text.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't' | b' ')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let year = digits(&bytes[0..4])?;
    let month = digits(&bytes[5..7])?;
    let day = digits(&bytes[8..10])?;
    let hour = digits(&bytes[11..13])?;
    let minute = digits(&bytes[14..16])?;
    let second = digits(&bytes[17..19])?;
    if !is_valid_date(i64::from(year), month, day) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let mut rest = &bytes[19..];
    let mut nanos = 0u32;
    if let Some(fraction) = rest.strip_prefix(b".") {
        let length = fraction.iter().take_while(|b| b.is_ascii_digit()).count();
        if length == 0 {
            return None;
        }
        for (index, digit) in fraction[..length].iter().take(9).enumerate() {
            nanos += u32::from(digit - b'0') * 10u32.pow(8 - index as u32);
        }
        rest = &fraction[length..];
    }
    let offset_seconds = match rest {
        [b'Z' | b'z'] => 0i64,
        [sign @ (b'+' | b'-'), h1, h2, b':', m1, m2] => {
            let hours = digits(&[*h1, *h2])?;
            let minutes = digits(&[*m1, *m2])?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            let offset = i64::from(hours * 3600 + minutes * 60);
            if *sign == b'-' { -offset } else { offset }
        }
        _ => return None,
    };
    let local = days_from_civil(i64::from(year), month, day) * SECONDS_PER_DAY
        + i64::from(hour * 3600 + minute * 60 + second);
    from_unix_parts(local - offset_seconds, nanos)
}

/// Absolute distance between two instants.
pub(crate) fn distance(a: SystemTime, b: SystemTime) -> Duration {
    a.duration_since(b).unwrap_or_else(|error| error.duration())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_parses_round_trip() {
        let time = parse_rfc3339("2026-11-21T04:33:12.345Z").unwrap();
        assert_eq!(format_rfc3339_millis(time), "2026-11-21T04:33:12.345Z");
        assert_eq!(unix_parts(time), (1_795_235_592, 345_000_000));
    }

    #[test]
    fn applies_offsets_and_fractions() {
        let utc = parse_rfc3339("2026-02-28T22:30:00Z").unwrap();
        assert_eq!(parse_rfc3339("2026-03-01T01:30:00+03:00"), Some(utc));
        assert_eq!(
            parse_rfc3339("2026-02-28t19:30:00.000000000-03:00"),
            Some(utc)
        );
        let fine = parse_rfc3339("1969-12-31T23:59:59.999999999123Z").unwrap();
        assert_eq!(unix_parts(fine), (-1, 999_999_999));
        assert_eq!(format_rfc3339_millis(fine), "1969-12-31T23:59:59.999Z");
    }

    #[test]
    fn rejects_malformed_input() {
        for text in [
            "",
            "2026-11-21",
            "2026-11-21T04:33:12",
            "2026-11-21T04:33:12+0300",
            "2026-02-29T00:00:00Z",
            "2026-11-21T24:00:00Z",
            "2026-11-21T23:59:60Z",
            "2026-11-21T04:33:12.Z",
            "2026-11-21T04:33:12Zjunk",
            "+026-11-21T04:33:12Z",
        ] {
            assert_eq!(parse_rfc3339(text), None, "{text}");
        }
    }

    #[test]
    fn checks_calendar_dates() {
        assert!(is_valid_date(2024, 2, 29));
        assert!(is_valid_date(2000, 2, 29));
        assert!(!is_valid_date(1900, 2, 29));
        assert!(!is_valid_date(2026, 4, 31));
        assert!(!is_valid_date(2026, 13, 1));
        assert!(!is_valid_date(2026, 1, 0));
    }

    #[test]
    fn civil_conversion_is_consistent() {
        for days in [-719_468, -1, 0, 1, 10_957, 20_778, 2_932_896] {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days);
        }
    }
}
