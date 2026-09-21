//! `sitemap.xml` (PRD §7.9 CM-80, RX-03).
//!
//! One entry per public route. A hidden page is routable and not in here; a
//! page that sets `noindex: false` on top of `hidden: true` is (CM-80).

use liyasa_core::ids::Route;

use crate::agents::feeds::Timestamp;

pub const FILE: &str = "sitemap.xml";

/// One route as the sitemap sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub route: Route,
    /// `updated` front matter, when the author wrote one.
    pub updated: Option<String>,
}

/// Renders the sitemap. `origin` is the canonical origin with no trailing
/// slash; `clock_unix` dates a page that carries no date of its own, so two
/// builds of the same content agree (§6.6.2 rule 1).
pub fn render(entries: &[Entry], origin: &str, clock_unix: i64) -> String {
    render_with_spans(entries, origin, clock_unix).0
}

/// The sitemap, and which route occupies which bytes of it, so the server can
/// drop the routes a reader may not see (AUTH-10). The spans are recorded as
/// the XML is written; recovering them afterwards would mean parsing this
/// function's own output back.
pub fn render_with_spans(
    entries: &[Entry],
    origin: &str,
    clock_unix: i64,
) -> (String, Vec<crate::manifest::ListingEntry>) {
    let origin = origin.trim_end_matches('/');
    let fallback = Timestamp { unix: clock_unix }.rfc_3339();
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    let mut spans = Vec::new();
    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.route.cmp(&b.route));
    for entry in sorted {
        let last_modified = entry
            .updated
            .as_deref()
            .and_then(Timestamp::parse_day)
            .map(Timestamp::rfc_3339)
            .unwrap_or_else(|| fallback.clone());
        let start = out.len();
        out.push_str("  <url>\n");
        out.push_str(&format!(
            "    <loc>{}{}</loc>\n",
            escape(origin),
            escape(entry.route.as_str())
        ));
        out.push_str(&format!("    <lastmod>{last_modified}</lastmod>\n"));
        out.push_str("  </url>\n");
        spans.push(crate::manifest::ListingEntry::page(
            entry.route.as_str(),
            None,
            start,
            out.len(),
        ));
    }
    out.push_str("</urlset>\n");
    (out, spans)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(route: &str, updated: Option<&str>) -> Entry {
        Entry {
            route: Route::new(route),
            updated: updated.map(str::to_owned),
        }
    }

    #[test]
    fn every_entry_is_an_absolute_url_in_route_order() {
        let xml = render(
            &[entry("/guides/install", None), entry("/", None)],
            "https://docs.acme.com/",
            1_789_473_600,
        );
        let root = xml
            .find("<loc>https://docs.acme.com/</loc>")
            .expect("the root");
        let guide = xml
            .find("<loc>https://docs.acme.com/guides/install</loc>")
            .expect("the guide");
        assert!(root < guide, "sorted by route");
    }

    #[test]
    fn a_page_with_no_date_is_dated_from_the_build_clock() {
        let xml = render(&[entry("/", None)], "https://docs.acme.com", 0);
        assert!(
            xml.contains("<lastmod>1970-01-01T00:00:00Z</lastmod>"),
            "{xml}"
        );
    }

    #[test]
    fn the_authors_date_wins_when_there_is_one() {
        let xml = render(
            &[entry("/", Some("2026-09-01"))],
            "https://docs.acme.com",
            0,
        );
        assert!(
            xml.contains("<lastmod>2026-09-01T00:00:00Z</lastmod>"),
            "{xml}"
        );
    }

    #[test]
    fn an_empty_sitemap_is_still_a_sitemap() {
        let xml = render(&[], "https://docs.acme.com", 0);
        assert!(xml.starts_with("<?xml"));
        assert!(xml.contains("<urlset"));
        assert!(!xml.contains("<url>"));
    }
}
