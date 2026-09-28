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
use liyasa_search::idx::Index;
use liyasa_search::idx::query::ReaderScope;

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

pub async fn handler(
    State(state): State<Arc<AppState>>,
    request: http::Request<axum::body::Body>,
) -> Response {
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
    let query_string = request.uri().query().unwrap_or_default().to_owned();
    let reader = request.extensions().get::<Principal>();
    let settings = liyasa_search::config::SearchSettings::default();

    // `rest` also returns a `SearchEvent`, `None` when the query never reached
    // the index — so a refused call is not counted as something a reader
    // searched for. It is dropped here rather than recorded: the ingest queue
    // takes an `EventRecord`, and inventing the mapping from `SearchEvent` to
    // one is WP-17's to write against ANA-01's schema, not mine to guess.
    // Search's analytics series is unfed until it does.
    let (response, _event) =
        liyasa_search::api::rest(index.as_ref(), &query_string, &settings, &scope(reader));

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
