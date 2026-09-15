//! RSS, Atom, and JSON feeds (RX-83).
//!
//! Timestamps come from the author's `updated` or `date` when it starts with a
//! calendar day, and from the build clock otherwise
//! (`plan/rfcs/1004-feed-timestamps.md`); everything is UTC, because a feed a
//! machine reads in another zone must not depend on the builder's.

use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use liyasa_core::build::BuildClock;
use serde_json::json;

use crate::agents::resource::{self, Resource, Surfaces};
use crate::agents::site::{PageRecord, SiteInput};

pub const CHANGELOG_RSS: &str = "/changelog.xml";
pub const CHANGELOG_ATOM: &str = "/changelog.atom";
pub const CHANGELOG_JSON: &str = "/changelog.json";
pub const UPDATES_RSS: &str = "/updates.xml";
pub const UPDATES_ATOM: &str = "/updates.atom";
pub const UPDATES_JSON: &str = "/updates.json";

/// How many entries a feed carries. Readers poll; a feed is a window, not an
/// archive, and the archive is the site.
const MAX_ENTRIES: usize = 50;

/// Generates whichever feeds `feeds.changelog` and `feeds.updates` ask for.
pub fn generate(site: &SiteInput, clock: BuildClock) -> Surfaces {
    let mut out = Surfaces::default();
    if site.feeds.changelog {
        let entries = entries(site, clock, |page| page.changelog);
        if !entries.is_empty() {
            let feed = Feed {
                title: format!("{} changelog", site.name),
                description: format!("Changes to {}.", site.name),
                paths: (CHANGELOG_RSS, CHANGELOG_ATOM, CHANGELOG_JSON),
            };
            feed.emit(site, &entries, clock, &mut out);
        }
    }
    if site.feeds.updates {
        let entries = entries(site, clock, |_| true);
        if !entries.is_empty() {
            let feed = Feed {
                title: format!("{} updates", site.name),
                description: format!("Every page update on {}.", site.name),
                paths: (UPDATES_RSS, UPDATES_ATOM, UPDATES_JSON),
            };
            feed.emit(site, &entries, clock, &mut out);
        }
    }
    out
}

