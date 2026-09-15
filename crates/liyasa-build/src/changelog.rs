//! Changelogs (PRD §7.13, CM-120, CM-121, CM-123).
//!
//! Two shapes, one stream: a page whose body holds `::update` entries, and a
//! directory of dated files. Each entry keeps the anchor the `update`
//! component renders, so a link into the changelog survives a rebuild, and the
//! stream feeds the RSS, Atom, and JSON feeds of CM-121.

use std::collections::BTreeSet;

use liyasa_core::document::{Block, BlockKind, Document, Node, PropValue};
use liyasa_core::ids::Route;

use crate::agents::feeds::Timestamp;

/// Where a changelog directory lives (CM-120).
pub const DIRECTORY: &str = "changelog";

pub const RSS_PATH: &str = "changelog/rss.xml";
pub const ATOM_PATH: &str = "changelog/atom.xml";
pub const JSON_PATH: &str = "changelog/feed.json";

/// One entry of the stream.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    /// `YYYY-MM-DD` as the author wrote it.
    pub date: String,
    pub version: Option<String>,
    pub labels: Vec<String>,
    pub title: String,
    /// The `id` the `update` component renders, so `#anchor` keeps working.
    pub anchor: String,
    /// The page the entry is on.
    pub route: Route,
    /// Plain text of the entry's body, which is what a feed carries.
    pub summary: String,
}

impl Entry {
    pub fn href(&self) -> String {
        format!("{}#{}", self.route.as_str(), self.anchor)
    }

    fn sort_key(&self) -> (i64, String) {
        let unix = Timestamp::parse_day(&self.date)
            .map(|stamp| stamp.unix)
            .unwrap_or_default();
        (-unix, self.title.clone())
    }
}

/// Entries written as `::update` blocks in a page's body (CM-120).
pub fn from_page(route: &Route, document: &Document) -> Vec<Entry> {
    let mut out = Vec::new();
    collect(&document.root, route, &mut out);
    out
}

fn collect(block: &Block, route: &Route, out: &mut Vec<Entry>) {
    if let BlockKind::Component { name, props, .. } = &block.kind
        && name == "update"
    {
        let date = string(props.get("date")).unwrap_or_default();
        let version = string(props.get("version"));
        let title = match (string(props.get("title")), version.clone()) {
            (Some(title), Some(version)) => format!("{version} — {title}"),
            (Some(title), None) => title,
            (None, Some(version)) => version,
            (None, None) => date.clone(),
        };
        out.push(Entry {
            // The component renders `slug("<date> <heading>")`; the anchor has
            // to be the same string or a link into the page misses.
            anchor: liyasa_components::anchor::slug(&format!("{date} {title}")),
            labels: list(props.get("labels")),
            summary: liyasa_components::text::of(&block.children),
            route: route.clone(),
            version: string(props.get("version")),
            title,
            date,
        });
    }
    for child in &block.children {
        if let Node::Block(child) = child {
            collect(child, route, out);
        }
    }
}

/// An entry written as its own dated file, `changelog/2026-09-01-title.md`
/// (CM-120).
pub fn from_file(
    path: &str,
    route: &Route,
    title: Option<&str>,
    labels: &[String],
    version: Option<&str>,
    summary: &str,
) -> Option<Entry> {
    let name = path.rsplit('/').next()?;
    let stem = name
        .strip_suffix(".md")
        .or_else(|| name.strip_suffix(".mdx"))?;
    let (date, rest) = split_dated(stem)?;
    let title = title.map(str::to_owned).unwrap_or_else(|| humanize(rest));
    Some(Entry {
        anchor: liyasa_components::anchor::slug(&format!("{date} {title}")),
        date,
        version: version.map(str::to_owned),
        labels: labels.to_vec(),
        title,
        route: route.clone(),
        summary: summary.to_owned(),
    })
}

/// Whether a path is one of the dated files a changelog directory holds.
pub fn is_entry_file(path: &str) -> bool {
    let Some(rest) = path.strip_prefix(&format!("{DIRECTORY}/")) else {
        return false;
    };
    let stem = rest
        .strip_suffix(".md")
        .or_else(|| rest.strip_suffix(".mdx"))
        .unwrap_or(rest);
    split_dated(stem).is_some()
}

