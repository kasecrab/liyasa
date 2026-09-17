//! Reading `liyasa build --build-time` (§6.6.2 rule 1).
//!
//! The build clock's first source is `SOURCE_DATE_EPOCH`, which `liyasa-build`
//! reads for itself; this flag is the second, and the engine takes it as
//! seconds since the Unix epoch. So an integer is the form that cannot be
//! misread — it is the same value the variable carries.
//!
//! An RFC 3339 timestamp is accepted as well, because the person setting a
//! reproducible build date has a date rather than an epoch, and computing one
//! by hand is the sort of step that gets a release wrong. The two cannot be
//! confused: an integer is tried first, and no integer contains a `T`.

/// Seconds since the Unix epoch, from either form.
pub fn parse(text: &str) -> Result<i64, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("a build time is a Unix timestamp or an RFC 3339 date".to_owned());
    }
    if let Ok(seconds) = text.parse::<i64>() {
        return Ok(seconds);
    }
    rfc3339(text).ok_or_else(|| {
        format!("`{text}` is neither a Unix timestamp nor an RFC 3339 date such as `2026-09-17T00:00:00Z`")
    })
}

/// `2026-09-17T00:00:00Z`, or the same with a `+HH:MM` offset. A space in
/// place of the `T` is accepted because shells and spreadsheets produce it.
fn rfc3339(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let year: i64 = text.get(0..4)?.parse().ok()?;
    let month: i64 = text.get(5..7)?.parse().ok()?;
    let day: i64 = text.get(8..10)?.parse().ok()?;
    if bytes[4] != b'-' || bytes[7] != b'-' || !matches!(bytes[10], b'T' | b't' | b' ') {
        return None;
    }
    let hour: i64 = text.get(11..13)?.parse().ok()?;
    let minute: i64 = text.get(14..16)?.parse().ok()?;
    let second: i64 = text.get(17..19)?.parse().ok()?;
    if bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        // A leap second is a real timestamp and maps to the following minute.
        || second > 60
    {
        return None;
    }

    // Anything after the seconds is a fractional part, which the build clock
    // has no room for, and then the offset.
    let mut rest = text.get(19..)?;
    if let Some(stripped) = rest.strip_prefix('.') {
        let digits = stripped
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(stripped.len());
        if digits == 0 {
            return None;
        }
        rest = stripped.get(digits..)?;
    }

    let offset = match rest {
        "Z" | "z" => 0,
        "" => return None,
        other => {
            let sign = match other.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let hours: i64 = other.get(1..3)?.parse().ok()?;
            let minutes: i64 = other.get(4..6)?.parse().ok()?;
            if other.as_bytes().get(3) != Some(&b':') || other.len() != 6 {
                return None;
            }
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 3600 + minutes * 60)
        }
    };

    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

/// Days between 1970-01-01 and the given date, by Howard Hinnant's algorithm.
/// Correct for every proleptic Gregorian date, before the epoch included.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_index = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unix_timestamp_is_taken_as_it_is() {
        assert_eq!(parse("0"), Ok(0));
        assert_eq!(parse("1789603200"), Ok(1_789_603_200));
        assert_eq!(parse("  1789603200  "), Ok(1_789_603_200));
    }

    /// The engine's own tests cover a negative build time, so the flag has to
    /// be able to express one.
    #[test]
    fn a_timestamp_before_the_epoch_is_a_timestamp() {
        assert_eq!(parse("-86400"), Ok(-86_400));
        assert_eq!(parse("1969-12-31T23:59:59Z"), Ok(-1));
    }

    #[test]
    fn an_rfc_3339_date_becomes_the_same_instant() {
        assert_eq!(parse("1970-01-01T00:00:00Z"), Ok(0));
        assert_eq!(parse("2000-01-01T00:00:00Z"), Ok(946_684_800));
        assert_eq!(parse("2026-09-17T00:00:00Z"), Ok(1_789_603_200));
        assert_eq!(parse("2026-09-17T12:34:56Z"), Ok(1_789_648_496));
    }

    #[test]
    fn a_leap_day_is_a_day() {
        assert_eq!(parse("2024-02-29T00:00:00Z"), Ok(1_709_164_800));
    }

    #[test]
    fn an_offset_is_applied_rather_than_ignored() {
        assert_eq!(parse("2026-09-17T01:00:00+01:00"), Ok(1_789_603_200));
        assert_eq!(parse("2026-09-16T23:00:00-01:00"), Ok(1_789_603_200));
    }

    #[test]
    fn a_fractional_second_is_dropped_rather_than_refused() {
        assert_eq!(parse("2026-09-17T00:00:00.000Z"), Ok(1_789_603_200));
        assert_eq!(parse("2026-09-17T00:00:00.123456Z"), Ok(1_789_603_200));
    }

    #[test]
    fn a_space_in_place_of_the_t_is_accepted() {
        assert_eq!(parse("2026-09-17 00:00:00Z"), Ok(1_789_603_200));
    }

    /// A date with no zone is refused rather than assumed: guessing the local
    /// zone would make the same command produce different output on two
    /// machines, which is the opposite of what the flag is for.
    #[test]
    fn a_date_without_a_zone_is_refused() {
        assert!(parse("2026-09-17T00:00:00").is_err());
        assert!(parse("2026-09-17").is_err());
    }

    #[test]
    fn nonsense_is_refused_with_the_form_it_wanted() {
        for text in [
            "",
            "yesterday",
            "2026-13-01T00:00:00Z",
            "2026-09-17T24:00:00Z",
            "2026-09-17T00:60:00Z",
            "2026/09/17T00:00:00Z",
            "2026-09-17T00:00:00+0100",
            "2026-09-17T00:00:00.Z",
        ] {
            let result = parse(text);
            assert!(result.is_err(), "{text:?} was accepted as {result:?}");
        }
        let message = parse("yesterday").expect_err("an error");
        assert!(message.contains("RFC 3339"), "{message}");
    }
}
