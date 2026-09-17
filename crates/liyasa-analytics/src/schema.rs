//! The published event schema (ANA-02).
//!
//! §34.6's JSON is the normative example and `EventRecord` is what the server
//! writes; the two disagree about `ts`, about `utm`, and about whether
//! `format` and `caller.kind` are closed sets. RFC 1701 records the three and
//! the reading taken here: the schema accepts both `ts` forms, carries `utm`
//! as an optional object derived from the route, and closes the two enums.

use serde_json::{Value, json};

/// Where the schema is served (ANA-02).
pub const PATH: &str = "/_liyasa/schema/event.json";

/// The schema's own identifier, which is what a `$ref` from another document
/// points at.
pub const ID: &str = "https://liyasa.dev/schema/event.json";

/// `format`, closed by ANA-02.
pub const FORMATS: &[&str] = &["html", "markdown", "json"];

/// `caller.kind`, closed by ANA-02.
pub const CALLER_KINDS: &[&str] = &["human", "agent", "bot", "integration"];

/// The JSON Schema served at [`PATH`].
pub fn event() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": ID,
        "title": "Liyasa analytics event",
        "description": "One row of the analytics event stream (PRD §26.1 ANA-02, §34.6).",
        "type": "object",
        "required": ["ts", "site", "env", "route", "type"],
        "additionalProperties": false,
        "properties": {
            "ts": {
                "description": "When it happened: milliseconds since the Unix epoch, or an RFC 3339 instant (RFC 1701).",
                "oneOf": [
                    { "type": "integer", "minimum": 0 },
                    { "type": "string", "format": "date-time" }
                ]
            },
            "site": { "type": "string", "minLength": 1 },
            "env": { "type": "string", "minLength": 1 },
            "route": {
                "type": "string",
                "description": "Path, with only `utm_*` and `q` surviving from the query string (ANA-03)."
            },
            "type": { "type": "string", "minLength": 1 },
            "variant": {
                "type": ["object", "null"],
                "additionalProperties": false,
                "properties": {
                    "version": { "type": ["string", "null"] },
                    "locale": { "type": ["string", "null"] },
                    "product": { "type": ["string", "null"] },
                    "region": { "type": ["string", "null"] }
                }
            },
            "props": {
                "type": ["object", "null"],
                "description": "Per-type payload, scrubbed of free text before storage (ANA-03)."
            },
            "session_key": {
                "type": "string",
                "description": "`blake3(salt_day, address, user agent family, site)`; the salt is destroyed at rotation, so this cannot be reversed or joined across days (ANA-03).",
                "pattern": "^(k1:[0-9a-f]{32})?$"
            },
            "caller": {
                "type": ["object", "null"],
                "additionalProperties": false,
                "properties": {
                    "kind": { "type": "string", "enum": CALLER_KINDS },
                    "agent_name": { "type": ["string", "null"] },
                    "headless": { "type": ["string", "null"] }
                }
            },
            "referrer_host": {
                "type": ["string", "null"],
                "description": "Host only; a referring URL is never recorded."
            },
            "utm": {
                "type": ["object", "null"],
                "additionalProperties": false,
                "properties": {
                    "source": { "type": ["string", "null"] },
                    "medium": { "type": ["string", "null"] },
                    "campaign": { "type": ["string", "null"] },
                    "term": { "type": ["string", "null"] },
                    "content": { "type": ["string", "null"] }
                }
            },
            "device": {
                "type": ["object", "null"],
                "additionalProperties": false,
                "properties": {
                    "class": { "type": ["string", "null"], "enum": ["desktop", "mobile", "tablet", "server", null] },
                    "os_family": { "type": ["string", "null"] },
                    "browser_family": { "type": ["string", "null"] }
                }
            },
            "country": {
                "type": ["string", "null"],
                "description": "From the edge header when present, never from an address lookup (ANA-02).",
                "pattern": "^[A-Z]{2}$"
            },
            "format": { "type": "string", "enum": FORMATS },
            "duration_ms": { "type": ["integer", "null"], "minimum": 0 }
        }
    })
}

/// Milliseconds since the epoch from either accepted `ts` form (RFC 1701).
pub fn timestamp_ms(ts: &Value) -> Option<i64> {
    match ts {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => parse_rfc3339_ms(s),
        _ => None,
    }
}

/// RFC 3339 to milliseconds, without a date crate (§6.2.1 has none).
///
/// Accepts `YYYY-MM-DDTHH:MM:SS`, an optional fractional second, and either
/// `Z` or a `±HH:MM` offset.
pub fn parse_rfc3339_ms(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let separator = bytes[10];
    if separator != b'T' && separator != b't' && separator != b' ' {
        return None;
    }
    let year: i64 = text.get(0..4)?.parse().ok()?;
    let month: i64 = text.get(5..7)?.parse().ok()?;
    let day: i64 = text.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let hour: i64 = text.get(11..13)?.parse().ok()?;
    let minute: i64 = text.get(14..17)?.strip_suffix(':')?.parse().ok()?;
    let second: i64 = text.get(17..19)?.parse().ok()?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let mut rest = text.get(19..)?;
    let mut millis = 0i64;
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits: String = fraction.chars().take_while(char::is_ascii_digit).collect();
        rest = &fraction[digits.len()..];
        // Three digits of it; the rest is finer than anything stored.
        let mut scaled = digits.chars().take(3).collect::<String>();
        while scaled.len() < 3 {
            scaled.push('0');
        }
        millis = scaled.parse().ok()?;
    }

    let offset_minutes = match rest.as_bytes().first() {
        Some(b'Z' | b'z') | None => 0,
        Some(sign @ (b'+' | b'-')) => {
            let hours: i64 = rest.get(1..3)?.parse().ok()?;
            let minutes: i64 = rest.get(4..6)?.parse().ok()?;
            let total = hours * 60 + minutes;
            if *sign == b'-' { -total } else { total }
        }
        _ => return None,
    };

    let days = days_from_civil(year, month, day);
    let seconds = days * 86_400 + (hour * 3_600) + (minute * 60) + second - offset_minutes * 60;
    Some(seconds * 1_000 + millis)
}

/// `YYYY-MM-DD` for an instant in milliseconds, in UTC.
///
/// The inverse of [`days_from_civil`], and the only date formatting in the
/// workspace: §6.2.1 has no date crate, and a digest that says "week of
/// 1789344000000" is not a digest.
pub fn format_date_ms(ms: i64) -> String {
    let days = ms.div_euclid(crate::query::DAY_MS);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// A proleptic Gregorian date from days since `1970-01-01` (Hinnant).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_position + 2) / 5 + 1) as u32;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Days between `1970-01-01` and a proleptic Gregorian date (Hinnant).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_position = (month + 9) % 12;
    let day_of_year = (153 * month_position + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The `utm` object ANA-02 lists, derived from the route WP-14 stores it on
/// (RFC 1701). `None` when the route carries no campaign parameter.
pub fn utm_from_route(route: &str) -> Option<Value> {
    let query = route.split_once('?')?.1;
    let mut object = serde_json::Map::new();
    for (name, value) in url::form_urlencoded::parse(query.as_bytes()) {
        let field = match name.as_ref() {
            "utm_source" => "source",
            "utm_medium" => "medium",
            "utm_campaign" => "campaign",
            "utm_term" => "term",
            "utm_content" => "content",
            _ => continue,
        };
        if !value.is_empty() {
            object.insert(field.to_owned(), Value::String(value.into_owned()));
        }
    }
    (!object.is_empty()).then_some(Value::Object(object))
}
