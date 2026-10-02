//! Search analytics events (SRC-08, §26.3).
//!
//! The types carry no reader identity — not a subject, not a session, not an
//! address — so there is nothing for the ingest pipeline to have to strip. The
//! one field a reader authors is the query itself, and [`scrub`] masks the
//! shapes people paste into a search box by accident.

use serde::{Deserialize, Serialize};

use crate::idx::query::Filters;
use crate::idx::search::Hit;

/// What §26.3 counts. `ANA-20` aggregates these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum SearchEvent {
    /// A query that returned something.
    Query {
        query: String,
        results: usize,
        locale: Option<String>,
        /// `default` as well as `skip_serializing_if`: without it an event
        /// with no facets serializes to JSON that will not deserialize, which
        /// is the common case rather than an edge one.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        filters: Vec<String>,
        /// The distinct page routes this result list showed, in rank order —
        /// ANA-20's per-page impressions. Anchors are dropped, so a query that
        /// showed three sections of one page showed that page once
        /// (`plan/rfcs/0706-what-shown-carries.md`). `serde(default)` reads an
        /// event written before the field existed.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        shown: Vec<String>,
    },
    /// A query that returned nothing: the list ANA-20 turns into "create a
    /// page for this query".
    NoResults {
        query: String,
        locale: Option<String>,
    },
    /// Which result was opened, and where it stood.
    Click {
        query: String,
        url: String,
        /// One-based, as a reader would count it.
        rank: usize,
    },
}

impl SearchEvent {
    /// The event a completed search emits.
    pub fn of(query: &str, locale: Option<&str>, filters: &Filters, hits: &[Hit]) -> Self {
        let query = scrub(query);
        let locale = locale.map(str::to_owned);
        if hits.is_empty() {
            return Self::NoResults { query, locale };
        }
        Self::Query {
            query,
            results: hits.len(),
            locale,
            filters: named(filters),
            shown: shown_pages(hits),
        }
    }

    pub fn click(query: &str, url: &str, rank: usize) -> Self {
        Self::Click {
            query: scrub(query),
            url: url.to_owned(),
            rank: rank + 1,
        }
    }

    pub fn query(&self) -> &str {
        match self {
            Self::Query { query, .. }
            | Self::NoResults { query, .. }
            | Self::Click { query, .. } => query,
        }
    }
}

/// Which facets were applied, by name — never their values, which on a private
/// site can name a reader's own group.
fn named(filters: &Filters) -> Vec<String> {
    let mut out = Vec::new();
    for (name, present) in [
        ("tab", filters.tab.is_some()),
        ("version", filters.version.is_some()),
        ("locale", filters.locale.is_some()),
        ("type", filters.kind.is_some()),
    ] {
        if present {
            out.push(name.to_owned());
        }
    }
    out
}

/// Masks what a reader sometimes pastes into a search box: an address, and a
/// long opaque run that is far more likely to be a key than a word.
/// The distinct pages a result list showed, in rank order.
///
/// By route rather than by `url`: `results` already counts the hits, and what
/// ANA-20 wants is the pages, so several sections of one page collapse to one
/// impression (RFC 0706). First appearance keeps its rank, so a consumer can
/// read position as well as presence.
fn shown_pages(hits: &[Hit]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(hits.len());
    for hit in hits {
        if !out.iter().any(|seen| seen == &hit.route) {
            out.push(hit.route.clone());
        }
    }
    out
}

