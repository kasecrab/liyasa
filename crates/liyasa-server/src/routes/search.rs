//! `GET /_liyasa/search` (REST-04, defect 146).
//!
//! The index is built into `dist/search-index/` and, until this route, nothing
//! read it. It was briefly served as a static file, which was recognised as
//! shipping every restricted page's text to anyone who asked — so the answer
//! is an endpoint that filters per reader, never a file.
//!
//! The filtering is `ReaderScope`, which `liyasa-search` applies inside the
//! query rather than to the results: a hit a reader may not see is not scored,
//! so it cannot leak through a snippet, a total or a facet count either.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use http::{StatusCode, header};
use liyasa_search::analytics::SearchEvent;
use liyasa_search::idx::Index;
use liyasa_search::idx::query::ReaderScope;
use serde_json::json;

/// The path this endpoint answers at, and the `route` every event carries.
pub const PATH: &str = "/_liyasa/search";

use super::AppState;
use crate::auth::session::Principal;

/// Everything under `dist/search-index/`, read once at startup.
///
/// Reading it per request would re-parse every shard for every query. It is
/// `None` on an instance whose bundle has no index — a site built before the
/// index existed, or a collector, which serves nothing.
pub fn open(dist: &std::path::Path) -> Option<Index> {
    let directory = dist.join("search-index");
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(&directory).ok()? {
        let entry = entry.ok()?;
        if entry.file_type().ok()?.is_file()
            && let Some(name) = entry.file_name().to_str()
        {
            files.insert(name.to_owned(), std::fs::read(entry.path()).ok()?);
        }
    }
    match Index::open(files) {
        Ok(index) => Some(index),
        Err(error) => {
            tracing::warn!(
                target: "liyasa_server",
                %error,
                directory = %directory.display(),
                "the search index did not open; search answers 503"
            );
            None
        }
    }
}

/// What this reader may be shown, from the session the auth layer resolved.
///
/// An anonymous reader has no groups, which `ReaderScope::admits` reads as
/// "public pages only" — the same decision `auth::decide` reaches for a page,
/// arrived at by a different route because a query is not a page fetch.
fn scope(reader: Option<&Principal>) -> ReaderScope {
    match reader {
        Some(principal) => ReaderScope {
            groups: principal.groups.iter().cloned().collect(),
            region: principal.region.clone(),
        },
        None => ReaderScope::default(),
    }
}

/// Who made this search, from the only thing that can tell.
///
/// Extracted so it can be asserted: the defect this replaced was an omitted
/// field, and an omitted field is invisible to a test of the function that
/// omits it. A unit test over this names the four answers.
fn caller_of(user_agent: Option<&str>) -> (super::session::CallerKind, Option<String>) {
    super::session::classify(user_agent, false)
}

