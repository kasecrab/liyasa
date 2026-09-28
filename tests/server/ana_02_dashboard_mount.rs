//! The analytics subtree: that the dashboard's routes are reachable at all,
//! and that ANA-02's schema is reachable by the caller who needs it.
//!
//! Eighteen analytics endpoints were written, unit-tested and driven as real
//! HTTP in `liyasa-analytics`'s own tests for weeks while nothing mounted
//! them — the eighth time this project has shipped a correct mechanism with
//! no caller. These tests are the mechanical answer to "is it wired": they
//! assert the router answers, not that the numbers are right, which is
//! `liyasa-analytics`'s job and already done there.

use http::StatusCode;
use liyasa_tests::server::{Harness, Setup, expect_status};

fn with_analytics(name: &str) -> Setup {
    Setup {
        analytics: true,
        ..Setup::new(name)
    }
}

#[tokio::test]
async fn the_dashboard_endpoints_are_mounted_when_there_is_a_database() {
    let (harness, _site) = Harness::new(with_analytics("ana02-mounted")).await;

    let response = harness.get("/_liyasa/api/v1/analytics/totals").await;
    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "the analytics subtree did not mount; the dashboard is unreachable"
    );
}

#[tokio::test]
async fn an_instance_without_an_analytics_database_skips_rather_than_failing() {
    let (harness, _site) = Harness::serving("ana02-absent").await;

    expect_status(
        harness.get("/_liyasa/api/v1/analytics/totals").await,
        StatusCode::NOT_FOUND,
    );
    let record = harness
        .mounted
        .iter()
        .find(|record| record.name == "analytics")
        .expect("the analytics subtree is declared whether or not it mounts");
    assert!(
        record.skipped.is_some(),
        "an instance with no analytics database must say why it mounted nothing, \
         not mount an empty router"
    );
}

/// ANA-02. A static-site collector validates its events against this document
/// before it is allowed to post any, so it holds no dashboard credential by
/// construction. Inside the guarded layer this answers 401 to exactly the
/// caller that needs it, which is why it is a second subtree entry.
#[tokio::test]
async fn the_event_schema_is_served_without_a_dashboard_credential() {
    let (harness, _site) = Harness::serving("ana02-schema").await;

    let response = expect_status(
        harness.get(liyasa_analytics::schema::PATH).await,
        StatusCode::OK,
    );
    assert_eq!(
        liyasa_tests::server::header(&response, "content-type"),
        Some("application/schema+json")
    );
}
