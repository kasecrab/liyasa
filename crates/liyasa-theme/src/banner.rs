//! The site-wide banner (CFG-70).
//!
//! Which banner a reader sees depends on their locale and on the build clock,
//! never on the wall clock (§6.6.2 rule 1): a build is reproducible, so a
//! banner that has expired is absent from the output rather than hidden by a
//! script.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use liyasa_core::ids::Fingerprint;
use serde::{Deserialize, Serialize};

/// The `banner` block of `liyasa.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BannerConfig {
    /// Markdown; the build renders it before it reaches a template.
    pub content: String,
    pub dismissible: bool,
    /// What a dismissal is remembered against. Changing it shows the banner
    /// again; when it is absent the content is its own identity.
    pub id: Option<String>,
    pub by_locale: BTreeMap<String, String>,
    /// `YYYY-MM-DD` or an RFC 3339 timestamp.
    pub start: Option<String>,
    pub end: Option<String>,
}

/// The banner that applies, with its Markdown still unrendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub id: String,
    pub content: String,
    pub dismissible: bool,
}

impl BannerConfig {
    /// The banner for this locale and this build, if any.
    pub fn resolve(&self, locale: &str, now: SystemTime) -> Option<Resolved> {
        let content = self.content_for(locale)?;
        if !self.is_live(now) {
            return None;
        }
        Some(Resolved {
            id: self.identity(&content),
            content,
            dismissible: self.dismissible,
        })
    }

    /// The exact locale first, then its language, then the default content.
    fn content_for(&self, locale: &str) -> Option<String> {
        let language = locale.split(['-', '_']).next().unwrap_or(locale);
        let text = self
            .by_locale
            .get(locale)
            .or_else(|| self.by_locale.get(language))
            .cloned()
            .unwrap_or_else(|| self.content.clone());
        (!text.trim().is_empty()).then_some(text)
    }

    fn is_live(&self, now: SystemTime) -> bool {
        let after_start = match self.start.as_deref().and_then(parse_time) {
            Some(start) => now >= start,
            None => true,
        };
        let before_end = match self.end.as_deref().and_then(parse_time) {
            Some(end) => now < end,
            None => true,
        };
        after_start && before_end
    }

    /// A stable dismissal key. Without an explicit `id`, the content is the
    /// identity, so editing the text shows it again — which is what an operator
    /// who edited it wanted.
    fn identity(&self, content: &str) -> String {
        match &self.id {
            Some(id) if !id.is_empty() => id.clone(),
            _ => Fingerprint::of(content.as_bytes()).to_hex()[..12].to_owned(),
        }
    }

    /// Dates that are not a date at all (CFG-70 gives no format beyond the
    /// example), so an operator can be told rather than silently ignored.
    pub fn unparseable_dates(&self) -> Vec<&str> {
        [self.start.as_deref(), self.end.as_deref()]
            .into_iter()
            .flatten()
            .filter(|text| parse_time(text).is_none())
            .collect()
    }
}

/// `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SS[Z]`, in UTC.
///
/// No date crate is in the dependency table (§6.2.1) and a banner window needs
/// no calendar arithmetic beyond this, so the conversion is done here.
fn parse_time(text: &str) -> Option<SystemTime> {
    let text = text.trim();
    let (date, time) = match text.split_once(['T', ' ']) {
        Some((date, time)) => (date, time.trim_end_matches('Z')),
        None => (text, ""),
    };
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let mut seconds = days_from_civil(year, month, day) * 86_400;
    if !time.is_empty() {
        let mut clock = time.split(':');
        let hours: i64 = clock.next()?.parse().ok()?;
        let minutes: i64 = clock.next().unwrap_or("0").parse().ok()?;
        let whole_seconds: i64 = clock
            .next()
            .unwrap_or("0")
            .split('.')
            .next()
            .unwrap_or("0")
            .parse()
            .ok()?;
        if hours > 23 || minutes > 59 || whole_seconds > 60 {
            return None;
        }
        seconds += hours * 3600 + minutes * 60 + whole_seconds;
    }

    if seconds >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_secs(seconds as u64))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_secs(seconds.unsigned_abs()))
    }
}

