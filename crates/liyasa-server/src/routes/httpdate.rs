//! `Last-Modified` and `If-Modified-Since` (RX-13).
//!
//! Only the IMF-fixdate form is produced, and the two obsolete forms RFC 9110
//! still requires a recipient to accept are parsed.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Civil date from a day number since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn format(time: SystemTime) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let weekday = (days + 3).rem_euclid(7) as usize;
    format!(
        "{}, {day:02} {} {year:04} {:02}:{:02}:{:02} GMT",
        DAYS[weekday],
        MONTHS[(month - 1) as usize],
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60,
    )
}

pub fn parse(text: &str) -> Option<SystemTime> {
    // The comma after the weekday is the only punctuation any of the three
    // forms carries, so dropping it leaves plain tokens.
    let flattened = text.replace(',', " ");
    let mut tokens: Vec<&str> = flattened.split_whitespace().collect();
    if tokens.first().is_some_and(|first| is_weekday(first)) {
        tokens.remove(0);
    }
    match tokens.as_slice() {
        // `06-Nov-94 08:49:37 GMT`, the obsolete RFC 850 form.
        [date, clock, ..] if date.contains('-') => {
            let mut parts = date.split('-');
            let day = parts.next()?.parse().ok()?;
            let month = month_index(parts.next()?)?;
            let year: i64 = parts.next()?.parse().ok()?;
            let year = if year < 70 { year + 2000 } else { year + 1900 };
            finish(year, month, day, clock)
        }
        // `Nov  6 08:49:37 1994`, the asctime form.
        [month, day, clock, year, ..] if month_index(month).is_some() => finish(
            year.parse().ok()?,
            month_index(month)?,
            day.parse().ok()?,
            clock,
        ),
        // `06 Nov 1994 08:49:37 GMT`, the one Liyasa sends.
        [day, month, year, clock, ..] => finish(
            year.parse().ok()?,
            month_index(month)?,
            day.parse().ok()?,
            clock,
        ),
        _ => None,
    }
}

fn is_weekday(token: &str) -> bool {
    token
        .get(..3)
        .is_some_and(|prefix| DAYS.iter().any(|day| day.eq_ignore_ascii_case(prefix)))
}

fn finish(year: i64, month: u32, day: u32, clock: &str) -> Option<SystemTime> {
    let mut parts = clock.split(':');
    let hour: u64 = parts.next()?.parse().ok()?;
    let minute: u64 = parts.next()?.parse().ok()?;
    let second: u64 = parts.next()?.parse().ok()?;
    if month == 0 || month > 12 || day == 0 || day > 31 || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let total = days * 86_400 + (hour * 3600 + minute * 60 + second) as i64;
    if total < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(total as u64))
}

fn month_index(name: &str) -> Option<u32> {
    MONTHS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(name))
        .map(|i| i as u32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn the_epoch_and_a_known_date_format_correctly() {
        assert_eq!(format(at(0)), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(format(at(784_111_777)), "Sun, 06 Nov 1994 08:49:37 GMT");
        assert_eq!(format(at(1_789_473_600)), "Tue, 15 Sep 2026 12:00:00 GMT");
    }

    #[test]
    fn every_form_a_recipient_must_accept_parses_to_the_same_instant() {
        let expected = at(784_111_777);
        for text in [
            "Sun, 06 Nov 1994 08:49:37 GMT",
            "Sunday, 06-Nov-94 08:49:37 GMT",
            "Sun Nov  6 08:49:37 1994",
        ] {
            assert_eq!(parse(text), Some(expected), "{text}");
        }
    }

    #[test]
    fn a_formatted_date_parses_back() {
        for secs in [0, 1, 951_782_400, 1_789_473_600, 2_000_000_000] {
            assert_eq!(parse(&format(at(secs))), Some(at(secs)), "{secs}");
        }
    }

    #[test]
    fn nonsense_is_refused_rather_than_guessed_at() {
        for text in [
            "",
            "yesterday",
            "Sun, 06 Foo 1994 08:49:37 GMT",
            "Sun, 06 Nov 1994",
        ] {
            assert!(parse(text).is_none(), "{text}");
        }
    }
}