/// The stream: newest first, ties broken by title so two entries on one day
/// keep one order (§6.6.2 rule 5).
pub fn stream(mut entries: Vec<Entry>) -> Vec<Entry> {
    entries.sort_by_key(Entry::sort_key);
    entries.dedup_by(|a, b| a.anchor == b.anchor && a.route == b.route);
    entries
}

/// Every label in the stream, which is what the UI filters on (CM-121).
pub fn labels(entries: &[Entry]) -> BTreeSet<String> {
    entries
        .iter()
        .flat_map(|entry| entry.labels.iter().cloned())
        .collect()
}

/// `/changelog/rss.xml` (CM-121).
pub fn rss(entries: &[Entry], site_name: &str, origin: &str) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<rss version=\"2.0\">\n  <channel>\n");
    out.push_str(&format!(
        "    <title>{} changelog</title>\n",
        escape(site_name)
    ));
    out.push_str(&format!("    <link>{}</link>\n", escape(origin)));
    out.push_str(&format!(
        "    <description>Changes to {}.</description>\n",
        escape(site_name)
    ));
    for entry in entries {
        out.push_str("    <item>\n");
        out.push_str(&format!("      <title>{}</title>\n", escape(&entry.title)));
        out.push_str(&format!(
            "      <link>{}{}</link>\n",
            escape(origin),
            escape(&entry.href())
        ));
        out.push_str(&format!(
            "      <guid isPermaLink=\"false\">{}{}</guid>\n",
            escape(origin),
            escape(&entry.href())
        ));
        if let Some(stamp) = Timestamp::parse_day(&entry.date) {
            out.push_str(&format!("      <pubDate>{}</pubDate>\n", stamp.rfc_822()));
        }
        for label in &entry.labels {
            out.push_str(&format!("      <category>{}</category>\n", escape(label)));
        }
        out.push_str(&format!(
            "      <description>{}</description>\n",
            escape(&entry.summary)
        ));
        out.push_str("    </item>\n");
    }
    out.push_str("  </channel>\n</rss>\n");
    out
}

/// `/changelog/atom.xml` (CM-121).
pub fn atom(entries: &[Entry], site_name: &str, origin: &str) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">\n");
    out.push_str(&format!(
        "  <title>{} changelog</title>\n",
        escape(site_name)
    ));
    out.push_str(&format!("  <id>{}/changelog</id>\n", escape(origin)));
    out.push_str(&format!(
        "  <link rel=\"self\" href=\"{}/{RSS_PATH}\"/>\n",
        escape(origin)
    ));
    for entry in entries {
        out.push_str("  <entry>\n");
        out.push_str(&format!("    <title>{}</title>\n", escape(&entry.title)));
        out.push_str(&format!(
            "    <id>{}{}</id>\n",
            escape(origin),
            escape(&entry.href())
        ));
        out.push_str(&format!(
            "    <link href=\"{}{}\"/>\n",
            escape(origin),
            escape(&entry.href())
        ));
        if let Some(stamp) = Timestamp::parse_day(&entry.date) {
            out.push_str(&format!("    <updated>{}</updated>\n", stamp.rfc_3339()));
        }
        for label in &entry.labels {
            out.push_str(&format!("    <category term=\"{}\"/>\n", escape(label)));
        }
        out.push_str(&format!(
            "    <summary>{}</summary>\n",
            escape(&entry.summary)
        ));
        out.push_str("  </entry>\n");
    }
    out.push_str("</feed>\n");
    out
}

