//! The assistant's two job registrations, driven end to end (AST-01, AST-22,
//! RFC 1404, defect 130).
//!
//! These run the worker rather than asserting `kinds()` contains a name. The
//! doc comment on `kinds()` already asserted four registrations existed while
//! the body was `&[]`, so "the registration is there" is precisely the claim
//! that was worthless.
//!
//! The end-to-end shape also catches the failure a careful registration cannot:
//! the job name is a string shared by two crates that cannot depend on each
//! other, and a rename on either side leaves the job *orphaned* — warned about
//! at startup and in readiness, and never failing.

use liyasa_ai::indexing::{JOB_NAME, JobPayload};
use liyasa_core::store::JobState;
use liyasa_server::deploy::queue::EMBED_JOB;
use liyasa_server::routes::work;
use liyasa_store::Enqueue;
use liyasa_tests::server::Harness;
use serde_json::json;

/// `liyasa-ai` and `liyasa-server` each hold this name and neither can depend
/// on the other, so nothing but a test can hold them together.
///
/// `liyasa_ai::indexing::JOB_NAME`'s doc comment used to read "Named here so
/// the server and this crate cannot disagree about it" while the two values
/// were `assistant.index` and `assistant.embed`. A sentence is not a
/// mechanism; this is.
#[test]
fn the_two_crates_spell_the_embedding_job_the_same_way() {
    assert_eq!(
        JOB_NAME, EMBED_JOB,
        "a rename on either side orphans the job: it is warned about at startup \
         and in readiness, and never fails"
    );
}

/// The payload `queue_embedding` actually builds must deserialize into the
/// struct the handler reads.
///
/// This is the half no amount of careful naming protects, and it is worse than
/// the orphan case: the names agree, the handler is dispatched, and it fails
/// its attempts to `dead` on every deploy.
#[test]
fn the_handler_reads_the_payload_the_deploy_queue_sends() {
    // The shape from deploy::queue::queue_embedding.
    let sent = json!({ "buildId": "b_01J", "project": "p_01J" });
    let payload: JobPayload =
        serde_json::from_value(sent).expect("the enqueued payload is readable by the handler");
    assert_eq!(payload.deployment, "b_01J");
    assert_eq!(payload.project, "p_01J");
    assert!(
        payload.routes.is_empty(),
        "a fresh build names no routes, and empty already means the whole site"
    );
}

#[tokio::test]
async fn the_embedding_job_is_claimed_and_run_rather_than_orphaned() {
    let (harness, _site) = Harness::serving("assistant-embed-job").await;
    let store = harness.state.store.clone().expect("a store");

    // Enqueued exactly as the deploy path does it.
    store
        .jobs_typed()
        .enqueue(&Enqueue {
            payload: json!({ "buildId": "b_01J", "project": "p_01J" }),
            ..Enqueue::new(EMBED_JOB, "p_01J:b_01J")
        })
        .await
        .expect("the deploy queues an embedding");

    let ran = work::run_once(&harness.state, work::kinds(), "replica-a")
        .await
        .expect("a worker pass");
    assert_eq!(ran, 1, "the registered handler claimed and ran the row");

    // Not orphaned: the whole point of defect 130.
    let report = work::registered(&harness.state, work::kinds())
        .await
        .expect("a report");
    assert!(
        !report.orphaned.iter().any(|name| name == EMBED_JOB),
        "the embedding job is registered and must not be reported orphaned: {:?}",
        report.orphaned
    );
    assert!(report.handled.iter().any(|name| name == EMBED_JOB));
}

#[tokio::test]
async fn a_payload_the_handler_cannot_read_fails_rather_than_skipping() {
    // A producer defect is not a missing precondition. Skipping would complete
    // the row and say the deploy was embedded.
    let (harness, _site) = Harness::serving("assistant-embed-bad-payload").await;
    let store = harness.state.store.clone().expect("a store");
    store
        .jobs_typed()
        .enqueue(&Enqueue {
            payload: json!({ "wrong": "shape" }),
            ..Enqueue::new(EMBED_JOB, "p_01J:b_bad")
        })
        .await
        .expect("a row");

    work::run_once(&harness.state, work::kinds(), "replica-a")
        .await
        .expect("a pass");

    let rows = store
        .jobs_typed()
        .list(&Default::default(), liyasa_core::store::Page::default())
        .await
        .expect("the jobs are listable");
    let row = rows
        .iter()
        .find(|j| j.key == "p_01J:b_bad")
        .expect("the row survives");
    assert_ne!(
        row.state,
        JobState::Done,
        "a malformed payload must not complete as though it embedded: {row:?}"
    );
    assert!(
        row.error.as_deref().unwrap_or("").contains("payload"),
        "the failure names the payload: {:?}",
        row.error
    );
}

#[tokio::test]
async fn the_retention_sweep_is_registered_and_runs() {
    let (harness, _site) = Harness::serving("assistant-retention-job").await;
    let store = harness.state.store.clone().expect("a store");
    store
        .jobs_typed()
        .enqueue(&Enqueue::new(
            liyasa_server::assistant::RETENTION_JOB,
            "day-20000",
        ))
        .await
        .expect("a row");

    let ran = work::run_once(&harness.state, work::kinds(), "replica-a")
        .await
        .expect("a pass");
    assert_eq!(ran, 1);

    let report = work::registered(&harness.state, work::kinds())
        .await
        .expect("a report");
    assert!(
        !report
            .orphaned
            .iter()
            .any(|name| name == liyasa_server::assistant::RETENTION_JOB),
        "{:?}",
        report.orphaned
    );
}

/// The sweep's timer enqueues at most one row per day however many replicas
/// fire, because the key is the day bucket (RFC 1404).
#[tokio::test]
async fn the_sweep_timer_enqueues_one_row_per_day_across_replicas() {
    let (harness, _site) = Harness::serving("assistant-retention-timer").await;
    let store = harness.state.store.clone().expect("a store");

    // Over the REGISTERED kinds rather than a synthetic one, which is what
    // this adds to the worker's own timer test: that the sweep this package
    // registered is the one that de-duplicates.
    let first = work::fire_timers(&harness.state, work::kinds())
        .await
        .expect("a tick");
    let second = work::fire_timers(&harness.state, work::kinds())
        .await
        .expect("a second replica's tick");
    assert_eq!((first, second), (1, 0), "rows added, not ticks taken");

    let rows = store
        .jobs_typed()
        .list(&Default::default(), liyasa_core::store::Page::default())
        .await
        .expect("the jobs are listable");
    let sweeps = rows
        .iter()
        .filter(|j| j.name == liyasa_server::assistant::RETENTION_JOB)
        .count();
    assert_eq!(sweeps, 1, "exactly one live row per day bucket");
}
