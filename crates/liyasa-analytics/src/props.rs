//! What each event type puts in `props` (ANA-01, ANA-02).
//!
//! `EventRecord::props` is a `serde_json::Value`, so nothing in the type
//! system stops an emitter from spelling a key differently from the query that
//! reads it. PRD §6.2 puts the event schema in this crate, so the shapes live
//! here: an emitter constructs one of these and the dashboard queries read the
//! same names out of `json_extract`.
//!
//! The playground shape is the one worth reading twice. ANA-01 says a
//! playground event carries "only the operation ID, status class, and latency
//! (never URLs, headers, or bodies)", and [`Playground`] has no field that
//! could hold one.

use liyasa_store::records::EventRecord;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// Nothing in this module renames its fields, and that is deliberate. These
// shapes are STORED, in the `props` column, and the queries in `search.rs` and
// `insights.rs` read them back with `json_extract(props, '$.status_class')`.
// Renaming them to camelCase for the wire would silently empty those reports,
// because `json_extract` returns null for a key that is not there rather than
// failing. The API response types in `traffic.rs`, `search.rs` and
// `retention.rs` are camelCase; these are not, and `key::` above is the list
// both sides share.

/// `props` keys, so a query and an emitter cannot disagree by a typo.
pub mod key {
    pub const QUERY: &str = "q";
    pub const SHOWN: &str = "shown";
    pub const RESULTS: &str = "results";
    pub const POSITION: &str = "position";
    pub const TARGET: &str = "target";
    pub const DEPTH: &str = "depth";
    pub const OPERATION: &str = "operation";
    pub const STATUS_CLASS: &str = "status_class";
    pub const LATENCY_MS: &str = "latency_ms";
    pub const TOOL: &str = "tool";
    pub const RATING: &str = "rating";
    pub const FEEDBACK_ID: &str = "feedback_id";
    pub const THREAD: &str = "thread";
    pub const BUILD: &str = "build";
    pub const ENVIRONMENT: &str = "environment";
}

/// What a search contributes to an event, without the request half.
///
/// `liyasa-search` builds a `SearchEvent` on every query and `routes/search.rs`
/// drops it, with a comment saying the mapping to `EventRecord` was not
/// invented there. It was right not to: which field goes where is this crate's
/// decision, because this crate reads them back in `search.rs`.
///
/// This carries only what the search knows. Everything else — the timestamp,
/// the site, the route, the session key, the caller, the device — is the
/// server's, and [`Emission::into_record`] leaves all of it untouched.
///
/// **There is deliberately no dependency on `liyasa-search` here.** Naming
/// `SearchEvent` would put tantivy in this crate's graph for a forty-line
/// mapping. Instead there is one constructor per variant, so the call site
/// matches the enum and invents nothing:
///
/// ```text
/// let emission = match event {
///     SearchEvent::Query { query, results, locale, filters } =>
///         props::search_event(query, *results, locale.as_deref(), filters),
///     SearchEvent::NoResults { query, locale } =>
///         props::search_no_results(query, locale.as_deref()),
///     SearchEvent::Click { query, url, rank } =>
///         props::search_click(query, url, *rank as u32),
/// };
/// state.ingest.push(emission.into_record(base));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emission {
    /// The event `type`.
    pub kind: &'static str,
    /// ANA-02's `variant`, which is what ANA-71 filters by.
    pub variant: Value,
    pub props: Value,
}

impl Emission {
    /// Why `shown` is absent from a mapped search, named here so it is
    /// findable rather than folklore.
    pub const SHOWN_NEEDS: &'static str =
        "liyasa-search::SearchEvent::Query carries no result routes";

    /// Fills the search half of a record the caller has already built.
    ///
    /// Only `kind`, `variant` and `props` are set. A mapping that also touched
    /// the session key or the caller would break unique-session counts and the
    /// human-against-agent split without failing anything.
    pub fn into_record(self, base: EventRecord) -> EventRecord {
        EventRecord {
            kind: self.kind.to_owned(),
            variant: self.variant,
            props: self.props,
            ..base
        }
    }
}

/// `locale` is the only variant dimension a search knows. `null` rather than
/// an omitted key, so the shape is the same either way.
fn variant_of(locale: Option<&str>) -> Value {
    json!({ "locale": locale })
}

