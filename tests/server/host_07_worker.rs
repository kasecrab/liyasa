//! The job worker (RFC 1404, HOST-07, AST-01, AST-22).
//!
//! The job store had leases, heartbeats, de-duplication and a backoff ladder
//! and nothing claimed a row: across the workspace the only caller of `claim`
//! outside a test was the store's own facade shim. These assertions are about
//! the thing that turns the handle.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use liyasa_core::store::JobState;
use liyasa_server::routes::work::{self, Outcome, Trigger};
use liyasa_store::Enqueue;
use liyasa_tests::server::{Harness, Setup};
use serde_json::json;

/// A handler that counts its runs, so "exactly once" is a number and not a
/// shape.
static RUNS: AtomicUsize = AtomicUsize::new(0);

fn counting<'a>(
    _: &'a Arc<liyasa_server::routes::AppState>,
    _: &'a liyasa_store::records::JobRecord,
) -> work::Run<'a> {
    Box::pin(async move {
        RUNS.fetch_add(1, Ordering::SeqCst);
        Outcome::Done(json!({ "ok": true }))
    })
}

fn failing<'a>(
    _: &'a Arc<liyasa_server::routes::AppState>,
    _: &'a liyasa_store::records::JobRecord,
) -> work::Run<'a> {
    Box::pin(async move { Outcome::Failed("the provider refused".to_owned()) })
}

fn skipping<'a>(
    _: &'a Arc<liyasa_server::routes::AppState>,
    _: &'a liyasa_store::records::JobRecord,
) -> work::Run<'a> {
    Box::pin(async move { Outcome::Skipped("no model provider is configured".to_owned()) })
}

