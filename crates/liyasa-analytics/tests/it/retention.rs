//! ANA-06, against a real analytics database.

use liyasa_analytics::retention::{self, Policy, Total, TotalsSink};
use liyasa_core::net::BoxFut;
use liyasa_core::store::StoreError;
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::support::{DAY, Event, HOUR, T0, analytics};

/// A sink that keeps what it was handed, so a test can check the number was
/// carried rather than merely that something was called.
#[derive(Default)]
struct Collected {
    totals: std::sync::Mutex<Vec<Total>>,
}

impl TotalsSink for Collected {
    fn absorb<'a>(&'a self, totals: &'a [Total]) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async move {
            self.totals
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend_from_slice(totals);
            Ok(())
        })
    }
}

/// A sink that fails, because a sweep must not delete when the carry failed.
struct Refuses;

impl TotalsSink for Refuses {
    fn absorb<'a>(&'a self, _totals: &'a [Total]) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async { Err(StoreError::Io("the totals store is down".to_owned())) })
    }
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let statement = format!("SELECT COUNT(*) AS n FROM {table}");
    sqlx::query(sqlx::AssertSqlSafe(statement))
        .fetch_one(pool)
        .await
        .expect("a count")
        .try_get("n")
        .expect("a number")
}

#[test]
fn the_boundaries_follow_the_policy() {
    let policy = Policy::default();
    assert_eq!(policy.raw_days, 90);
    assert_eq!(policy.aggregate_months, 13);
    assert_eq!(policy.raw_boundary(T0), T0 - 90 * DAY);
    // Thirteen mean Gregorian months is 396 days.
    assert_eq!(policy.aggregate_boundary(T0), T0 - 396 * DAY);

    let short = Policy {
        raw_days: 7,
        aggregate_months: 1,
    };
    assert_eq!(short.raw_boundary(T0), T0 - 7 * DAY);
    assert_eq!(short.aggregate_boundary(T0), T0 - 30 * DAY);
}

#[tokio::test]
async fn a_sweep_deletes_raw_rows_past_the_window_and_keeps_the_rest() {
    let events = vec![
        Event::new("page_view", "/old", T0 - 100 * DAY).build(),
        Event::new("page_view", "/old", T0 - 91 * DAY).build(),
        Event::new("page_view", "/kept", T0 - 89 * DAY).build(),
        Event::new("page_view", "/kept", T0 - HOUR).build(),
    ];
    let (_dir, writer) = analytics("retention-raw", events).await;
    assert_eq!(count(writer.pool(), "event").await, 4);

    let report = retention::sweep(writer.pool(), Policy::default(), T0, None)
        .await
        .expect("a sweep");
    assert_eq!(report.raw_deleted, 2);
    assert_eq!(count(writer.pool(), "event").await, 2);
    assert_eq!(report.raw_boundary, T0 - 90 * DAY);
    assert_eq!(report.at, T0);
    assert_eq!(report.policy, Policy::default());
    assert!(report.deleted_anything());

    // Running it again deletes nothing: the pass is idempotent.
    let again = retention::sweep(writer.pool(), Policy::default(), T0, None)
        .await
        .expect("a sweep");
    assert_eq!(again.raw_deleted, 0);
    assert!(!again.deleted_anything());
}

#[tokio::test]
async fn the_rollup_survives_a_raw_sweep() {
    // This is the whole reason the rollup exists: a count from thirteen months
    // ago is still answerable after its raw rows are gone.
    let events = vec![
        Event::new("page_view", "/a", T0 - 100 * DAY).build(),
        Event::new("page_view", "/a", T0 - 100 * DAY).build(),
    ];
    let (_dir, writer) = analytics("retention-rollup-survives", events).await;
    retention::sweep(writer.pool(), Policy::default(), T0, None)
        .await
        .expect("a sweep");
    assert_eq!(count(writer.pool(), "event").await, 0);
    assert_eq!(count(writer.pool(), "agg_hour").await, 1);
    let row = sqlx::query("SELECT count AS n FROM agg_hour")
        .fetch_one(writer.pool())
        .await
        .expect("the rollup row");
    assert_eq!(row.try_get::<i64, _>("n").expect("a count"), 2);
}