/// One search event, mapped through `liyasa-analytics`' constructors.
///
/// The match is here rather than in that crate on purpose: naming
/// `SearchEvent` there would pull tantivy into its dependency graph for a
/// forty-line mapping, so WP-17 built three constructors and left the enum to
/// the crate that already depends on both.
///
/// `into_record` sets only `kind`, `variant` and `props`. The timestamp, site,
/// route, session key, caller and device come from the base record built here
/// — a mapping that reset the session key would break unique-session counts
/// and the human/agent split without failing anything.
fn record(state: &Arc<AppState>, event: &SearchEvent, parts: &http::request::Parts) {
    use liyasa_analytics::props;

    let emission = match event {
        SearchEvent::Query {
            query,
            results,
            locale,
            filters,
        } => props::search_event(query, *results, locale.as_deref(), filters),
        SearchEvent::NoResults { query, locale } => {
            props::search_no_results(query, locale.as_deref())
        }
        SearchEvent::Click { query, url, rank } => props::search_click(query, url, *rank as u32),
    };

    let user_agent = parts
        .headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok());
    // `structural: false`, which is the judgement in this call and not a
    // default. `session::classify` takes it because the answer is per route:
    // the MCP surface hardcodes `agent` because only agents reach it, and the
    // browser beacon passes `false` because only readers do.
    //
    // `/_liyasa/search` is reached by BOTH — the reader's own search box and an
    // agent working the REST surface — so the route discriminates nothing and
    // the user agent is the only thing that can. `true` would file every
    // reader's search as agent traffic, which is the majority case and the
    // wrong answer; `false` still files a known agent UA as `Agent`, a bot as
    // `Bot`, and an absent UA as `Integration` rather than as a person.
    let (kind, agent_name) = caller_of(user_agent);
    let base = liyasa_store::records::EventRecord {
        ts: liyasa_store::now_ms(),
        site: state.config.site.clone(),
        env: state.config.env.clone(),
        route: PATH.to_owned(),
        // Never `..Default::default()` for this field. `EventRecord` derives
        // `Default`, so an omitted caller is `Value::Null`, and the dashboard
        // groups on `json_extract(caller, '$.kind')` — a NULL kind folds into
        // `human`, so an agent's search would be counted as a person's with no
        // error and no empty table (WP-17 found this). The doc comment above
        // guards the session key against exactly this and I left the caller
        // half of the same sentence unguarded.
        caller: json!({ "kind": kind.as_str(), "agent_name": agent_name }),
        format: "json".to_owned(),
        session_key: state
            .salt
            .key(state.client_ip(parts), user_agent, &state.config.site),
        ..Default::default()
    };
    let _ = state.ingest.push(emission.into_record(base));
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    request: http::Request<axum::body::Body>,
) -> Response {
    let (parts, _body) = request.into_parts();
    let Some(index) = state.search_index() else {
        // `no-store` here too, not only on a result: whether this instance has
        // an index is a property of the instance, and a shared cache holding
        // the 503 would answer for a replica that does.
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "private, no-store"),
            ],
            r#"{"error":"this instance has no search index"}"#,
        )
            .into_response();
    };
    let query_string = parts.uri.query().unwrap_or_default().to_owned();
    let reader = parts.extensions.get::<Principal>();
    let settings = liyasa_search::config::SearchSettings::default();

    // `None` when the query never reached the index, so a refused call is not
    // counted as something a reader searched for (ANA-01).
    let (response, event) =
        liyasa_search::api::rest(index.as_ref(), &query_string, &settings, &scope(reader));
    if let Some(event) = event {
        record(&state, &event, &parts);
    }

    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    // Reader-dependent by construction: the same query returns different
    // results to different readers, so a shared cache holding one reader's
    // copy is the disclosure this endpoint exists to avoid (AUTH-13).
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        serde_json::to_string(&response.body).unwrap_or_else(|_| "{}".to_owned()),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four answers, and above all that none of them is absent.
    ///
    /// A null caller is not a fifth answer — the dashboard groups on
    /// `json_extract(caller, '$.kind')` and folds an unknown kind into
    /// `human`, so an omitted field counts every agent as a person with no
    /// error anywhere (WP-17's finding).
    #[test]
    fn every_search_names_who_made_it() {
        use super::super::session::CallerKind;

        let (kind, name) = caller_of(Some("ClaudeBot/1.0"));
        assert_eq!(kind, CallerKind::Agent);
        assert_eq!(name.as_deref(), Some("claudebot"));

        let (kind, _) = caller_of(Some(
            "Mozilla/5.0 (Macintosh) AppleWebKit/537.36 Chrome/120 Safari/537.36",
        ));
        assert_eq!(kind, CallerKind::Human, "a reader's search box is a person");

        // Not `Human`: a request with no user agent is a script.
        let (kind, _) = caller_of(None);
        assert_eq!(kind, CallerKind::Integration);

        for agent in [Some("ClaudeBot/1.0"), Some("Chrome/120"), None] {
            let (kind, _) = caller_of(agent);
            assert!(
                !kind.as_str().is_empty(),
                "a search always records a caller kind"
            );
        }
    }
}
