//! ANA-02's published event schema must be reachable WITHOUT a session.
//!
//! `dashboard_read_ratchet.rs` holds the endpoints that should be guarded and
//! are not. This is the other direction, and it is not symmetric: an analytics
//! read served to anyone is a disclosure, while a *public* document that ends
//! up inside the guarded layer answers 401 to exactly the caller that needs it,
//! and nothing anywhere fails.
//!
//! The caller is the reason. `/_liyasa/schema/event.json` is what a static-site
//! collector and a browser client validate their events against BEFORE they are
//! permitted to post any, so a caller that reaches for it has no dashboard
//! credential by construction. That is why `liyasa-analytics` exposes it from
//! `schema_router()` rather than from `mount()`, and why it registers as a
//! second `Subtree` with `permission: None` — a single entry for the whole
//! crate would have swallowed it.
//!
//! What this can assert today and what it cannot are different, and the
//! difference is checked rather than assumed. Whether the route is mounted
//! belongs to `liyasa-server`; that it is never behind a permission is true
//! either way, so that half runs now.

use http::StatusCode;
use liyasa_tests::server::{Harness, header};
use std::path::PathBuf;

fn api_ts() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("web/dashboard/src/api.ts");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// The `auth: "public"` rows, read out of the declaration rather than restated.
///
/// A parser that found nothing would make every assertion below vacuous, so
/// finding nothing is a failure.
fn declared_public(source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in source.lines() {
        if !line.contains("auth: \"public\"") {
            continue;
        }
        let field = |name: &str| -> Option<String> {
            let at = line.find(&format!("{name}: "))? + name.len() + 2;
            let rest = &line[at..];
            let quote = rest.chars().next()?;
            if quote != '"' && quote != '`' {
                return None;
            }
            let start = rest.find(quote)? + 1;
            let end = rest[start..].find(quote)? + start;
            Some(rest[start..end].replace("${API_BASE}", "/_liyasa/api/v1"))
        };
        let id = field("id").unwrap_or_else(|| panic!("no `id` in: {line}"));
        let path = field("path").unwrap_or_else(|| panic!("no `path` in: {line}"));
        out.push((id, path));
    }
    assert!(
        !out.is_empty(),
        "no `auth: \"public\"` row parsed out of api.ts. Either the table's shape \
         changed, or ANA-02's schema stopped being declared public — and this test \
         would otherwise pass by finding nothing to check."
    );
    out
}

#[test]
fn the_schema_is_declared_public_and_it_is_the_only_one() {
    let rows = declared_public(&api_ts());
    let ids: Vec<&str> = rows.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        ["schema.event"],
        "every analytics endpoint but the published schema reads someone's traffic, \
         a search query or a feedback comment, and none of those may be public"
    );
    assert_eq!(rows[0].1, "/_liyasa/schema/event.json");
}

#[tokio::test]
async fn the_published_schema_is_never_behind_a_permission() {
    // True whether or not anything mounts it, which is what makes this half
    // meaningful before the route exists. A 401 here is the defect: the
    // document would be refused to the only caller that has a use for it.
    let (harness, _site) = Harness::serving("public-schema").await;
    for (id, path) in declared_public(&api_ts()) {
        let response = harness.get(&path).await;
        let status = response.status();
        assert_ne!(
            status,
            StatusCode::UNAUTHORIZED,
            "`{id}` is declared public and answered 401. A collector validates its \
             events against this document before it may post any, so it has no \
             session to offer."
        );
        assert_ne!(status, StatusCode::FORBIDDEN, "`{id}` is declared public");
    }
}

#[tokio::test]
async fn the_schema_is_served_once_something_mounts_it() {
    let (harness, _site) = Harness::serving("public-schema-served").await;

    // Whether the subtree mounted is the application's answer, not this
    // test's guess: `mounted` is what `routes::application` recorded while
    // composing, and a skipped subtree carries its own reason.
    let record = harness
        .mounted
        .iter()
        .find(|record| record.name.contains("analytics") && record.name.contains("schema"));
    let Some(record) = record.filter(|record| record.mounted) else {
        let reason = record
            .and_then(|record| record.skipped.clone())
            .unwrap_or_else(|| {
                "no analytics-schema subtree is registered in this build".to_owned()
            });
        eprintln!(
            "ana_02_public_schema: not asserting the body — {reason}. \
             The half that does not depend on mounting ran in the test above."
        );
        return;
    };
    assert!(record.mounted);

    let response = harness.get("/_liyasa/schema/event.json").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header(&response, "content-type"),
        Some("application/schema+json"),
        "a JSON Schema is served as one so a validator can dispatch on it"
    );

    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("a body");
    let served: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(served["$id"], liyasa_analytics::schema::ID);
    // What is served is the schema, not a page that happens to be JSON.
    assert!(
        served["properties"]["session_key"].is_object(),
        "the document served is not the event schema: {served}"
    );
}