#[tokio::test]
async fn a_sweep_with_nowhere_to_put_a_total_keeps_the_rows_and_says_so() {
    let events = vec![Event::new("page_view", "/a", T0 - 400 * DAY).build()];
    let (_dir, writer) = analytics("retention-no-sink", events).await;

    let report = retention::sweep(writer.pool(), Policy::default(), T0, None)
        .await
        .expect("a sweep");
    assert_eq!(report.rollup_deleted, 0);
    assert_eq!(report.totals_absorbed, 0);
    assert_eq!(
        report.rollup_kept_for_want_of_a_sink, 1,
        "ANA-06 keeps totals forever, and there is no totals table (RFC 1703)"
    );
    assert_eq!(
        count(writer.pool(), "agg_hour").await,
        1,
        "the row is still there, so the number is not lost"
    );
}

#[tokio::test]
async fn a_sweep_deletes_the_rollup_only_after_its_totals_are_carried() {
    let events = vec![
        Event::new("page_view", "/a", T0 - 400 * DAY).build(),
        Event::new("page_view", "/a", T0 - 399 * DAY).build(),
        Event::new("page_view", "/b", T0 - 400 * DAY).build(),
        Event::new("page_view", "/a", T0 - HOUR).build(),
    ];
    let (_dir, writer) = analytics("retention-with-sink", events).await;
    let sink = Collected::default();

    let report = retention::sweep(writer.pool(), Policy::default(), T0, Some(&sink))
        .await
        .expect("a sweep");
    assert_eq!(report.totals_absorbed, 2, "/a and /b");
    assert_eq!(report.rollup_deleted, 3, "three expired hourly rows");
    assert_eq!(report.rollup_kept_for_want_of_a_sink, 0);

    let carried = sink.totals.lock().expect("the totals").clone();
    let a = carried.iter().find(|t| t.route == "/a").expect("/a");
    assert_eq!(
        a.count, 2,
        "the two expired views of /a were folded into one total, not dropped"
    );
    assert_eq!(a.site, "acme-docs");
    assert_eq!(a.kind, "page_view");
    assert_eq!(a.caller_kind, "human");
    assert_eq!(
        carried.iter().find(|t| t.route == "/b").expect("/b").count,
        1
    );

    // The recent row is untouched.
    assert_eq!(count(writer.pool(), "agg_hour").await, 1);
}

#[tokio::test]
async fn a_sink_that_fails_stops_the_deletion() {
    let events = vec![Event::new("page_view", "/a", T0 - 400 * DAY).build()];
    let (_dir, writer) = analytics("retention-sink-fails", events).await;

    let error = retention::sweep(writer.pool(), Policy::default(), T0, Some(&Refuses))
        .await
        .expect_err("the sweep fails with the sink");
    assert!(matches!(error, StoreError::Io(_)));
    assert_eq!(
        count(writer.pool(), "agg_hour").await,
        1,
        "nothing was deleted, because nothing took the total"
    );
}

#[tokio::test]
async fn the_horizon_says_how_far_back_each_table_can_answer() {
    let events = vec![
        Event::new("page_view", "/a", T0 - 400 * DAY).build(),
        Event::new("page_view", "/a", T0 - HOUR).build(),
    ];
    let (_dir, writer) = analytics("retention-horizon", events).await;

    let before = retention::horizon(writer.pool(), Policy::default(), T0)
        .await
        .expect("a horizon");
    assert_eq!(before.raw_from, T0 - 400 * DAY);
    assert_eq!(before.rollup_from, T0 - 400 * DAY);

    retention::sweep(writer.pool(), Policy::default(), T0, None)
        .await
        .expect("a sweep");
    let after = retention::horizon(writer.pool(), Policy::default(), T0)
        .await
        .expect("a horizon");
    assert_eq!(
        after.raw_from,
        T0 - HOUR,
        "raw events now only reach back to the most recent one left"
    );
    assert_eq!(
        after.rollup_from,
        T0 - 400 * DAY,
        "and the rollup still reaches back further, which is what the chart labels"
    );
}

#[tokio::test]
async fn a_report_serialises_to_the_audit_record_a_job_stores() {
    let (_dir, writer) = analytics(
        "retention-audit",
        vec![Event::new("page_view", "/a", T0 - 100 * DAY).build()],
    )
    .await;
    let report = retention::sweep(writer.pool(), Policy::default(), T0, None)
        .await
        .expect("a sweep");
    let value = serde_json::to_value(&report).expect("a job result");
    assert_eq!(value["rawDeleted"], 1);
    assert_eq!(value["rawBoundary"], T0 - 90 * DAY);
    assert_eq!(value["policy"]["rawDays"], 90);
    assert_eq!(value["at"], T0);
    let back: retention::SweepReport = serde_json::from_value(value).expect("it round-trips");
    assert_eq!(back, report);
}
