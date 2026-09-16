//! HOST-07: job store semantics in every topology.
//!
//! Given three server replicas and a nightly verification schedule; when the
//! schedule fires; then exactly one run happens, a killed worker's lease
//! expires and another replica re-claims it, and a duplicate trigger with the
//! same key is not queued.
//!
//! The replicas here are three workers against one SQLite file rather than
//! three processes on Postgres (RFC 1402). The claim is a select followed by
//! a version-guarded update, which is the same statement on both backends, so
//! the contention this exercises is the contention a cluster has.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use http::StatusCode;
use liyasa_core::store::JobState;
use liyasa_store::{Enqueue, Enqueued, IngestQueue, MasterKey, SqliteStore};
use liyasa_tests::server::{Harness, body_json, expect_status};

async fn store(name: &str) -> (std::path::PathBuf, Arc<SqliteStore>) {
    let dir = std::env::temp_dir().join(format!("liyasa-host07-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a test directory");
    let store = SqliteStore::open(
        &dir.join("liyasa.db"),
        MasterKey::generate().expect("a key"),
        IngestQueue::new(64, 16),
    )
    .await
    .expect("a store");
    (dir, Arc::new(store))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_nightly_schedule_runs_exactly_once_across_three_replicas() {
    let (dir, store) = store("once").await;
    store
        .jobs_typed()
        .enqueue(&Enqueue::new("verify.nightly", "site"))
        .await
        .expect("the schedule fires");

    let runs = Arc::new(AtomicUsize::new(0));
    let mut replicas = Vec::new();
    for name in ["replica-a", "replica-b", "replica-c"] {
        let store = store.clone();
        let runs = runs.clone();
        replicas.push(tokio::spawn(async move {
            // Every replica polls; only one may win the row.
            for _ in 0..20 {
                if let Some(job) = store
                    .jobs_typed()
                    .claim(name, Duration::from_secs(60))
                    .await
                    .expect("a claim attempt")
                {
                    runs.fetch_add(1, Ordering::SeqCst);
                    store
                        .jobs_typed()
                        .complete(&job.id, serde_json::json!({ "by": name }))
                        .await
                        .expect("completion");
                    return Some(name);
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            None
        }));
    }

    let mut winners = Vec::new();
    for replica in replicas {
        if let Some(name) = replica.await.expect("a replica finished") {
            winners.push(name);
        }
    }
    assert_eq!(runs.load(Ordering::SeqCst), 1, "winners: {winners:?}");
    assert_eq!(winners.len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_killed_replicas_lease_expires_and_another_re_claims_it() {
    let (dir, store) = store("lease").await;
    let jobs = store.jobs_typed();
    jobs.enqueue(&Enqueue::new("verify.nightly", "site"))
        .await
        .expect("the schedule fires");

    // replica-a takes the job with a short lease and then dies without
    // heartbeating or completing.
    let leased = jobs
        .claim("replica-a", Duration::from_millis(50))
        .await
        .expect("a claim")
        .expect("a job");
    assert_eq!(leased.state, JobState::Leased);
    assert!(
        jobs.claim("replica-b", Duration::from_secs(60))
            .await
            .expect("a claim attempt")
            .is_none(),
        "a live lease is not stealable"
    );

    tokio::time::sleep(Duration::from_millis(120)).await;

    let reclaimed = jobs
        .claim("replica-b", Duration::from_secs(60))
        .await
        .expect("a claim")
        .expect("the expired lease is re-claimable");
    assert_eq!(reclaimed.id, leased.id);
    assert_eq!(reclaimed.worker.as_deref(), Some("replica-b"));
    assert_eq!(reclaimed.attempts, 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_duplicate_trigger_with_the_same_key_is_not_queued() {
    let (dir, store) = store("dedup").await;
    let jobs = store.jobs_typed();
    let first = jobs
        .enqueue(&Enqueue::new("drift.refresh", "source:pricing"))
        .await
        .expect("a trigger");

    for _ in 0..5 {
        let again = jobs
            .enqueue(&Enqueue::new("drift.refresh", "source:pricing"))
            .await
            .expect("a repeat trigger");
        assert_eq!(again, Enqueued::Duplicate(first.id()));
    }
    assert_eq!(
        jobs.depth(Some("drift.refresh")).await.expect("the depth"),
        1
    );

    // A leased job still holds its key: a refresh that is running is not
    // queued again behind itself.
    jobs.claim("replica-a", Duration::from_secs(60))
        .await
        .expect("a claim")
        .expect("a job");
    assert_eq!(
        jobs.enqueue(&Enqueue::new("drift.refresh", "source:pricing"))
            .await
            .expect("a trigger while running"),
        Enqueued::Duplicate(first.id())
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_draining_replica_hands_its_leases_straight_back() {
    let (dir, store) = store("drain").await;
    let jobs = store.jobs_typed();
    for n in 0..3 {
        jobs.enqueue(&Enqueue::new("links.check", format!("page:{n}")))
            .await
            .expect("a job");
    }
    for _ in 0..3 {
        jobs.claim("replica-a", Duration::from_secs(600))
            .await
            .expect("a claim")
            .expect("a job");
    }
    assert!(
        jobs.claim("replica-b", Duration::from_secs(60))
            .await
            .expect("a claim attempt")
            .is_none()
    );

    // NFR-31: the leases come back at once rather than after ten minutes.
    assert_eq!(jobs.release("replica-a").await.expect("a drain"), 3);
    assert!(
        jobs.claim("replica-b", Duration::from_secs(60))
            .await
            .expect("a claim")
            .is_some()
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn the_rest_endpoints_list_retry_and_cancel_a_job() {
    let (harness, _site) = Harness::serving("host07-rest").await;
    let store = harness.state.store.clone().expect("a store");
    let id = store
        .jobs_typed()
        .enqueue(&Enqueue::new("verify.nightly", "site"))
        .await
        .expect("a job")
        .id();

    let listed = body_json(expect_status(
        harness.get("/_liyasa/api/v1/jobs").await,
        StatusCode::OK,
    ))
    .await;
    assert_eq!(listed["items"].as_array().expect("items").len(), 1);
    assert_eq!(listed["items"][0]["name"], "verify.nightly");
    assert_eq!(listed["items"][0]["state"], "queued");

    let filtered = body_json(harness.get("/_liyasa/api/v1/jobs?state=dead").await).await;
    assert!(filtered["items"].as_array().expect("items").is_empty());

    let cancelled = body_json(expect_status(
        harness
            .post_json(
                &format!("/_liyasa/api/v1/jobs/{id}/cancel"),
                serde_json::json!({}),
            )
            .await,
        StatusCode::OK,
    ))
    .await;
    assert_eq!(cancelled["state"], "failed");

    let retried = body_json(expect_status(
        harness
            .post_json(
                &format!("/_liyasa/api/v1/jobs/{id}/retry"),
                serde_json::json!({}),
            )
            .await,
        StatusCode::OK,
    ))
    .await;
    assert_eq!(retried["state"], "queued");
    assert_eq!(retried["attempts"], 0);

    // Cancelling something that is not live is a conflict, not a 404 and not
    // a silent success.
    store
        .jobs_typed()
        .complete(&id, serde_json::json!({}))
        .await
        .ok();
    expect_status(
        harness
            .post_json(
                &format!("/_liyasa/api/v1/jobs/{id}/cancel"),
                serde_json::json!({}),
            )
            .await,
        StatusCode::CONFLICT,
    );

    expect_status(
        harness.get("/_liyasa/api/v1/jobs/not-a-ulid").await,
        StatusCode::BAD_REQUEST,
    );
}