struct Feed {
    title: String,
    description: String,
    paths: (&'static str, &'static str, &'static str),
}

impl Feed {
    fn emit(&self, site: &SiteInput, entries: &[Entry<'_>], clock: BuildClock, out: &mut Surfaces) {
        let (rss, atom, json) = self.paths;
        out.resources.push(Resource::new(
            rss,
            resource::RSS,
            self.rss(site, entries, rss),
        ));
        out.resources.push(Resource::new(
            atom,
            resource::ATOM,
            self.atom(site, entries, atom, clock),
        ));
        out.resources.push(Resource::new(
            json,
            resource::FEED_JSON,
            self.json(site, entries, json),
        ));
    }

    fn rss(&self, site: &SiteInput, entries: &[Entry<'_>], path: &str) -> String {
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
        out.push_str("<rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\">\n");
        out.push_str("  <channel>\n");
        let _ = writeln!(out, "    <title>{}</title>", escape(&self.title));
        let _ = writeln!(
            out,
            "    <link>{}</link>",
            escape(&site.origin.page_url(&root_route()))
        );
        let _ = writeln!(
            out,
            "    <description>{}</description>",
            escape(&self.description)
        );
        let _ = writeln!(
            out,
            "    <language>{}</language>",
            escape(site.locale.as_str())
        );
        let _ = writeln!(
            out,
            "    <atom:link href=\"{}\" rel=\"self\" type=\"application/rss+xml\"/>",
            escape(&site.origin.resource_url(path))
        );
        for entry in entries {
            out.push_str("    <item>\n");
            let _ = writeln!(out, "      <title>{}</title>", escape(&entry.page.title));
            let _ = writeln!(out, "      <link>{}</link>", escape(&entry.html_url));
            let _ = writeln!(
                out,
                "      <guid isPermaLink=\"true\">{}</guid>",
                escape(&entry.html_url)
            );
            let _ = writeln!(out, "      <pubDate>{}</pubDate>", entry.at.rfc_822());
            let _ = writeln!(
                out,
                "      <description>{}</description>",
                escape(entry.summary())
            );
            out.push_str("    </item>\n");
        }
        out.push_str("  </channel>\n</rss>\n");
        out
    }

    fn atom(
        &self,
        site: &SiteInput,
        entries: &[Entry<'_>],
        path: &str,
        clock: BuildClock,
    ) -> String {
        let updated = entries
            .first()
            .map_or_else(|| Timestamp::of(clock), |entry| entry.at);
        let self_url = site.origin.resource_url(path);
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
        out.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">\n");
        let _ = writeln!(out, "  <title>{}</title>", escape(&self.title));
        let _ = writeln!(out, "  <subtitle>{}</subtitle>", escape(&self.description));
        let _ = writeln!(out, "  <id>{}</id>", escape(&self_url));
        let _ = writeln!(
            out,
            "  <link href=\"{}\" rel=\"self\" type=\"application/atom+xml\"/>",
            escape(&self_url)
        );
        let _ = writeln!(
            out,
            "  <link href=\"{}\" rel=\"alternate\" type=\"text/html\"/>",
            escape(&site.origin.page_url(&root_route()))
        );
        let _ = writeln!(out, "  <updated>{}</updated>", updated.rfc_3339());
        for entry in entries {
            out.push_str("  <entry>\n");
            let _ = writeln!(out, "    <title>{}</title>", escape(&entry.page.title));
            let _ = writeln!(out, "    <id>{}</id>", escape(&entry.html_url));
            let _ = writeln!(
                out,
                "    <link href=\"{}\" rel=\"alternate\" type=\"text/html\"/>",
                escape(&entry.html_url)
            );
            // The Markdown form, so an agent reading the feed never has to
            // guess the route (RX-60, RX-65).
            let _ = writeln!(
                out,
                "    <link href=\"{}\" rel=\"alternate\" type=\"text/markdown\"/>",
                escape(&entry.markdown_url)
            );
            let _ = writeln!(out, "    <updated>{}</updated>", entry.at.rfc_3339());
            let _ = writeln!(out, "    <summary>{}</summary>", escape(entry.summary()));
            out.push_str("  </entry>\n");
        }
        out.push_str("</feed>\n");
        out
    }

    fn json(&self, site: &SiteInput, entries: &[Entry<'_>], path: &str) -> String {
        let items: Vec<serde_json::Value> = entries
            .iter()
            .map(|entry| {
                json!({
                    "id": entry.html_url,
                    "url": entry.html_url,
                    "external_url": entry.markdown_url,
                    "title": entry.page.title,
                    "summary": entry.summary(),
                    "date_published": entry.at.rfc_3339(),
                    "language": site.locale.as_str(),
                })
            })
            .collect();
        let feed = json!({
            "version": "https://jsonfeed.org/version/1.1",
            "title": self.title,
            "description": self.description,
            "home_page_url": site.origin.page_url(&root_route()),
            "feed_url": site.origin.resource_url(path),
            "language": site.locale.as_str(),
            "items": items,
        });
        let mut body = serde_json::to_string_pretty(&feed).unwrap_or_else(|_| "{}".to_owned());
        body.push('\n');
        body
    }
}

struct Entry<'a> {
    page: &'a PageRecord,
    at: Timestamp,
    html_url: String,
    markdown_url: String,
}

impl Entry<'_> {
    fn summary(&self) -> &str {
        self.page.description.as_deref().unwrap_or_default()
    }
}

/// Published pages matching `select`, newest first, capped.
fn entries<'a>(
    site: &'a SiteInput,
    clock: BuildClock,
    select: impl Fn(&PageRecord) -> bool,
) -> Vec<Entry<'a>> {
    let mut out: Vec<Entry<'a>> = site
        .published()
        .filter(|page| select(page))
        .map(|page| Entry {
            page,
            at: Timestamp::for_page(page, clock),
            html_url: site.origin.page_url(&page.route),
            markdown_url: site.origin.markdown_url(&page.route),
        })
        .collect();
    // Route breaks the tie so two pages dated the same day keep a stable
    // order across builds (§6.6.2).
    out.sort_by(|a, b| {
        b.at.unix
            .cmp(&a.at.unix)
            .then_with(|| a.page.route.as_str().cmp(b.page.route.as_str()))
    });
    out.truncate(MAX_ENTRIES);
    out
}

fn root_route() -> liyasa_core::ids::Route {
    liyasa_core::ids::Route::new("/")
}

/// A UTC instant, formatted for the three feed formats without a date crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp {
    pub unix: i64,
}

impl Timestamp {
    pub fn of(clock: BuildClock) -> Self {
        let unix = clock
            .0
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_else(|error| -(error.duration().as_secs() as i64));
        Self { unix }
    }