/// `/changelog/feed.json`, JSON Feed 1.1 (CM-121).
pub fn json_feed(entries: &[Entry], site_name: &str, origin: &str) -> String {
    let items: Vec<serde_json::Value> = entries
        .iter()
        .map(|entry| {
            serde_json::json!({
                "id": format!("{origin}{}", entry.href()),
                "url": format!("{origin}{}", entry.href()),
                "title": entry.title,
                "content_text": entry.summary,
                "date_published": Timestamp::parse_day(&entry.date)
                    .map(|stamp| stamp.rfc_3339()),
                "tags": entry.labels,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "version": "https://jsonfeed.org/version/1.1",
        "title": format!("{site_name} changelog"),
        "home_page_url": origin,
        "feed_url": format!("{origin}/{JSON_PATH}"),
        "items": items,
    }))
    .unwrap_or_else(|_| "{}".to_owned())
}

/// The stream a directory of dated files renders as (CM-120).
///
/// Each entry links to its own page; the body stays there, so one entry is one
/// route and the stream is the index of them.
pub fn stream_html(entries: &[Entry]) -> String {
    let mut out = String::from("<div class=\"ly-changelog\" data-liyasa=\"changelog\">\n");
    for entry in entries {
        out.push_str(&format!(
            "<article class=\"ly-update\" data-liyasa=\"update\" id=\"{}\" data-date=\"{}\"",
            escape(&entry.anchor),
            escape(&entry.date)
        ));
        if let Some(version) = &entry.version {
            out.push_str(&format!(" data-version=\"{}\"", escape(version)));
        }
        if !entry.labels.is_empty() {
            out.push_str(&format!(
                " data-labels=\"{}\"",
                escape(&entry.labels.join(","))
            ));
        }
        out.push_str(">\n");
        out.push_str(&format!(
            "<time datetime=\"{}\">{}</time>\n",
            escape(&entry.date),
            escape(&entry.date)
        ));
        out.push_str(&format!(
            "<h2><a href=\"{}\">{}</a></h2>\n",
            escape(&entry.href()),
            escape(&entry.title)
        ));
        if !entry.summary.trim().is_empty() {
            out.push_str(&format!("<p>{}</p>\n", escape(entry.summary.trim())));
        }
        out.push_str("</article>\n");
    }
    out.push_str("</div>\n");
    out
}

/// CM-123's half that does not need a repository: a merged pull request turned
/// into the entry an author reviews.
///
/// Connecting to a repository is `liyasa-git`'s and the agent's; what belongs
/// here is the shape the proposal takes, so both produce the same Markdown.
pub fn draft_entry(date: &str, title: &str, labels: &[String], body: &str) -> String {
    let labels = match labels.is_empty() {
        true => String::new(),
        false => format!(" labels=\"{}\"", labels.join(",")),
    };
    let body = body.trim();
    format!("::update{{date=\"{date}\" title=\"{title}\"{labels}}}\n{body}\n::\n")
}

fn split_dated(stem: &str) -> Option<(String, &str)> {
    let bytes = stem.as_bytes();
    if bytes.len() < 11 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'-' {
        return None;
    }
    let date = stem.get(0..10)?;
    Timestamp::parse_day(date)?;
    Some((date.to_owned(), stem.get(11..)?))
}

fn humanize(slug: &str) -> String {
    let spaced = slug.replace(['-', '_'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => spaced,
    }
}

fn string(value: Option<&PropValue>) -> Option<String> {
    match value? {
        PropValue::Str(text) => Some(text.clone()),
        PropValue::Num(number) => Some(number.to_string()),
        _ => None,
    }
}

fn list(value: Option<&PropValue>) -> Vec<String> {
    match value {
        Some(PropValue::List(items)) => {
            items.iter().filter_map(|item| string(Some(item))).collect()
        }
        // `labels="api,billing"` is one string of comma-separated labels.
        Some(PropValue::Str(text)) => text
            .split(',')
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(date: &str, title: &str, labels: &[&str]) -> Entry {
        Entry {
            date: date.to_owned(),
            version: None,
            labels: labels.iter().map(|l| (*l).to_owned()).collect(),
            title: title.to_owned(),
            anchor: liyasa_components::anchor::slug(&format!("{date} {title}")),
            route: Route::new("/changelog"),
            summary: format!("What changed on {date}."),
        }
    }

    #[test]
    fn the_stream_is_newest_first() {
        let stream = stream(vec![
            entry("2026-01-01", "Older", &[]),
            entry("2026-09-01", "Newer", &[]),
            entry("2026-05-01", "Middle", &[]),
        ]);
        let titles: Vec<&str> = stream.iter().map(|entry| entry.title.as_str()).collect();
        assert_eq!(titles, ["Newer", "Middle", "Older"]);
    }

    #[test]
    fn an_entry_keeps_the_anchor_the_component_renders() {
        let entry = entry("2026-09-01", "2.3 — Billing", &["api"]);
        assert_eq!(entry.anchor, "2026-09-01-2-3-billing");
        assert_eq!(entry.href(), "/changelog#2026-09-01-2-3-billing");
    }

    #[test]
    fn a_dated_file_is_an_entry() {
        assert!(is_entry_file("changelog/2026-09-01-billing.md"));
        assert!(!is_entry_file("changelog/index.md"));
        assert!(!is_entry_file("guides/2026-09-01-billing.md"));

        let entry = from_file(
            "changelog/2026-09-01-billing-api.md",
            &Route::new("/changelog/2026-09-01-billing-api"),
            None,
            &["api".to_owned()],
            Some("2.3"),
            "Billing endpoints moved.",
        )
        .expect("a dated file is an entry");
        assert_eq!(entry.date, "2026-09-01");
        assert_eq!(entry.title, "Billing api");
        assert_eq!(entry.version.as_deref(), Some("2.3"));
    }

    #[test]
    fn a_file_with_no_date_is_not_an_entry() {
        assert!(
            from_file(
                "changelog/notes.md",
                &Route::new("/changelog/notes"),
                None,
                &[],
                None,
                ""
            )
            .is_none()
        );
    }

    #[test]
    fn labels_are_collected_for_the_filter_ui() {
        let entries = vec![
            entry("2026-09-01", "One", &["api", "billing"]),
            entry("2026-08-01", "Two", &["api"]),
        ];
        let labels = labels(&entries);
        assert_eq!(
            labels.into_iter().collect::<Vec<_>>(),
            ["api".to_owned(), "billing".to_owned()]
        );
    }

    #[test]
    fn the_rss_feed_carries_every_entry_with_a_link_and_a_date() {
        let entries = stream(vec![entry("2026-09-01", "Billing", &["api"])]);
        let rss = rss(&entries, "Acme docs", "https://docs.acme.com");
        assert!(rss.contains("<title>Acme docs changelog</title>"), "{rss}");
        assert!(
            rss.contains("https://docs.acme.com/changelog#2026-09-01-billing"),
            "{rss}"
        );
        assert!(rss.contains("<category>api</category>"), "{rss}");
        assert!(rss.contains("<pubDate>"), "{rss}");
    }

    #[test]
    fn the_atom_feed_is_well_formed_enough_to_name_its_entries() {
        let entries = stream(vec![entry("2026-09-01", "Billing", &[])]);
        let atom = atom(&entries, "Acme docs", "https://docs.acme.com");
        assert!(atom.starts_with("<?xml"), "{atom}");
        assert!(atom.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\">"));
        assert!(
            atom.contains("<updated>2026-09-01T00:00:00Z</updated>"),
            "{atom}"
        );
    }

    #[test]
    fn the_json_feed_is_json_feed() {
        let entries = stream(vec![entry("2026-09-01", "Billing", &["api"])]);
        let feed: serde_json::Value =
            serde_json::from_str(&json_feed(&entries, "Acme docs", "https://docs.acme.com"))
                .expect("the feed is JSON");
        assert_eq!(feed["version"], "https://jsonfeed.org/version/1.1");
        assert_eq!(feed["items"][0]["tags"][0], "api");
        assert_eq!(
            feed["items"][0]["url"],
            "https://docs.acme.com/changelog#2026-09-01-billing"
        );
    }

    #[test]
    fn xml_special_characters_are_escaped() {
        let mut entry = entry("2026-09-01", "Fixed <script> & \"quotes\"", &[]);
        entry.summary = "A & B".to_owned();
        let rss = rss(&[entry], "Acme & Co", "https://docs.acme.com");
        assert!(!rss.contains("<script>"), "{rss}");
        assert!(rss.contains("&amp;"), "{rss}");
    }

    #[test]
    fn the_stream_page_lists_every_entry_with_its_labels() {
        let entries = stream(vec![
            entry("2026-09-01", "Billing", &["api", "billing"]),
            entry("2026-08-01", "Usage", &[]),
        ]);
        let html = stream_html(&entries);
        assert!(html.contains("id=\"2026-09-01-billing\""), "{html}");
        assert!(html.contains("data-labels=\"api,billing\""), "{html}");
        let first = html.find("2026-09-01").expect("the newer entry");
        let second = html.find("2026-08-01").expect("the older entry");
        assert!(first < second, "newest first");
    }

    #[test]
    fn a_drafted_entry_is_the_directive_an_author_reviews() {
        let drafted = draft_entry(
            "2026-09-01",
            "Billing endpoints moved",
            &["api".to_owned(), "billing".to_owned()],
            "The `/v1/billing` endpoints are now `/v2/billing`.\n",
        );
        assert!(
            drafted.starts_with("::update{date=\"2026-09-01\""),
            "{drafted}"
        );
        assert!(drafted.contains("labels=\"api,billing\""), "{drafted}");
        assert!(drafted.ends_with("::\n"), "{drafted}");
    }
}
