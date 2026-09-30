//! Search feeds the analytics series (ANA-20, ANA-06).
//!
//! Two call sites WP-17 built constructors for, verified from this side: the
//! `SearchEvent` mapping and the plan's retention ceiling. The mapping lives
//! here rather than in `liyasa-analytics` because naming `SearchEvent` there
//! would pull tantivy into that crate's graph for forty lines.

use liyasa_analytics::retention::Policy;

/// The plan narrows the operator's window and never widens it.
///
/// Before this, `org/routes.rs:301` reported `analyticsRetentionDays` and
/// nothing acted on it — the API told an operator a number the system did not
/// honour, which is worse than not reporting one.
#[test]
fn a_plan_ceiling_narrows_retention_and_a_generous_one_does_not_widen_it() {
    let configured = Policy::default();

    let capped = configured.capped_by(Some(30));
    assert_eq!(capped.raw_days, 30, "the plan's ceiling wins");
    assert!(
        capped.aggregate_months < configured.aggregate_months,
        "the cap reaches the rollups too, or it honours the ceiling on the rows \
         nobody queries and ignores it on the ones the dashboard draws from"
    );

    let generous = configured.capped_by(Some(100_000));
    assert_eq!(
        (generous.raw_days, generous.aggregate_months),
        (configured.raw_days, configured.aggregate_months),
        "a plan more generous than the configuration changes nothing"
    );

    let uncapped = configured.capped_by(None);
    assert_eq!(
        (uncapped.raw_days, uncapped.aggregate_months),
        (configured.raw_days, configured.aggregate_months),
        "a self-hosted instance has no plan and no cap"
    );
}

/// `into_record` must fill only what the search event knows.
///
/// A mapping that also touched the session key would break unique-session
/// counts and the human/agent split, and nothing would fail — the series would
/// simply be wrong.
#[test]
fn the_mapping_leaves_the_session_key_and_the_caller_alone() {
    let base = liyasa_store::records::EventRecord {
        ts: 1_700_000_000_000,
        site: "docs".to_owned(),
        env: "production".to_owned(),
        route: "/_liyasa/search".to_owned(),
        session_key: "a-session".to_owned(),
        caller: serde_json::json!({ "kind": "agent" }),
        ..Default::default()
    };

    let record = liyasa_analytics::props::search_event("rate limits", 3, Some("en"), &[])
        .into_record(base.clone());

    assert_eq!(record.kind, "search");
    assert_eq!(record.props["q"], "rate limits");
    assert_eq!(record.props["results"], 3);
    assert_eq!(
        record.session_key, base.session_key,
        "the key passes through"
    );
    assert_eq!(record.caller, base.caller, "the caller passes through");
    assert_eq!(record.ts, base.ts);
    assert_eq!(record.route, base.route);
}

/// A query the index refused is not something a reader searched for.
#[tokio::test]
async fn a_refused_query_records_nothing() {
    use http::StatusCode;
    use liyasa_tests::server::Harness;

    let (harness, _site) = Harness::serving("ana20-refused").await;
    let before = harness.state.ingest.depth();

    // No index on the fixture, so this never reaches the parser.
    let response = harness.get("/_liyasa/search?q=install").await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        harness.state.ingest.depth(),
        before,
        "a call that never reached the index is not a search"
    );
}
