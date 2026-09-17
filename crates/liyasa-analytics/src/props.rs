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

use serde::{Deserialize, Serialize};

/// `props` keys, so a query and an emitter cannot disagree by a typo.
pub mod key {
    pub const QUERY: &str = "q";
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

/// `type: "search"` — one query run (ANA-20). The text is scrubbed before it
/// reaches storage (ANA-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Search {
    pub q: String,
    /// How many results came back. Zero is the no-result case ANA-20 reports.
    pub results: u32,
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
