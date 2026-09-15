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
use crate::idx::query::{self, Filters, Query, ReaderScope};
use crate::idx::search::{Hit, SearchOptions};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
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
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
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

/// What a surface searches. The browser index and the server index both
/// implement it and both rank with `idx::score`, so "the same ranking" (SRC-10)
/// holds for a private site as well as a static one.
pub trait Engine {
    fn run(
        &self,
        query: &Query,
        context: &Context,
        options: &SearchOptions,
    ) -> Result<Vec<Hit>, SearchError>;
}

impl Engine for Index {
    fn run(
        &self,
        query: &Query,
        context: &Context,
        options: &SearchOptions,
    ) -> Result<Vec<Hit>, SearchError> {
        self.search(query, context, options)
    }
}

/// Runs a request against an index. Both engines rank with `idx::score`.
pub fn search<E: Engine + ?Sized>(
    index: &E,
    request: &SearchRequest,
    settings: &SearchSettings,
    reader: &ReaderScope,
) -> Result<(SearchResponse, SearchEvent), SearchError> {
    if let Some(limit) = request.limit
        && limit > MAX_LIMIT
    {
        return Err(SearchError::Query(format!(
            "`limit` is {limit}; this endpoint returns at most {MAX_LIMIT} results per query"
        )));
    }
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

    let hits = index.run(&parsed, &context, &options)?;
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

/// Where the REST endpoint is mounted, so the server, the docs, and the agent
/// surfaces name one path.
pub const REST_PATH: &str = "/api/search";

/// The MCP tool's name, as `tool_schema` publishes it.
pub const TOOL_NAME: &str = "search";

/// The largest `limit` a caller may ask for. The same number is the `maximum`
/// in [`tool_schema`], so a conforming agent never trips it.
pub const MAX_LIMIT: usize = 50;

/// A REST answer, with no transport in it: the server turns this into whatever
/// its HTTP layer speaks.
#[derive(Debug, Clone, PartialEq)]
pub struct RestResponse {
    pub status: u16,
    /// A [`SearchResponse`] at 200, a `Diagnostic` otherwise.
    pub body: serde_json::Value,
}

/// `GET /api/search?q=...`. The parameters are the request's fields, with the
/// facets spelled `filters.<name>`; `q` is the short spelling of `query`.
pub fn request_from_query_string(query_string: &str) -> Result<SearchRequest, SearchError> {
    let mut request = SearchRequest::default();
    let mut seen_query = false;

    for pair in query_string.trim_start_matches('?').split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (decode(key)?, decode(value)?),
            None => (decode(pair)?, String::new()),
        };
        match key.as_str() {
            "q" | "query" => {
                request.query = value;
                seen_query = true;
            }
            "locale" => request.locale = Some(value),
            "version" => request.version = Some(value),
            "tab" => request.tab = Some(value),
            "limit" => request.limit = Some(number("limit", &value)?),
            "snippets" => request.snippets = Some(boolean("snippets", &value)?),
            "filters.tab" => request.filters.tab = Some(value),
            "filters.version" => request.filters.version = Some(value),
            "filters.locale" => request.filters.locale = Some(value),
            "filters.type" => request.filters.kind = Some(value),
            other => {
                return Err(SearchError::Query(format!(
                    "`{other}` is not a search parameter; expected one of q, locale, version, \
                     tab, limit, snippets, filters.tab, filters.version, filters.locale, \
                     filters.type"
                )));
            }
        }
    }

    if !seen_query || request.query.trim().is_empty() {
        return Err(SearchError::Query(
            "a search needs a query: `?q=<words>`".to_owned(),
        ));
    }
    Ok(request)
}

/// Answers the REST endpoint. The event is `None` when the request never
/// reached the index, so a refused call is not counted as something a reader
/// searched for.
pub fn rest<E: Engine + ?Sized>(
    index: &E,
    query_string: &str,
    settings: &SearchSettings,
    reader: &ReaderScope,
) -> (RestResponse, Option<SearchEvent>) {
    let request = match request_from_query_string(query_string) {
        Ok(request) => request,
        Err(error) => return (refused(&error), None),
    };
    match search(index, &request, settings, reader) {
        Ok((response, event)) => {
            let body = serde_json::to_value(&response)
                .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }));
            (RestResponse { status: 200, body }, Some(event))
        }
        Err(error) => (refused(&error), None),
    }
}

/// The MCP `tools/list` payload.
pub fn tools_list() -> serde_json::Value {
    serde_json::json!({ "tools": [tool_schema()] })
}