/// Howard Hinnant's civil-from-days, inverted: days since 1970-01-01.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> SystemTime {
        parse_time(text).expect("the fixture parses")
    }

    fn config() -> BannerConfig {
        BannerConfig {
            content: "Liyasa 1.0 is out".to_owned(),
            dismissible: true,
            id: Some("launch".to_owned()),
            by_locale: BTreeMap::from([("de".to_owned(), "Liyasa 1.0 ist da".to_owned())]),
            start: None,
            end: None,
        }
    }

    #[test]
    fn dates_convert_to_the_epoch_the_way_a_calendar_does() {
        assert_eq!(at("1970-01-01"), UNIX_EPOCH);
        assert_eq!(
            at("2026-09-15"),
            UNIX_EPOCH + Duration::from_secs(1_789_430_400)
        );
        assert_eq!(
            at("2026-09-15T12:30:00Z"),
            at("2026-09-15") + Duration::from_secs(12 * 3600 + 1800)
        );
        // A leap day is a real day.
        assert_eq!(
            at("2024-03-01") - Duration::from_secs(86_400),
            at("2024-02-29")
        );
    }

    #[test]
    fn a_date_that_is_not_one_is_reported_rather_than_assumed() {
        assert!(parse_time("soon").is_none());
        assert!(parse_time("2026-13-01").is_none());
        assert!(parse_time("2026-09").is_none());
        let config = BannerConfig {
            start: Some("soon".to_owned()),
            end: Some("2026-09-15".to_owned()),
            ..config()
        };
        assert_eq!(config.unparseable_dates(), vec!["soon"]);
    }

    #[test]
    fn the_locale_chooses_the_text() {
        let config = config();
        assert_eq!(
            config.resolve("de", UNIX_EPOCH).map(|b| b.content),
            Some("Liyasa 1.0 ist da".to_owned())
        );
        assert_eq!(
            config.resolve("de-AT", UNIX_EPOCH).map(|b| b.content),
            Some("Liyasa 1.0 ist da".to_owned()),
            "a region falls back to its language"
        );
        assert_eq!(
            config.resolve("fr", UNIX_EPOCH).map(|b| b.content),
            Some("Liyasa 1.0 is out".to_owned())
        );
    }

    #[test]
    fn a_window_decides_whether_the_banner_is_built_at_all() {
        let config = BannerConfig {
            start: Some("2026-09-01".to_owned()),
            end: Some("2026-10-01".to_owned()),
            ..config()
        };
        assert!(config.resolve("en", at("2026-09-15")).is_some());
        assert!(config.resolve("en", at("2026-08-31")).is_none());
        assert!(
            config.resolve("en", at("2026-10-01")).is_none(),
            "the end is exclusive"
        );
    }

    #[test]
    fn a_changed_id_re_shows_a_dismissed_banner() {
        let first = config().resolve("en", UNIX_EPOCH).expect("a banner");
        let second = BannerConfig {
            id: Some("relaunch".to_owned()),
            ..config()
        }
        .resolve("en", UNIX_EPOCH)
        .expect("a banner");
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn without_an_id_the_content_is_the_identity() {
        let config = BannerConfig {
            id: None,
            ..config()
        };
        let first = config.resolve("en", UNIX_EPOCH).expect("a banner");
        let edited = BannerConfig {
            content: "Liyasa 1.1 is out".to_owned(),
            id: None,
            ..config.clone()
        }
        .resolve("en", UNIX_EPOCH)
        .expect("a banner");
        assert_ne!(first.id, edited.id);
        assert_eq!(
            first.id,
            config.resolve("en", UNIX_EPOCH).expect("again").id
        );
    }

    #[test]
    fn an_empty_banner_is_no_banner() {
        let config = BannerConfig {
            content: "   ".to_owned(),
            by_locale: BTreeMap::new(),
            ..config()
        };
        assert!(config.resolve("en", UNIX_EPOCH).is_none());
    }
}
