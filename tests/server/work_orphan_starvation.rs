//! A job this binary cannot run must not stop the jobs it can (RFC 1404).
//!
//! `run_up_to` claims one row at a time and `claim` has no name filter, so a
//! row whose name is in no `JobKind` is discovered only after it is already
//! leased. The loop releases it and `break`s, and the comment there says why:
//! "`claim` orders by priority and would hand back the same row." That is
//! true, and it is why `continue` is not the repair — `ran` counts only
//! handled jobs, so continuing would re-claim the same released row until the
//! limit without ever advancing.
//!
//! The cost is that every job sorting below the unrunnable one is never
//! examined. `release_one` restores `state = 'queued'` without moving `run_at`
//! and decrements `attempts`, so the row neither ages into `dead` nor moves
//! out of the way: the starvation is permanent until a binary registers the
//! name or someone deletes the row.
//!
//! Reachable from two shipped routes. `liyasa-analytics` enqueues
//! `agent.create_page` (ANA-20's "create page for this query") and
//! `agent.fix_page` (ANA-30's "ask the agent to fix"), and `routes/work.rs`
//! registers neither, so one operator click leaves the instance's retention
//! and digest passes unreachable.
//!
//! **Fixed by the first of those two**, in the commit this test lands in:
//! `Jobs::claim_runnable` takes the handled names and filters in SQL, so an
//! unrunnable row is never claimed, never released, and never ends a pass. The
//! other option — pushing `run_at` out in `release_one` — was rejected because
//! `run_at` is also what backoff schedules, and a column with two meanings is
//! a defect waiting for a reader; deferring a row would also churn its version
//! on every pass forever.
//!
//! The test stays written as the effect rather than as the mechanism, so it
//! still fails if a later change reintroduces starvation by another route.

use liyasa_server::deploy::queue::EMBED_JOB;
use liyasa_server::routes::work;
use liyasa_store::Enqueue;
use liyasa_tests::server::Harness;
use serde_json::json;

/// A name no package can ever register, so a later registration cannot
/// invalidate this test by making the code correct — the orphan-fixture trap
/// in CLAUDE.md. The two real unregistered names are deliberately not used
/// here: an assertion that `agent.create_page` is unhandled is a claim I want
/// to become false.
const UNRUNNABLE: &str = "nobody.handles.this";

#[tokio::test]
async fn an_unrunnable_job_does_not_starve_the_jobs_the_binary_can_run() {
    let (harness, _site) = Harness::serving("work-orphan-starvation").await;
    let store = harness.state.store.clone().expect("a store");
    let jobs = store.jobs_typed();

    // Priority rather than insertion order, so "sorts first" is decided by
    // `claim`'s `ORDER BY priority DESC` and not by how two ids happen to
    // compare. Any orphan that sorts first does this; priority is just the
    // one lever that says so without depending on id generation.
    jobs.enqueue(&Enqueue {
        priority: 1,
        ..Enqueue::new(UNRUNNABLE, "orphan")
    })
    .await
    .expect("a row nothing handles");

    // Queued exactly as the deploy path does it, so the handler reads it.
    jobs.enqueue(&Enqueue {
        payload: json!({ "buildId": "b_01J", "project": "p_01J" }),
        ..Enqueue::new(EMBED_JOB, "p_01J:b_01J")
    })
    .await
    .expect("a row this binary handles");

    let ran = work::run_once(&harness.state, work::kinds(), "replica-a")
        .await
        .expect("a worker pass");
    assert_eq!(
        ran, 1,
        "the runnable row below the orphan must be examined; 0 means the \
         orphan was claimed, released, and ended the pass"
    );

    // And the next pass does not recover: the released row kept its priority
    // and its `run_at`, so it is still what `claim` returns.
    let ran_again = work::run_once(&harness.state, work::kinds(), "replica-a")
        .await
        .expect("a second worker pass");
    assert_eq!(
        ran_again, 0,
        "nothing runnable is left, so a second pass runs nothing — if this is \
         1 the first pass was the one that starved"
    );

    // The orphan is reported as orphaned rather than silently absent, which is
    // the half defect 130 already built and which still works.
    let report = work::registered(&harness.state, work::kinds())
        .await
        .expect("a report");
    assert!(
        report.orphaned.iter().any(|name| name == UNRUNNABLE),
        "an unrunnable live row must be visible to readiness: {:?}",
        report.orphaned
    );
}
