//! WP-17's scheduled work, adapted to the job registry (ANA-06, ANA-42).
//!
//! `liyasa-analytics` cannot register its own jobs: `JobKind.run` names
//! `AppState`, which lives here, so a handler in that crate would be a cycle —
//! the same reason its routes need an adapter (RFC 1403). The passes
//! themselves are written and tested there; what is here is the seam.

use std::sync::Arc;

use liyasa_analytics::serve::Analytics;
use liyasa_store::jobs::Enqueue;

use liyasa_store::records::JobRecord;

use super::AppState;
use super::work::{JobKind, Outcome, Run};

pub const RETENTION_JOB: &str = liyasa_analytics::actions::RETENTION;
pub const DIGEST_JOB: &str = liyasa_analytics::actions::DIGEST;

/// The two registrations, as consts rather than literals in `work.rs`.
///
/// That file is `merge=union`, so a multi-line entry there can interleave with
/// another package's and leave something `cargo fmt` cannot parse — which
/// redded eight branches at the format step on 2026-09-28 (defect 192). A
/// const here is named by one short line there, and the reasoning stays beside
/// the handlers instead of being stranded by somebody else's append.
pub const RETENTION: JobKind = JobKind::scheduled(RETENTION_JOB, retention_due, run_retention);
pub const DIGEST: JobKind = JobKind::scheduled(DIGEST_JOB, digest_due, run_digest);

/// The dashboard's view of this instance, or `None` when it opened no
/// analytics database.
///
/// One constructor for the routes and the jobs both: two would be two sets of
/// defaults to keep in step, and a retention pass running under a different
/// policy from the one the dashboard displays is a difference nobody would
/// see until the numbers had already gone.
pub fn view(app: &Arc<AppState>) -> Option<Analytics> {
    let store = app.store.clone()?;
    let pool = app.analytics_pool()?.clone();
    Some(Analytics {
        analytics: pool,
        app: store.clone_pool(),
        site: app.config.site.clone(),
        project: None,
        retention: liyasa_analytics::retention::Policy::default(),
        integrations: serde_json::Value::Null,
        pages: Vec::new(),
    })
}

const MS_PER_DAY: i64 = 24 * 60 * 60 * 1000;

/// One deletion pass a day, at most once across every replica (ANA-06).
///
/// The key is the day bucket, so every replica may fire and the store's unique
/// index over live rows leaves exactly one (RFC 1404).
pub fn retention_due(_state: &Arc<AppState>) -> Option<Enqueue> {
    Some(liyasa_analytics::actions::run_retention(
        liyasa_store::now_ms() / MS_PER_DAY,
    ))
}

/// The report is the point, not a side effect. ANA-06 asks for deletion to be
/// auditable, and `SweepReport` carries the policy it ran under, both
/// boundaries and the counts — so the job row *is* the audit record, readable
/// through `liyasa jobs list` with no second table. Discarding it would leave
/// the clause unmet however correct the deletion was.
pub fn run_retention<'a>(state: &'a Arc<AppState>, _job: &'a JobRecord) -> Run<'a> {
    Box::pin(async move {
        let Some(analytics) = view(state) else {
            return Outcome::Skipped("this instance has no analytics database".to_owned());
        };
        // `None` for the totals sink is deliberate (RFC 1703). Without one the
        // sweep deletes raw rows past the window and KEEPS expired rollups,
        // reporting how many it kept: ANA-06 keeps totals forever and there is
        // nowhere yet to put them, so a sweep can never be what loses an
        // all-time number. A non-zero `rollupKeptForWantOfASink` is the
        // designed outcome, not a failure to log.
        match liyasa_analytics::serve::sweep(&analytics, None).await {
            Ok(report) => match serde_json::to_value(&report) {
                Ok(value) => Outcome::Done(value),
                Err(e) => Outcome::Failed(format!("the sweep report did not serialise: {e}")),
            },
            Err(e) => Outcome::Failed(format!("the retention pass failed: {e}")),
        }
    })
}

/// One digest a week, keyed on the Monday it covers (ANA-42).
pub fn digest_due(_state: &Arc<AppState>) -> Option<Enqueue> {
    Some(liyasa_analytics::actions::send_digest(
        liyasa_analytics::digest::week_starting(liyasa_store::now_ms()),
        None,
    ))
}

/// Renders the digest and hands it to whatever sends it.
///
/// Nothing sends it yet, so this skips rather than failing: a job whose
/// precondition is a missing configuration would otherwise burn its whole
/// backoff ladder to reach `dead` and tell the operator nothing. Rendering
/// still runs, because a digest that cannot be built is a defect worth
/// reporting today and a destination that does not exist is not.
pub fn run_digest<'a>(state: &'a Arc<AppState>, job: &'a JobRecord) -> Run<'a> {
    Box::pin(async move {
        let Some(analytics) = view(state) else {
            return Outcome::Skipped("this instance has no analytics database".to_owned());
        };
        let week = job
            .payload
            .get("week_starting")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or_else(|| liyasa_analytics::digest::week_starting(liyasa_store::now_ms()));
        let filters = liyasa_analytics::query::Filters::default();
        match liyasa_analytics::serve::weekly_digest(&analytics, week, &filters).await {
            Ok(_digest) => Outcome::Skipped("no digest destination is configured".to_owned()),
            Err(e) => Outcome::Failed(format!("the weekly digest failed to render: {e}")),
        }
    })
}
