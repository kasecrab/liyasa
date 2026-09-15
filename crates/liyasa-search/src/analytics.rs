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
        #[serde(skip_serializing_if = "Vec::is_empty")]
        filters: Vec<String>,
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
}