    /// The author's `updated`, then `date`, then the build clock.
    pub fn for_page(page: &PageRecord, clock: BuildClock) -> Self {
        page.updated
            .as_deref()
            .and_then(Self::parse_day)
            .unwrap_or_else(|| Self::of(clock))
    }

    /// `YYYY-MM-DD` at the start of a string, at midnight UTC.
    pub fn parse_day(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() < 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        let year: i64 = text.get(0..4)?.parse().ok()?;
        let month: i64 = text.get(5..7)?.parse().ok()?;
        let day: i64 = text.get(8..10)?.parse().ok()?;
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }
        Some(Self {
            unix: days_from_civil(year, month, day) * 86_400,
        })
    }

    fn civil(self) -> (i64, i64, i64, i64, i64, i64) {
        let days = self.unix.div_euclid(86_400);
        let secs = self.unix.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        (year, month, day, secs / 3600, (secs / 60) % 60, secs % 60)
    }

    /// The day of the week, 0 = Thursday (1970-01-01).
    fn weekday(self) -> usize {
        (self.unix.div_euclid(86_400) + 4).rem_euclid(7) as usize
    }

    pub fn rfc_3339(self) -> String {
        let (y, mo, d, h, mi, s) = self.civil();
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
    }

    pub fn rfc_822(self) -> String {
        const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let (y, mo, d, h, mi, s) = self.civil();
        let month = MONTHS
            .get((mo - 1).clamp(0, 11) as usize)
            .copied()
            .unwrap_or("Jan");
        format!(
            "{}, {d:02} {month} {y:04} {h:02}:{mi:02}:{s:02} +0000",
            DAYS[self.weekday()]
        )
    }
}

impl From<SystemTime> for Timestamp {
    fn from(time: SystemTime) -> Self {
        Self::of(BuildClock(time))
    }
}

