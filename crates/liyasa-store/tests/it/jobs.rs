//! The job table's lease semantics (HOST-07).

use std::time::Duration;

use liyasa_core::store::{JobQuery, JobState, Page};
use liyasa_store::jobs::{Enqueue, Enqueued, Jobs, backoff};

use crate::support::app_db;

fn refresh(name: &str, source: &str) -> Enqueue {
    Enqueue::new(name, source)
}

#[tokio::test]
async fn a_second_trigger_with_the_same_key_is_not_queued_twice() {
    let (_dir, pool) = app_db("jobs-dedup").await;
    let jobs = Jobs::new(pool);

    let first = jobs
        .enqueue(&refresh("drift.refresh", "source:openapi"))
        .await
        .expect("the first trigger");
    let second = jobs
        .enqueue(&refresh("drift.refresh", "source:openapi"))
        .await
        .expect("the second trigger");

    assert!(matches!(first, Enqueued::Queued(_)));
    assert_eq!(second, Enqueued::Duplicate(first.id()));
    assert_eq!(jobs.depth(None).await.expect("the depth"), 1);

    // A different source is a different key, and so a different job.
    let other = jobs
        .enqueue(&refresh("drift.refresh", "source:pricing"))
        .await
        .expect("another source");
    assert!(matches!(other, Enqueued::Queued(_)));
    assert_eq!(jobs.depth(None).await.expect("the depth"), 2);

    // Once the first is done its key is free again.
    jobs.complete(&first.id(), serde_json::json!({"ok": true}))
        .await
        .expect("completion");
    let requeued = jobs
        .enqueue(&refresh("drift.refresh", "source:openapi"))
        .await
        .expect("a fresh trigger");
    assert!(matches!(requeued, Enqueued::Queued(_)));
    assert_ne!(requeued.id(), first.id());
}

#[tokio::test]
async fn exactly_one_worker_claims_a_scheduled_job() {
    let (_dir, pool) = app_db("jobs-one-claim").await;
    let jobs = Jobs::new(pool);
    jobs.enqueue(&Enqueue::new("verify.nightly", "site"))
        .await
        .expect("the schedule fires");

    let mut claimed = Vec::new();
    for worker in ["replica-a", "replica-b", "replica-c"] {
        if let Some(job) = jobs
            .claim(worker, Duration::from_secs(60))
            .await
            .expect("a claim attempt")
        {
            claimed.push((worker.to_owned(), job));
        }
    }

    assert_eq!(claimed.len(), 1, "one run, not three");
    let (worker, job) = &claimed[0];
    assert_eq!(job.state, JobState::Leased);
    assert_eq!(job.worker.as_deref(), Some(worker.as_str()));
    assert_eq!(job.attempts, 1);
}

#[tokio::test]
async fn a_dead_workers_lease_expires_and_another_replica_re_claims_it() {
    let (_dir, pool) = app_db("jobs-lease").await;
    let jobs = Jobs::new(pool);
    jobs.enqueue(&Enqueue::new("verify.nightly", "site"))
        .await
        .expect("the schedule fires");

    let first = jobs
        .claim("replica-a", Duration::from_millis(40))
        .await
        .expect("a claim")
        .expect("a job");
    assert!(
        jobs.claim("replica-b", Duration::from_secs(60))
            .await
            .expect("a claim attempt")
            .is_none(),
        "the lease is live, so nobody else may have it"
    );

    // replica-a stops heartbeating.
    tokio::time::sleep(Duration::from_millis(80)).await;

    let second = jobs
        .claim("replica-b", Duration::from_secs(60))
        .await
        .expect("a claim")
        .expect("the expired lease is re-claimable");
    assert_eq!(second.id, first.id);
    assert_eq!(second.worker.as_deref(), Some("replica-b"));
    assert_eq!(second.attempts, 2, "the re-claim counts as an attempt");

    // The dead worker's heartbeat is refused: it no longer holds the lease.
    assert!(jobs.heartbeat(&first.id).await.is_ok());
}

#[tokio::test]
async fn a_heartbeat_renews_the_lease_and_is_refused_once_it_has_expired() {
    let (_dir, pool) = app_db("jobs-heartbeat").await;
    let jobs = Jobs::new(pool);
    jobs.enqueue(&Enqueue::new("build", "project:a"))
        .await
        .expect("a job");
    let job = jobs
        .claim("worker", Duration::from_millis(120))
        .await
        .expect("a claim")
        .expect("a job");

    tokio::time::sleep(Duration::from_millis(60)).await;
    jobs.heartbeat(&job.id).await.expect("a live lease renews");
    let renewed = jobs.get(&job.id).await.expect("a read").expect("the job");
    assert!(renewed.lease_until > job.lease_until);

    // Nobody may steal it while it is being renewed.
    assert!(
        jobs.claim("thief", Duration::from_secs(60))
            .await
            .expect("a claim attempt")
            .is_none()
    );

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        jobs.heartbeat(&job.id).await.is_err(),
        "an expired lease is not renewed; another replica may hold it"
    );
}