/// The MCP `tools/call` handler. A caller's mistake comes back as a tool
/// result with `isError`, which is what the protocol asks for: the model reads
/// it and retries, rather than the transport failing under it.
pub fn tool_call<E: Engine + ?Sized>(
    index: &E,
    name: &str,
    arguments: &serde_json::Value,
    settings: &SearchSettings,
    reader: &ReaderScope,
) -> (serde_json::Value, Option<SearchEvent>) {
    if name != TOOL_NAME {
        return (
            tool_error(format!(
                "`{name}` is not a tool; this server offers `{TOOL_NAME}`"
            )),
            None,
        );
    }
    let request: SearchRequest = match serde_json::from_value(arguments.clone()) {
        Ok(request) => request,
        Err(error) => return (tool_error(error.to_string()), None),
    };
    if request.query.trim().is_empty() {
        return (tool_error("a search needs a `query`".to_owned()), None);
    }

    match search(index, &request, settings, reader) {
        Ok((response, event)) => {
            let structured = serde_json::to_value(&response)
                .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() }));
            let result = serde_json::json!({
                "content": [{ "type": "text", "text": transcript(&response) }],
                "structuredContent": structured,
                "isError": false,
            });
            (result, Some(event))
        }
        Err(error) => {
            let diagnostic = error.diagnostic();
            (
                tool_error(format!(
                    "{}: {} ({})",
                    diagnostic.code.as_str(),
                    diagnostic.message,
                    diagnostic.url
                )),
                None,
            )
        }
    }
}

/// What a model reads. One line per result, because an agent that has to parse
/// prose to find a route will parse it wrong.
fn transcript(response: &SearchResponse) -> String {
    use std::fmt::Write as _;

    if response.results.is_empty() {
        return format!("No results for `{}`.", response.query);
    }
    let mut out = format!(
        "{} result{} for `{}`:\n",
        response.total,
        if response.total == 1 { "" } else { "s" },
        response.query
    );
    for (rank, result) in response.results.iter().enumerate() {
        let title = if result.section == result.title || result.section.is_empty() {
            result.title.clone()
        } else {
            format!("{} › {}", result.title, result.section)
        };
        let _ = write!(out, "\n{}. {} — {}", rank + 1, result.url, title);
        if let Some(snippet) = &result.snippet {
            let _ = write!(out, "\n   {}", snippet.replace('\n', " "));
        }
    }
    out
}

fn refused(error: &SearchError) -> RestResponse {
    let diagnostic = error.diagnostic();
    RestResponse {
        status: match error {
            // The caller can fix these by asking differently.
            SearchError::Query(_) => 400,
            // A broken or mismatched index is the deployment's problem.
            SearchError::Corrupt { .. }
            | SearchError::FormatVersion { .. }
            | SearchError::MissingDictionary(_) => 500,
        },
        body: serde_json::to_value(&diagnostic)
            .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() })),
    }
}

fn tool_error(message: String) -> serde_json::Value {
    serde_json::json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true,
    })
}

fn number(key: &str, value: &str) -> Result<usize, SearchError> {
    value
        .parse()
        .map_err(|_| SearchError::Query(format!("`{key}` is `{value}`, which is not a number")))
}

fn boolean(key: &str, value: &str) -> Result<bool, SearchError> {
    match value {
        "true" | "1" | "" => Ok(true),
        "false" | "0" => Ok(false),
        other => Err(SearchError::Query(format!(
            "`{key}` is `{other}`, which is not true or false"
        ))),
    }
}

/// `application/x-www-form-urlencoded`, rejecting a broken escape rather than
/// replacing it: a mangled query should be reportable, not silently answered.
fn decode(text: &str) -> Result<String, SearchError> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'+' => {
                out.push(b' ');
                at += 1;
            }
            b'%' => {
                let hex = bytes
                    .get(at + 1..at + 3)
                    .and_then(|pair| std::str::from_utf8(pair).ok())
                    .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                    .ok_or_else(|| {
                        SearchError::Query(format!(
                            "`{text}` is not a valid query string: `%` must be followed by two \
                             hex digits"
                        ))
                    })?;
                out.push(hex);
                at += 3;
            }
            byte => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(out)
        .map_err(|_| SearchError::Query(format!("`{text}` does not decode to text")))
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
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT },
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
    fn a_field_the_request_does_not_have_is_refused_rather_than_ignored() {
        // `additionalProperties: false` in the published schema, enforced.
        let error = serde_json::from_str::<SearchRequest>(r#"{ "query": "a", "sort": "date" }"#)
            .expect_err("must reject");
        assert!(error.to_string().contains("sort"), "{error}");
    }

    #[test]
    fn a_broken_escape_is_a_diagnostic_rather_than_a_replacement_character() {
        let error = decode("rate%2limits").expect_err("must reject");
        assert_eq!(error.diagnostic().code.as_str(), "E1004");
        assert_eq!(decode("rate%20limits").expect("decodes"), "rate limits");
        assert_eq!(decode("%E6%A4%9C%E7%B4%A2").expect("decodes"), "検索");
    }

    #[test]
    fn a_broken_index_is_the_servers_fault_and_a_bad_query_is_the_callers() {
        assert_eq!(refused(&SearchError::Query("bad".to_owned())).status, 400);
        assert_eq!(refused(&SearchError::corrupt("docs-0.bin")).status, 500);
        assert_eq!(
            refused(&SearchError::FormatVersion {
                found: 2,
                supported: 1
            })
            .status,
            500
        );
    }

    #[test]
    fn the_tool_schema_names_the_one_required_field() {
        let schema = tool_schema();
        assert_eq!(schema["name"], "search");
        assert_eq!(schema["inputSchema"]["required"][0], "query");
        assert_eq!(schema["inputSchema"]["additionalProperties"], false);
    }
}