/// Howard Hinnant's `days_from_civil`, which is exact for every proleptic
/// Gregorian date and needs no table.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Its inverse.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month, day)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use liyasa_core::ids::{Locale, Route};

    use super::*;
    use crate::agents::site::{AgentsSettings, CanonicalOrigin, FeedsSettings, SiteInput};

    /// 2026-09-15T12:00:00Z, so a test that falls back to the clock is still
    /// reading a fixed instant (§6.6.2 rule 1).
    fn clock() -> BuildClock {
        BuildClock(UNIX_EPOCH + Duration::from_secs(1_789_473_600))
    }

    fn page(route: &str, title: &str, updated: Option<&str>, changelog: bool) -> PageRecord {
        PageRecord {
            id: None,
            route: Route::new(route),
            title: title.to_owned(),
            description: Some(format!("What changed in {title}.")),
            locale: Locale::new("en"),
            version: None,
            tab: None,
            group: None,
            indexable: true,
            personalized: false,
            markdown: format!("# {title}\n"),
            updated: updated.map(str::to_owned),
            changelog,
        }
    }

    fn site() -> SiteInput {
        SiteInput {
            name: "Liyasa".to_owned(),
            summary: Some("Documentation.".to_owned()),
            origin: CanonicalOrigin::parse("https://example.com").expect("a valid origin"),
            locale: Locale::new("en"),
            version: None,
            pages: vec![
                page(
                    "/changelog/2026-09",
                    "September 2026",
                    Some("2026-09-15"),
                    true,
                ),
                page(
                    "/changelog/2026-08",
                    "August 2026",
                    Some("2026-08-01"),
                    true,
                ),
                page("/guide/install", "Install", Some("2026-07-04"), false),
            ],
            nav: Vec::new(),
            agents: AgentsSettings::default(),
            feeds: FeedsSettings::default(),
        }
    }

    fn body(surfaces: &Surfaces, path: &str) -> String {
        surfaces
            .get(path)
            .unwrap_or_else(|| panic!("{path}"))
            .body
            .clone()
    }

    #[test]
    fn rx_83_the_changelog_ships_in_all_three_formats() {
        let surfaces = generate(&site(), clock());
        for path in [CHANGELOG_RSS, CHANGELOG_ATOM, CHANGELOG_JSON] {
            assert!(surfaces.get(path).is_some(), "{path}");
        }
    }

    #[test]
    fn rx_83_the_changelog_feed_holds_only_changelog_pages() {
        let rss = body(&generate(&site(), clock()), CHANGELOG_RSS);
        assert!(rss.contains("September 2026"), "{rss}");
        assert!(!rss.contains("Install"), "{rss}");
    }

    #[test]
    fn rx_83_updates_are_off_until_the_key_turns_them_on() {
        let surfaces = generate(&site(), clock());
        assert!(surfaces.get(UPDATES_RSS).is_none());

        let mut site = site();
        site.feeds.updates = true;
        let surfaces = generate(&site, clock());
        let rss = body(&surfaces, UPDATES_RSS);
        assert!(rss.contains("Install"), "{rss}");
        assert!(rss.contains("September 2026"), "{rss}");
    }

    #[test]
    fn rx_83_entries_are_newest_first() {
        let json: serde_json::Value =
            serde_json::from_str(&body(&generate(&site(), clock()), CHANGELOG_JSON))
                .expect("valid JSON");
        let items = json["items"].as_array().expect("items");
        assert_eq!(items[0]["title"], "September 2026");
        assert_eq!(items[1]["title"], "August 2026");
    }

    #[test]
    fn rx_83_every_entry_links_to_both_representations() {
        let atom = body(&generate(&site(), clock()), CHANGELOG_ATOM);
        assert!(
            atom.contains(
                r#"<link href="https://example.com/changelog/2026-09" rel="alternate" type="text/html"/>"#
            ),
            "{atom}"
        );
        assert!(
            atom.contains(
                r#"<link href="https://example.com/changelog/2026-09.md" rel="alternate" type="text/markdown"/>"#
            ),
            "{atom}"
        );
    }

    #[test]
    fn rx_83_a_personalized_page_never_reaches_a_feed() {
        let mut site = site();
        site.feeds.updates = true;
        let mut private = page("/dashboard", "Dashboard", Some("2026-09-15"), false);
        private.personalized = true;
        site.pages.push(private);
        assert!(!body(&generate(&site, clock()), UPDATES_RSS).contains("Dashboard"));
    }

    #[test]
    fn rx_83_timestamps_are_formatted_for_each_format() {
        let at = Timestamp::parse_day("2026-09-15").expect("a day");
        assert_eq!(at.rfc_3339(), "2026-09-15T00:00:00Z");
        assert_eq!(at.rfc_822(), "Tue, 15 Sep 2026 00:00:00 +0000");
    }

    #[test]
    fn rx_83_a_date_the_author_invented_falls_back_to_the_build_clock() {
        assert!(Timestamp::parse_day("Spring 2026").is_none());
        let page = page("/guide/install", "Install", Some("Spring 2026"), true);
        assert_eq!(Timestamp::for_page(&page, clock()), Timestamp::of(clock()));
    }

    #[test]
    fn rx_83_the_build_clock_round_trips() {
        assert_eq!(Timestamp::of(clock()).rfc_3339(), "2026-09-15T12:00:00Z");
    }

    #[test]
    fn rx_83_the_calendar_arithmetic_holds_at_the_hard_dates() {
        for (text, rfc_3339, weekday) in [
            ("1970-01-01", "1970-01-01T00:00:00Z", "Thu"),
            ("2000-02-29", "2000-02-29T00:00:00Z", "Tue"),
            ("2024-02-29", "2024-02-29T00:00:00Z", "Thu"),
            ("2100-03-01", "2100-03-01T00:00:00Z", "Mon"),
            ("1969-12-31", "1969-12-31T00:00:00Z", "Wed"),
        ] {
            let at = Timestamp::parse_day(text).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(at.rfc_3339(), rfc_3339);
            assert!(at.rfc_822().starts_with(weekday), "{}", at.rfc_822());
        }
        assert!(Timestamp::parse_day("2026-13-01").is_none());
        assert!(Timestamp::parse_day("2026-09").is_none());
    }

    #[test]
    fn rx_83_markup_in_a_title_is_escaped() {
        let mut site = site();
        site.pages[0].title = "A <b> & \"quoted\" release".to_owned();
        let rss = body(&generate(&site, clock()), CHANGELOG_RSS);
        assert!(
            rss.contains("A &lt;b&gt; &amp; &quot;quoted&quot; release"),
            "{rss}"
        );
        assert!(!rss.contains("<b>"), "{rss}");
    }

    #[test]
    fn rx_83_a_site_with_no_changelog_publishes_no_changelog_feed() {
        let mut site = site();
        for page in &mut site.pages {
            page.changelog = false;
        }
        assert!(generate(&site, clock()).resources.is_empty());
    }
}