#[tokio::test]
async fn a_failure_backs_off_while_attempts_remain() {
    let (_dir, pool) = app_db("jobs-backoff").await;
    let jobs = Jobs::new(pool);
    let mut enqueue = Enqueue::new("webhook.deliver", "delivery:1");
    enqueue.max_attempts = 3;
    let id = jobs.enqueue(&enqueue).await.expect("a job").id();

    jobs.claim("worker", Duration::from_secs(60))
        .await
        .expect("a claim")
        .expect("a job");
    assert_eq!(
        jobs.fail(&id, "connection refused")
            .await
            .expect("a failure"),
        JobState::Queued
    );

    let queued = jobs.get(&id).await.expect("a read").expect("the job");
    assert_eq!(queued.attempts, 1);
    assert_eq!(queued.error.as_deref(), Some("connection refused"));
    assert!(
        queued.run_at >= liyasa_store::now_ms() + 50_000,
        "the first retry waits about a minute"
    );
    assert!(
        jobs.claim("worker", Duration::from_secs(60))
            .await
            .expect("a claim attempt")
            .is_none(),
        "a backed-off job is not runnable yet"
    );
}

#[tokio::test]
async fn the_last_attempt_kills_the_job_and_an_operator_can_retry_it() {
    let (_dir, pool) = app_db("jobs-dead").await;
    let jobs = Jobs::new(pool);
    let mut enqueue = Enqueue::new("webhook.deliver", "delivery:1");
    enqueue.max_attempts = 1;
    let id = jobs.enqueue(&enqueue).await.expect("a job").id();

    jobs.claim("worker", Duration::from_secs(60))
        .await
        .expect("a claim")
        .expect("a job");
    assert_eq!(
        jobs.fail(&id, "gone for good").await.expect("a failure"),
        JobState::Dead
    );
    let dead = jobs.get(&id).await.expect("a read").expect("the job");
    assert_eq!(dead.state, JobState::Dead);
    assert_eq!(dead.worker, None, "a finished job holds no lease");

    jobs.retry(&id).await.expect("an operator retry");
    let requeued = jobs.get(&id).await.expect("a read").expect("the job");
    assert_eq!(requeued.state, JobState::Queued);
    assert_eq!(requeued.attempts, 0, "a retry starts the attempts over");
    assert!(requeued.error.is_none());
    assert!(
        jobs.claim("worker", Duration::from_secs(60))
            .await
            .expect("a claim")
            .is_some()
    );
}

#[test]
fn the_backoff_ladder_is_the_documented_one() {
    let minutes = |d: std::time::Duration| d.as_secs() / 60;
    assert_eq!(minutes(backoff(1)), 1);
    assert_eq!(minutes(backoff(2)), 5);
    assert_eq!(minutes(backoff(3)), 30);
    assert_eq!(minutes(backoff(4)), 120);
    assert_eq!(minutes(backoff(5)), 720);
    assert_eq!(
        minutes(backoff(50)),
        720,
        "the ladder stops at its last rung"
    );
}

#[tokio::test]
async fn priority_orders_the_queue_and_a_drain_hands_the_leases_back() {
    let (_dir, pool) = app_db("jobs-priority").await;
    let jobs = Jobs::new(pool);
    for (name, priority) in [("reindex", -10), ("deploy", 100), ("verify", 0)] {
        let mut enqueue = Enqueue::new(name, "one");
        enqueue.priority = priority;
        jobs.enqueue(&enqueue).await.expect("a job");
    }

    let mut order = Vec::new();
    while let Some(job) = jobs
        .claim("worker", Duration::from_secs(60))
        .await
        .expect("a claim attempt")
    {
        order.push(job.name.clone());
        if order.len() == 3 {
            break;
        }
    }
    assert_eq!(order, ["deploy", "verify", "reindex"]);

    let released = jobs.release("worker").await.expect("a drain");
    assert_eq!(released, 3);
    let queued = jobs
        .list(
            &JobQuery {
                state: Some(JobState::Queued),
                ..JobQuery::default()
            },
            Page::default(),
        )
        .await
        .expect("a listing");
    assert_eq!(queued.len(), 3, "a drained replica leaves nothing leased");
    assert!(
        queued.iter().all(|j| j.attempts == 0),
        "a released job did not really attempt anything"
    );
}

#[tokio::test]
async fn cancelling_a_job_stops_it_and_listing_filters_by_name_and_state() {
    let (_dir, pool) = app_db("jobs-cancel").await;
    let jobs = Jobs::new(pool);
    let id = jobs
        .enqueue(&Enqueue::new("links.check", "site"))
        .await
        .expect("a job")
        .id();
    jobs.enqueue(&Enqueue::new("verify.nightly", "site"))
        .await
        .expect("a job");

    jobs.cancel(&id).await.expect("a cancellation");
    assert!(
        jobs.cancel(&id).await.is_err(),
        "a job that is not live cannot be cancelled again"
    );

    let listed = jobs
        .list(
            &JobQuery {
                name: Some("links.check".to_owned()),
                ..JobQuery::default()
            },
            Page::default(),
        )
        .await
        .expect("a listing");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].state, JobState::Failed);
    assert_eq!(jobs.depth(None).await.expect("the depth"), 1);
}
