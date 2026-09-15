//! The one request and response shape every caller shares (SRC-10).
//!
//! The MCP `search` tool, the REST endpoint, the CLI, and the browser worker
//! all go through here, so "the same ranking" is not a promise four callers
//! have to keep separately.

use serde::{Deserialize, Serialize};

use crate::analytics::SearchEvent;
use crate::config::SearchSettings;
use crate::doc::DocKind;
use crate::error::SearchError;
use crate::idx::Index;
use crate::idx::manifest::Context;
use crate::idx::query::{self, Filters, ReaderScope};
use crate::idx::search::{Hit, SearchOptions};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchRequest {
    pub query: String,
    /// The page the search was opened from, which picks the shard.
    pub locale: Option<String>,
    pub version: Option<String>,
    pub tab: Option<String>,
    /// Facets, in addition to any the query string carries as `version:v2`.
    pub filters: RequestFilters,
    /// `search.maxResults` unless the caller asks for fewer.
    pub limit: Option<usize>,
    pub snippets: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RequestFilters {
    pub tab: Option<String>,
    pub version: Option<String>,
    pub locale: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

impl RequestFilters {
    fn apply(&self, filters: &mut Filters) -> Result<(), SearchError> {
        for (name, value) in [
            ("tab", self.tab.as_deref()),
            ("version", self.version.as_deref()),
            ("locale", self.locale.as_deref()),
            ("type", self.kind.as_deref()),
        ] {
            if let Some(value) = value {
                filters.set(name, value)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub url: String,
    pub title: String,
    pub section: String,
    pub breadcrumb: Vec<String>,
    #[serde(rename = "type")]
    pub kind: DocKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// Byte ranges within `snippet` that matched, for the caller to mark up.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<(u32, u32)>,
    pub score: f32,
}

impl From<Hit> for SearchResult {
    fn from(hit: Hit) -> Self {
        let (snippet, highlights) = match hit.snippet {
            Some(snippet) => (
                Some(snippet.text),
                snippet
                    .highlights
                    .into_iter()
                    .map(|h| (h.start, h.end))
                    .collect(),
            ),
            None => (None, Vec::new()),
        };
        Self {
            url: hit.url,
            title: hit.title,
            section: hit.section,
            breadcrumb: hit.breadcrumb,
            kind: hit.kind,
            tab: hit.tab,
            version: hit.version,
            locale: hit.locale,
            snippet,
            highlights,
            score: hit.score,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    /// The query as parsed, which is what an agent should echo rather than the
    /// raw string.
    pub query: String,
    pub results: Vec<SearchResult>,
    /// How many the index found before `limit` was applied.
    pub total: usize,
}

/// Runs a request against the browser index. The server runs the same request
/// through `server::ServerSearcher`; both rank with `idx::score`.
pub fn search(
    index: &Index,
    request: &SearchRequest,
    settings: &SearchSettings,
    reader: &ReaderScope,
) -> Result<(SearchResponse, SearchEvent), SearchError> {
    let locale = request.locale.as_deref().unwrap_or("en");
    let mut parsed = query::parse(&request.query, locale)?;
    request.filters.apply(&mut parsed.filters)?;
    settings.allow(&mut parsed.filters);

    let context = Context {
        locale: request.locale.clone(),
        version: request.version.clone(),
        tab: request.tab.clone(),
    };
    let options = SearchOptions {
        max_results: request.limit.unwrap_or(settings.max_results),
        snippets: request.snippets.unwrap_or(settings.snippets),
        reader: reader.clone(),
        ..settings.search_options()
    };

    let hits = index.search(&parsed, &context, &options)?;
    let event = SearchEvent::of(
        &request.query,
        request.locale.as_deref(),
        &parsed.filters,
        &hits,
    );
    let total = hits.len();
    Ok((
        SearchResponse {
            query: parsed.raw.clone(),
            results: hits.into_iter().map(SearchResult::from).collect(),
            total,
        },
        event,
    ))
}

/// The MCP tool's JSON Schema, so the server and the docs describe one tool.
pub fn tool_schema() -> serde_json::Value {
    serde_json::json!({
        "name": "search",
        "description": "Full-text search over this site's documentation. \
                        Returns one result per page section, best first.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Words to search for. Supports \"exact phrases\", \
                                    trailing prefixes, and filters such as `version:v2`."
                },
                "locale": { "type": "string", "description": "BCP 47 tag, such as `en`." },
                "version": { "type": "string" },
                "tab": { "type": "string" },
                "filters": {
                    "type": "object",
                    "properties": {
                        "tab": { "type": "string" },
                        "version": { "type": "string" },
                        "locale": { "type": "string" },
                        "type": { "type": "string", "enum": ["page", "endpoint", "changelog"] }
                    },
                    "additionalProperties": false
                },
                "limit": { "type": "integer", "minimum": 1, "maximum": 50 },
                "snippets": { "type": "boolean" }
            },
            "required": ["query"],
            "additionalProperties": false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_round_trips_through_json() {
        let json = r#"{
            "query": "rate limits",
            "locale": "en",
            "filters": { "type": "endpoint" },
            "limit": 5
        }"#;
        let request: SearchRequest = serde_json::from_str(json).expect("parses");
        assert_eq!(request.query, "rate limits");
        assert_eq!(request.filters.kind.as_deref(), Some("endpoint"));
        assert_eq!(request.limit, Some(5));
    }

    #[test]
    fn an_unknown_content_type_is_a_diagnostic_rather_than_silence() {
        let mut filters = Filters::default();
        let request = RequestFilters {
            kind: Some("widget".to_owned()),
            ..RequestFilters::default()
        };
        let error = request.apply(&mut filters).expect_err("must reject");
        assert_eq!(error.diagnostic().code.as_str(), "E1004");
    }

    #[test]
    fn the_tool_schema_names_the_one_required_field() {
        let schema = tool_schema();
        assert_eq!(schema["name"], "search");
        assert_eq!(schema["inputSchema"]["required"][0], "query");
        assert_eq!(schema["inputSchema"]["additionalProperties"], false);
    }
}
