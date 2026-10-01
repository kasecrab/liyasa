//! `GET /_liyasa/search` (REST-04, defect 146).
//!
//! Driven as HTTP through `routes::application`, not by calling the handler:
//! a handler nothing reaches is the rate-limit pool it replaces, one layer up,
//! and this package has already marked a row `done` on the strength of a test
//! that built its own state.

use http::StatusCode;
use liyasa_tests::server::{Harness, header};

#[tokio::test]
async fn a_query_is_answered_as_json_rather_than_404() {
    let (harness, _site) = Harness::serving("rest04-search").await;

    let response = harness.get("/_liyasa/search?q=install").await;
    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "the search endpoint is not mounted; the index nothing reads is still unread"
    );
    assert_eq!(
        header(&response, "content-type"),
        Some("application/json"),
        "a search answers data, never a page"
    );
}

/// The endpoint exists BECAUSE serving the index as a file shipped every
/// restricted page's text. A shared cache holding one reader's results is the
/// same disclosure by another route (AUTH-13), so the answer is never shared.
#[tokio::test]
async fn results_are_never_stored_by_a_shared_cache() {
    let (harness, _site) = Harness::serving("rest04-cache").await;

    let response = harness.get("/_liyasa/search?q=install").await;
    let cache = header(&response, "cache-control").unwrap_or_default();
    assert!(
        cache.contains("private") && cache.contains("no-store"),
        "search results are reader-dependent and must not be cached: {cache:?}"
    );
}

/// A query the parser rejects is refused with a reason rather than a panic or
/// an empty result set, which would read as "nothing matched".
#[tokio::test]
async fn a_request_with_no_query_is_refused_and_says_what_is_missing() {
    let (harness, _site) = Harness::serving("rest04-noquery").await;
    // The fixture site is built without an index, so this instance answers
    // 503 to every query and the refusal below is not the one under test.
    // Skipped with the reason rather than asserted around (RFC 1402); it
    // starts running the day the fixture builds an index, with no edit here.
    if harness.state.search_index().is_none() {
        eprintln!(
            "rest_04_search: not asserting the query refusal — this instance has no \
             search index, so every query is answered 503 before the parser sees it."
        );
        return;
    }

    let response = harness.get("/_liyasa/search").await;
    assert_ne!(response.status(), StatusCode::OK, "a search needs a query");
    let body = liyasa_tests::server::body_text(response).await;
    assert!(
        body.contains("q=") || body.to_lowercase().contains("query"),
        "the refusal names what is missing: {body}"
    );
}

/// The fixture site is built without a search index, which is the shape of
/// every site built before this route existed. It must say so rather than
/// answer 404 — a 404 is indistinguishable from the endpoint not existing,
/// which is the state this commit ends.
#[tokio::test]
async fn an_instance_with_no_index_says_so_rather_than_looking_unmounted() {
    let (harness, _site) = Harness::serving("rest04-noindex").await;

    let response = harness.get("/_liyasa/search?q=install").await;
    let status = response.status();
    let body = liyasa_tests::server::body_text(response).await;

    // Both branches assert. Written as `if 503 { .. } else { .. }` this
    // silently stopped checking anything the day the harness gained an index:
    // the false branch had no assertion, so a test about the 503's wording
    // passed by not reaching it. WP-17 caught that while fixing the harness.
    match status {
        StatusCode::SERVICE_UNAVAILABLE => assert!(
            body.contains("search index"),
            "the 503 names the missing index rather than looking unmounted: {body}"
        ),
        StatusCode::OK => assert!(
            body.contains("results") || body.contains("hits") || body.contains('{'),
            "an instance with an index answers a result document: {body}"
        ),
        other => panic!("a query is answered or refused, not {other}: {body}"),
    }
}