pub fn scrub(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| {
            if word.contains('@') && word.contains('.') {
                return "[address]".to_owned();
            }
            let opaque = word.len() >= 24
                && word
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');
            if opaque && word.chars().any(|c| c.is_ascii_digit()) {
                return "[token]".to_owned();
            }
            word.to_owned()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::DocKind;
    use crate::idx::search::Hit;

    fn hit(url: &str) -> Hit {
        Hit {
            url: url.to_owned(),
            route: url.to_owned(),
            anchor: String::new(),
            title: String::new(),
            section: String::new(),
            breadcrumb: Vec::new(),
            kind: DocKind::Page,
            tab: None,
            version: None,
            locale: None,
            score: 1.0,
            updated: None,
            matched: 1,
            snippet: None,
        }
    }

    /// A hit on one section of a page, which is what a real result list is
    /// mostly made of.
    fn section(route: &str, anchor: &str) -> Hit {
        let mut hit = hit(&format!("{route}#{anchor}"));
        hit.route = route.to_owned();
        hit.anchor = anchor.to_owned();
        hit
    }

    #[test]
    fn a_query_with_results_records_their_count() {
        let event = SearchEvent::of(
            "rate limits",
            Some("en"),
            &Filters::default(),
            &[hit("/a"), hit("/b")],
        );
        assert_eq!(
            event,
            SearchEvent::Query {
                query: "rate limits".to_owned(),
                results: 2,
                locale: Some("en".to_owned()),
                filters: Vec::new(),
                shown: vec!["/a".to_owned(), "/b".to_owned()],
            }
        );
    }

    #[test]
    fn a_query_with_nothing_is_its_own_event() {
        let event = SearchEvent::of("quinoa", None, &Filters::default(), &[]);
        assert!(matches!(event, SearchEvent::NoResults { .. }));
    }

    #[test]
    fn a_click_counts_from_one() {
        let event = SearchEvent::click("rate limits", "/guides/limits", 0);
        assert_eq!(
            event,
            SearchEvent::Click {
                query: "rate limits".to_owned(),
                url: "/guides/limits".to_owned(),
                rank: 1,
            }
        );
    }

    #[test]
    fn a_filter_is_recorded_by_name_and_never_by_value() {
        let filters = Filters {
            version: Some("internal-beta".to_owned()),
            ..Filters::default()
        };
        let event = SearchEvent::of("limits", None, &filters, &[hit("/a")]);
        let json = serde_json::to_string(&event).expect("serializes");
        assert!(json.contains("\"version\""), "{json}");
        assert!(!json.contains("internal-beta"), "{json}");
    }

    #[test]
    fn no_event_carries_a_reader_identity() {
        let events = [
            SearchEvent::of("limits", Some("en"), &Filters::default(), &[hit("/a")]),
            SearchEvent::of("quinoa", None, &Filters::default(), &[]),
            SearchEvent::click("limits", "/a", 3),
        ];
        for event in events {
            let json = serde_json::to_string(&event).expect("serializes");
            for forbidden in ["subject", "session", "reader", "user", "ip", "email"] {
                assert!(
                    !json.contains(forbidden),
                    "`{forbidden}` in {json} (SRC-08: never the reader identity)"
                );
            }
        }
    }

    #[test]
    fn an_address_pasted_into_the_box_is_masked() {
        assert_eq!(
            scrub("invoice for ada@example.com"),
            "invoice for [address]"
        );
    }

    #[test]
    fn a_long_opaque_run_is_masked() {
        assert_eq!(
            scrub(concat!("sk_", "live_4eC39HqLyjWDarjtT1zdp7dc")),
            "[token]",
            "an API key is not a search term"
        );
        assert_eq!(
            scrub("authentication"),
            "authentication",
            "an ordinary long word is not a token"
        );
        assert_eq!(scrub("rate limits"), "rate limits");
    }

    #[test]
    fn scrubbing_keeps_the_shape_of_the_query() {
        assert_eq!(scrub("version:v2 rate limits"), "version:v2 rate limits");
        assert_eq!(scrub(""), "");
    }

    /// ANA-20 counts per-page impressions, so three sections of one page are
    /// one impression. Counting them separately would give a page a click rate
    /// divided by however many sections it happens to have — a property of the
    /// page's structure, not of anything a reader did.
    #[test]
    fn sections_of_one_page_are_one_impression() {
        let event = SearchEvent::of(
            "rate limits",
            Some("en"),
            &Filters::default(),
            &[
                section("/guides/limits", "burst"),
                hit("/guides/limits"),
                section("/guides/limits", "sustained"),
            ],
        );
        let SearchEvent::Query { shown, results, .. } = &event else {
            panic!("three hits is a query event: {event:?}");
        };
        assert_eq!(shown, &["/guides/limits"], "one page, shown once");
        assert_eq!(*results, 3, "`results` still counts the hits themselves");
    }

    #[test]
    fn shown_is_in_rank_order_and_keeps_the_first_appearance() {
        let event = SearchEvent::of(
            "limits",
            None,
            &Filters::default(),
            &[
                hit("/v2/guides/limits"),
                section("/guides/limits", "burst"),
                hit("/v1/guides/limits"),
                hit("/guides/limits"),
            ],
        );
        let SearchEvent::Query { shown, .. } = &event else {
            panic!("expected a query event");
        };
        assert_eq!(
            shown,
            &["/v2/guides/limits", "/guides/limits", "/v1/guides/limits"],
            "rank order, and `/guides/limits` keeps the position its section won"
        );
    }

    /// A version is a different page, not a duplicate of one.
    #[test]
    fn versions_of_a_page_are_separate_impressions() {
        let event = SearchEvent::of(
            "limits",
            None,
            &Filters::default(),
            &[hit("/v1/guides/limits"), hit("/v2/guides/limits")],
        );
        let SearchEvent::Query { shown, .. } = &event else {
            panic!("expected a query event");
        };
        assert_eq!(shown.len(), 2);
    }

    #[test]
    fn a_query_that_showed_nothing_carries_no_shown_list() {
        let event = SearchEvent::of("quinoa", None, &Filters::default(), &[]);
        // `NoResults` has no `shown` field at all: an empty list would be its
        // only possible value, so the type says so instead of the data.
        assert!(matches!(event, SearchEvent::NoResults { .. }));
    }

    /// The case the `skip_serializing_if`/`default` pair exists for, and the
    /// one that was broken: a search with no facets omits `filters` on the way
    /// out, so without `default` it could not be read back. Every real query
    /// on a site that declares no facets takes this path.
    #[test]
    fn an_event_with_no_facets_survives_a_round_trip() {
        let event = SearchEvent::of("limits", Some("en"), &Filters::default(), &[hit("/a")]);
        let json = serde_json::to_string(&event).expect("serializes");
        assert!(
            !json.contains("filters"),
            "an empty list stays off the wire"
        );
        assert_eq!(
            serde_json::from_str::<SearchEvent>(&json).expect("reads back"),
            event
        );
    }

    /// The wire shape, because a consumer reads JSON and not the enum. An
    /// empty list stays off the wire and `serde(default)` reads an event that
    /// predates the field, so ingest can take both while producers catch up.
    #[test]
    fn shown_round_trips_and_an_older_event_still_reads() {
        let event = SearchEvent::of(
            "limits",
            None,
            &Filters::default(),
            &[section("/guides/limits", "burst")],
        );
        let json = serde_json::to_value(&event).expect("serializes");
        assert_eq!(json["shown"], serde_json::json!(["/guides/limits"]));

        let older = serde_json::json!({
            "event": "query",
            "query": "limits",
            "results": 1,
            "locale": null
        });
        let parsed: SearchEvent = serde_json::from_value(older).expect("an older event reads");
        let SearchEvent::Query { shown, .. } = &parsed else {
            panic!("expected a query event");
        };
        assert!(shown.is_empty(), "absent reads as empty, not as an error");
    }
}