#[tokio::test]
async fn the_worker_runs_a_queued_job_and_records_its_result() {
    RUNS.store(0, Ordering::SeqCst);
    let (harness, _site) = Harness::serving("worker-runs").await;
    let store = harness.state.store.clone().expect("a store");
    let id = store
        .jobs_typed()
        .enqueue(&Enqueue::new("test.counting", "one"))
        .await
        .expect("a job")
        .id();

    let kinds = [work::JobKind {
        name: "test.counting",
        trigger: Trigger::Caller,
        run: counting,
    }];
    let ran = work::run_once(&harness.state, &kinds, "replica-a")
        .await
        .expect("a pass");
    assert_eq!(ran, 1, "one runnable job, one run");
    assert_eq!(RUNS.load(Ordering::SeqCst), 1);

    let job = store
        .jobs_typed()
        .get(&id)
        .await
        .expect("a read")
        .expect("the job");
    assert_eq!(job.state, JobState::Done);
    assert_eq!(job.result, Some(json!({ "ok": true })));
    assert_eq!(job.worker, None, "a finished job holds no lease");

    // A second pass has nothing to do rather than running it again.
    assert_eq!(
        work::run_once(&harness.state, &kinds, "replica-a")
            .await
            .expect("a pass"),
        0
    );
    assert_eq!(RUNS.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_failing_job_goes_back_on_the_ladder_and_a_skipped_one_does_not() {
    let (harness, _site) = Harness::serving("worker-outcomes").await;
    let store = harness.state.store.clone().expect("a store");

    let failed = store
        .jobs_typed()
        .enqueue(&Enqueue::new("test.failing", "one"))
        .await
        .expect("a job")
        .id();
    let skipped = store
        .jobs_typed()
        .enqueue(&Enqueue::new("test.skipping", "one"))
        .await
        .expect("a job")
        .id();

    let kinds = [
        work::JobKind {
            name: "test.failing",
            trigger: Trigger::Caller,
            run: failing,
        },
        work::JobKind {
            name: "test.skipping",
            trigger: Trigger::Caller,
            run: skipping,
        },
    ];
    work::run_once(&harness.state, &kinds, "replica-a")
        .await
        .expect("a pass");
    work::run_once(&harness.state, &kinds, "replica-a")
        .await
        .expect("a pass");

    let failed = store
        .jobs_typed()
        .get(&failed)
        .await
        .expect("a read")
        .expect("the job");
    assert_eq!(failed.state, JobState::Queued, "a failure retries");
    assert!(
        failed.run_at > liyasa_store::now_ms(),
        "and waits for the backoff"
    );
    assert_eq!(failed.error.as_deref(), Some("the provider refused"));

    let skipped = store
        .jobs_typed()
        .get(&skipped)
        .await
        .expect("a read")
        .expect("the job");
    assert_eq!(
        skipped.state,
        JobState::Done,
        "a precondition this instance cannot meet is not a failure to retry"
    );
    assert!(
        skipped
            .result
            .as_ref()
            .and_then(|r| r.get("skipped"))
            .and_then(|s| s.as_str())
            .is_some_and(|s| s.contains("model provider")),
        "the reason is on the row where `liyasa jobs list` shows it: {:?}",
        skipped.result
    );
}

#[tokio::test]
async fn a_job_no_handler_claims_is_released_rather_than_failed() {
    let (harness, _site) = Harness::serving("worker-unknown").await;
    let store = harness.state.store.clone().expect("a store");
    let id = store
        .jobs_typed()
        .enqueue(&Enqueue::new("some.newer.package", "one"))
        .await
        .expect("a job")
        .id();

    // A binary older than whoever enqueued must not burn the ladder to `dead`.
    let kinds: [work::JobKind; 0] = [];
    work::run_once(&harness.state, &kinds, "replica-a")
        .await
        .expect("a pass");

    let job = store
        .jobs_typed()
        .get(&id)
        .await
        .expect("a read")
        .expect("the job");
    assert_eq!(
        job.state,
        JobState::Queued,
        "still runnable by a newer binary"
    );
    assert_eq!(job.attempts, 0, "the release gave the attempt back");
    assert!(job.error.is_none());
}

#[tokio::test]
async fn a_timer_enqueues_one_row_however_many_replicas_fire_it() {
    let (harness, _site) = Harness::serving("worker-timer").await;
    let store = harness.state.store.clone().expect("a store");

    fn daily(_: &Arc<liyasa_server::routes::AppState>) -> Option<Enqueue> {
        Some(Enqueue::new("test.sweep", "bucket"))
    }
    let kinds = [work::JobKind {
        name: "test.sweep",
        trigger: Trigger::Scheduled(daily),
        run: counting,
    }];

    // Three replicas, each firing its own timer for the same bucket. The
    // at-most-once guarantee is the store's unique index over live rows, not
    // anything the worker does (RFC 1404).
    for _ in 0..3 {
        work::fire_timers(&harness.state, &kinds)
            .await
            .expect("a tick");
    }
    assert_eq!(
        store
            .jobs_typed()
            .depth(Some("test.sweep"))
            .await
            .expect("the depth"),
        1,
        "three replicas, one row"
    );
}

#[tokio::test]
async fn a_deployment_enqueues_what_packages_registered_for_it() {
    let (harness, _site) = Harness::serving("worker-deploy").await;
    let store = harness.state.store.clone().expect("a store");

    fn after_deploy(
        _: &Arc<liyasa_server::routes::AppState>,
        event: &serde_json::Value,
    ) -> Option<Enqueue> {
        let build = event.get("buildId")?.as_str()?;
        let mut enqueue = Enqueue::new("test.reindex", build);
        enqueue.payload = json!({ "buildId": build });
        Some(enqueue)
    }
    let kinds = [work::JobKind {
        name: "test.reindex",
        trigger: Trigger::DeploymentSucceeded(after_deploy),
        run: counting,
    }];

    work::on_deployment(&harness.state, &kinds, &json!({ "buildId": "blake3:abc" }))
        .await
        .expect("the event");
    assert_eq!(
        store
            .jobs_typed()
            .depth(Some("test.reindex"))
            .await
            .expect("the depth"),
        1
    );

    // The same deployment announced twice is one re-index, because the build
    // is the de-duplication key.
    work::on_deployment(&harness.state, &kinds, &json!({ "buildId": "blake3:abc" }))
        .await
        .expect("the event");
    assert_eq!(
        store
            .jobs_typed()
            .depth(Some("test.reindex"))
            .await
            .expect("the depth"),
        1
    );

    // A different build is different work.
    work::on_deployment(&harness.state, &kinds, &json!({ "buildId": "blake3:def" }))
        .await
        .expect("the event");
    assert_eq!(
        store
            .jobs_typed()
            .depth(Some("test.reindex"))
            .await
            .expect("the depth"),
        2
    );
}

#[tokio::test]
async fn an_instance_with_no_store_runs_nothing_rather_than_failing() {
    let (harness, _) = Harness::new(Setup {
        with_store: false,
        ..Setup::new("worker-nostore")
    })
    .await;
    let kinds = [work::JobKind {
        name: "test.counting",
        trigger: Trigger::Caller,
        run: counting,
    }];
    assert_eq!(
        work::run_once(&harness.state, &kinds, "replica-a")
            .await
            .expect("a pass on an instance with nothing to claim from"),
        0
    );
}

#[test]
fn every_registered_kind_is_named_once() {
    let names: Vec<&str> = work::kinds().iter().map(|k| k.name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        names.len(),
        sorted.len(),
        "a job name is registered twice, so two handlers would race for it: {names:?}"
    );
}

#[tokio::test]
async fn work_nobody_can_run_is_named_rather_than_silently_queued() {
    // The dominant defect in this project is a complete mechanism with no
    // caller. A registry that starts cleanly and says nothing would be the
    // next one, so the absence has to be visible from outside (RFC 1404).
    let (harness, _site) = Harness::serving("worker-orphans").await;
    let store = harness.state.store.clone().expect("a store");
    store
        .jobs_typed()
        .enqueue(&Enqueue::new("assistant.index", "build-1"))
        .await
        .expect("a job some package enqueued");
    store
        .jobs_typed()
        .enqueue(&Enqueue::new("test.counting", "one"))
        .await
        .expect("a job this binary can run");

    let kinds = [work::JobKind {
        name: "test.counting",
        trigger: Trigger::Caller,
        run: counting,
    }];
    let registered = work::registered(&harness.state, &kinds)
        .await
        .expect("a report");
    assert_eq!(registered.handled, ["test.counting"]);
    assert_eq!(
        registered.orphaned,
        ["assistant.index"],
        "a queued name with no handler is work nobody can do and must be named"
    );

    // A job that is handled does not show up as orphaned once it has run.
    work::run_once(&harness.state, &kinds, "replica-a")
        .await
        .expect("a pass");
    let after = work::registered(&harness.state, &kinds)
        .await
        .expect("a report");
    assert_eq!(after.orphaned, ["assistant.index"]);
}

#[tokio::test]
async fn readiness_names_the_jobs_this_binary_cannot_run() {
    let (harness, _site) = Harness::serving("worker-ready").await;
    let store = harness.state.store.clone().expect("a store");

    let clean = liyasa_tests::server::body_json(harness.get("/_liyasa/ready").await).await;
    assert_eq!(
        clean["orphanedJobs"].as_array().map(Vec::len),
        Some(0),
        "nothing is queued yet: {clean}"
    );

    // A name nothing will ever register. It used to be "assistant.retention",
    // which stopped being orphaned the moment WP-18 registered it (defect 130)
    // and took this assertion red with it. The fixture has to be a name no
    // package can claim, or every future registration breaks this test for
    // being correct.
    store
        .jobs_typed()
        .enqueue(&Enqueue::new("nobody.handles.this", "day-1"))
        .await
        .expect("a job");
    let body = liyasa_tests::server::body_json(harness.get("/_liyasa/ready").await).await;
    let orphaned: Vec<&str> = body["orphanedJobs"]
        .as_array()
        .expect("the list")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        orphaned.contains(&"nobody.handles.this"),
        "an operator asking why nothing is indexing must be told: {body}"
    );
    assert!(
        body["checks"]
            .as_array()
            .expect("the checks")
            .iter()
            .any(|c| c["name"] == "jobs" && c["status"] == "warn"),
        "{body}"
    );
}

#[tokio::test]
async fn releasing_an_unhandled_job_leaves_another_workers_leases_alone() {
    // `Jobs::release` is by worker, so an unhandled job used to hand back
    // every row that worker held. Harmless only while the loop holds one at a
    // time, which made the `break` load-bearing without saying so (WP-16).
    // `release_one` is scoped to the id; this is the input that tells the two
    // apart.
    let (harness, _site) = Harness::serving("worker-release-scope").await;
    let store = harness.state.store.clone().expect("a store");
    let held = store
        .jobs_typed()
        .enqueue(&Enqueue::new("test.counting", "held"))
        .await
        .expect("a job")
        .id();

    // Claim it BEFORE the unhandled job exists, so which row this takes does
    // not depend on the claim order of two rows with the same priority.
    let first = store
        .jobs_typed()
        .claim("replica-a", Duration::from_secs(600))
        .await
        .expect("a claim")
        .expect("a job");
    assert_eq!(first.id, held);

    store
        .jobs_typed()
        .enqueue(&Enqueue::new("some.newer.package", "one"))
        .await
        .expect("a job");

    // The same worker then meets a job it has no handler for.
    let kinds: [work::JobKind; 0] = [];
    work::run_once(&harness.state, &kinds, "replica-a")
        .await
        .expect("a pass");

    let still_held = store
        .jobs_typed()
        .get(&held)
        .await
        .expect("a read")
        .expect("the job");
    assert_eq!(
        still_held.state,
        JobState::Leased,
        "an unrelated in-flight job must not be re-queued underneath its handler"
    );
    assert_eq!(still_held.worker.as_deref(), Some("replica-a"));
}
