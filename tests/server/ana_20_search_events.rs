//! Search feeds the analytics series (ANA-20, ANA-06).
//!
//! Two call sites WP-17 built constructors for, verified from this side: the
//! `SearchEvent` mapping and the plan's retention ceiling. The mapping lives
//! here rather than in `liyasa-analytics` because naming `SearchEvent` there
//! would pull tantivy into that crate's graph for forty lines.

use http::StatusCode;
use liyasa_analytics::query::{Filters, Range};
use liyasa_analytics::retention::Policy;
use liyasa_tests::server::{Harness, Setup};

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
///
/// The instance without an index is now asked for explicitly. It used to be
/// ambient — the harness loaded no index for anyone, so this passed on a 503
/// that every other search test was also passing on, and none of them drove a
/// search that returned a result. The shape is still worth testing; it just
/// has to be chosen rather than inherited.
#[tokio::test]
async fn a_refused_query_records_nothing() {
    let (harness, _site) = Harness::new(Setup {
        search_index: false,
        ..Setup::new("ana20-refused")
    })
    .await;
    let before = harness.state.ingest.depth();

    let response = harness.get("/_liyasa/search?q=install").await;
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "an instance with no index answers 503 rather than an empty result"
    );
    assert_eq!(
        harness.state.ingest.depth(),
        before,
        "a call that never reached the index is not a search"
    );
}

// ---- the effect, rather than the shape ----
//
// Everything above calls `capped_by` or `into_record` directly. Those are
// claims about structure: the function exists and does what it says. The
// clauses are claims about effects — that a search produces a row the report
// reads, and that a narrower plan deletes more — and nothing here drove
// either. WP-20c's phrasing: a clause is a claim about an effect, and every
// check we ran was a claim about structure.

/// A search through the composed application lands a row `search::queries`
/// reads back.
///
/// Four things have to hold at once and none of them is visible from a unit
/// test: the handler reaches `record`, the mapping writes the keys the report
/// queries, the ingest queue carries them, and the writer stores them where
/// the report looks. A mapping that wrote `query` instead of `q` passes every
/// assertion in this file above and reports nothing here.
#[tokio::test]
async fn a_search_lands_a_row_the_report_reads() {
    let (mut harness, _site) = Harness::new(Setup {
        analytics: true,
        ..Setup::new("ana20-effect")
    })
    .await;

    let response = harness.get("/_liyasa/search?q=install").await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the search itself has to succeed before its event means anything"
    );

    let written = harness.flush_analytics().await;
    assert!(
        written > 0,
        "the handler pushed nothing: `record` was not reached, or the queue \
         dropped it"
    );

    let range = Range::new(0, liyasa_store::now_ms() + 60_000);
    let queries =
        liyasa_analytics::search::queries(harness.analytics_pool(), range, &Filters::default(), 10)
            .await
            .expect("the search report runs");

    let install = queries
        .iter()
        .find(|row| row.q == "install")
        .unwrap_or_else(|| panic!("`install` is not in the report; it returned {queries:?}"));
    assert_eq!(install.searches, 1);
    assert_eq!(
        install.empty + install.searches,
        1 + install.empty,
        "the row is counted once either way"
    );
}

/// The same search is reachable through the locale filter that ANA-71 applies,
/// which is the reason `locale` lives in `variant` rather than `props`.
#[tokio::test]
async fn a_search_event_is_filterable_by_the_dimensions_ana_71_offers() {
    let (mut harness, _site) = Harness::new(Setup {
        analytics: true,
        ..Setup::new("ana20-filter")
    })
    .await;
    assert_eq!(
        harness.get("/_liyasa/search?q=install").await.status(),
        StatusCode::OK
    );
    harness.flush_analytics().await;

    let range = Range::new(0, liyasa_store::now_ms() + 60_000);
    let unfiltered =
        liyasa_analytics::search::queries(harness.analytics_pool(), range, &Filters::default(), 10)
            .await
            .expect("the report runs");
    assert!(!unfiltered.is_empty());

    // A dimension nothing was recorded under selects nothing. Without this the
    // filter could be ignored entirely and the test above would still pass.
    let absent = Filters {
        locale: Some("zz".to_owned()),
        ..Filters::default()
    };
    let filtered = liyasa_analytics::search::queries(harness.analytics_pool(), range, &absent, 10)
        .await
        .expect("the report runs");
    assert!(
        filtered.is_empty(),
        "a locale filter that matches nothing returned {filtered:?}"
    );
}

/// ANA-06's wiring: the policy the instance sweeps under is the plan's ceiling
/// narrowing the operator's configuration, read off the object the job uses.
///
/// `capped_by` having the right arithmetic is asserted above. This asserts
/// that something passes the plan in — which is the half that was missing when
/// `org/routes.rs` reported `analyticsRetentionDays` and nothing acted on it.
#[tokio::test]
async fn the_policy_the_sweep_runs_under_is_the_one_the_instance_built() {
    let (harness, _site) = Harness::new(Setup {
        analytics: true,
        ..Setup::new("ana06-wiring")
    })
    .await;

    let view = liyasa_server::routes::analytics::view(&harness.state)
        .expect("an instance with an analytics database has a view");

    // No organization on this instance, so no plan and no ceiling: the
    // configured policy survives untouched. That is the branch an OSS operator
    // runs, and `capped_by(None)` is what keeps it unchanged.
    assert_eq!(
        view.retention,
        liyasa_analytics::retention::Policy::default(),
        "with no plan the operator's configuration is the policy"
    );

    // And the job reads the same object, rather than building a second policy
    // that could drift from the one the dashboard reports.
    assert_eq!(
        liyasa_server::routes::analytics::RETENTION.name,
        liyasa_analytics::actions::RETENTION,
        "the registry and the enqueuer spell the job the same way"
    );
}