/// A query that returned something (ANA-20).
///
/// `facets` are names only — `liyasa-search` never sends their values, because
/// on a private site a facet value can name a reader's own group.
pub fn search_event(
    query: &str,
    results: usize,
    locale: Option<&str>,
    facets: &[String],
) -> Emission {
    let mut props = json!({ "q": query, "results": results });
    if !facets.is_empty() {
        props["filters"] = json!(facets);
    }
    Emission {
        kind: "search",
        variant: variant_of(locale),
        props,
    }
}

/// A query that returned nothing.
///
/// `results: 0` is written explicitly. ANA-20's no-result list is
/// `json_extract(props, '$.results') = 0`, and an omitted key is null rather
/// than zero there — the row would drop out of the report that exists for it.
pub fn search_no_results(query: &str, locale: Option<&str>) -> Emission {
    Emission {
        kind: "search",
        variant: variant_of(locale),
        props: json!({ "q": query, "results": 0 }),
    }
}

/// A result opened from the overlay.
///
/// `position` is passed through unchanged: `SearchEvent::click` already
/// converts to one-based, and adding one here would put every click one rank
/// further down than it happened.
pub fn search_click(query: &str, target: &str, position: u32) -> Emission {
    Emission {
        kind: "search_click",
        variant: json!({ "locale": null }),
        props: json!({ "q": query, "target": target, "position": position }),
    }
}

/// `type: "search"` — one query run (ANA-20). The text is scrubbed before it
/// reaches storage (ANA-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Search {
    pub q: String,
    /// How many results came back. Zero is the no-result case ANA-20 reports.
    pub results: u32,
    /// The routes the result list showed, in rank order. ANA-20 asks for
    /// per-page impressions, and a page is only impressed on a reader if it
    /// was in the list; nothing else on the event records which pages those
    /// were. Capped at the visible page of results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shown: Vec<String>,
}

/// `type: "search_click"` — a result opened from the overlay (ANA-20).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchClick {
    pub q: String,
    /// The route clicked through to, which is what "most clicked result" ranks.
    pub target: String,
    /// One-based rank in the result list.
    pub position: u32,
}

/// `type: "playground_request"` — ANA-01 permits three fields and no more.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playground {
    /// The OpenAPI `operationId`. Never a URL: a URL carries the path
    /// parameters a caller filled in.
    pub operation: String,
    /// `2xx`, `4xx`, `5xx` — a class, not a code, and never a body.
    pub status_class: String,
    pub latency_ms: u32,
}

/// The three classes a playground response is reduced to.
pub fn status_class(status: u16) -> &'static str {
    match status {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        _ => "5xx",
    }
}

/// `type: "mcp_call"` (ANA-10 counts these as first-class agent adoption).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpCall {
    pub tool: String,
    pub status_class: String,
    pub latency_ms: u32,
}

/// `type: "assistant_message"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantMessage {
    /// Opaque thread identifier; never the message text.
    pub thread: String,
    pub latency_ms: u32,
    /// `1`, `-1`, or absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<i32>,
}

/// `type: "feedback"` — the marker the server pushes beside the stored row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feedback {
    pub feedback_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<i32>,
}

/// `type: "deployment"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deployment {
    pub build: String,
    pub environment: String,
}

/// `type: "scroll_depth"` — a percentage, reported by the reader runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrollDepth {
    pub depth: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_playground_event_has_nowhere_to_put_a_url() {
        let value = serde_json::to_value(Playground {
            operation: "createPayment".to_owned(),
            status_class: status_class(422).to_owned(),
            latency_ms: 31,
        })
        .expect("serialises");
        let keys: Vec<&str> = value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["latency_ms", "operation", "status_class"]);
    }

    #[test]
    fn a_status_becomes_a_class_and_loses_the_code() {
        assert_eq!(status_class(200), "2xx");
        assert_eq!(status_class(201), "2xx");
        assert_eq!(status_class(404), "4xx");
        assert_eq!(status_class(422), "4xx");
        assert_eq!(status_class(503), "5xx");
        // Anything above 599 is still a failure, not a panic or a gap.
        assert_eq!(status_class(999), "5xx");
    }
}
