//! GIT-20: auto-deploy on push, end to end.
//!
//! Given a project with a previous build; when a single-page commit is pushed;
//! then the deployment completes end to end in under 10 s.
//!
//! **What this test measures, and what it does not.** GIT-20's ten seconds
//! covers delivery, build and swap. §6.13 puts the build in a separate worker
//! process, and no worker exists in this workspace yet, so the build step here
//! is a stub: the test measures everything on either side of it — verifying
//! the delivery, queueing with the cache and verify inputs, recording the
//! outcome, moving the pointer, and queueing the post-deploy embedding — and
//! holds that to a budget of its own. The acceptance table calls the full
//! version a staging gate for exactly this reason; when a worker exists, the
//! stub here is the only line that changes.
//!
//! The second half is the part GIT-20 is really about and is not a timing
//! claim at all: the deploy is incremental because the job names the previous
//! build to warm from, and the embedding is *after* the swap rather than on
//! the path, so a slow model provider cannot hold a deploy open.

use std::time::{Duration, Instant};

use http::StatusCode;
use liyasa_core::ids::JobId;
use liyasa_core::store::JobState;
use liyasa_git::repo::RepoRef;
use liyasa_git::webhook::{Provider, sign};
use liyasa_server::deploy::queue::{BuildOutcome, DeployQueue, EMBED_JOB};
use liyasa_server::deploy::service::{Binding, Hooks};
use liyasa_tests::deploy::{Harness, body_json};

const SECRET: &str = "the-app-webhook-secret-value";

/// Everything except the build itself, which belongs to a worker process.
const SERVER_SIDE_BUDGET: Duration = Duration::from_secs(2);

fn push_payload(before: &str, after: &str) -> Vec<u8> {
    format!(
        r#"{{"ref":"refs/heads/main",
            "before":"{before}",
            "after":"{after}",
            "repository":{{"full_name":"kasecrab/liyasa"}},
            "installation":{{"id":44556677}},
            "commits":[{{"id":"{after}","message":"docs: fix one line",
                        "added":[],"modified":["docs/reference/limits.md"],"removed":[]}}]}}"#
    )
    .into_bytes()
}

async fn harness(name: &str) -> Harness {
    Harness::build(name, |state, project| {
        state
            .with_hooks(Hooks::new().with_secret(Provider::GitHub, SECRET))
            .with_binding(Binding::new(
                RepoRef::new("kasecrab", "liyasa"),
                project,
                "main",
            ))
    })
    .await
}

async fn deliver(harness: &Harness, id: &str, body: &[u8]) -> serde_json::Value {
    let signature = sign(SECRET, body);
    let response = harness
        .post_raw(
            "/_liyasa/hooks/github",
            &[
                ("x-github-event", "push"),
                ("x-github-delivery", id),
                ("x-hub-signature-256", &signature),
            ],
            body,
        )
        .await;
    assert!(
        response.status().is_success(),
        "the delivery was refused: {:?}",
        response.status()
    );
    body_json(response).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_single_page_push_deploys_within_the_budget_this_side_of_the_worker() {
    let harness = harness("deploy-end-to-end").await;
    let project = harness.project;

    // A previous build, already live: this is the "project with a previous
    // build" the requirement starts from.
    let previous = harness
        .build_record("production", "previous", liyasa_store::now_ms() - 60_000)
        .await;
    harness
        .store
        .deployments_typed()
        .point(&project, "production", &previous)
        .await
        .expect("the previous deploy");

    let started = Instant::now();

    let queued = deliver(
        &harness,
        "d-deploy-1",
        &push_payload("a".repeat(40).as_str(), "b".repeat(40).as_str()),
    )
    .await;
    let job = JobId::parse(queued["jobId"].as_str().expect("a job id")).expect("a ULID");

    // --- the worker's part, stubbed: build the bundle, record the outcome ---
    let built = harness
        .build_record("production", "built", liyasa_store::now_ms() - 1_000)
        .await;
    let queue = DeployQueue::new(harness.store.clone());
    queue
        .complete(
            &job,
            &BuildOutcome::new(built.to_string())
                .with_logs("https://liyasa.example/builds/2/logs")
                .with_verification("1 page verified"),
        )
        .await
        .expect("the outcome records");
    // --- end of the worker's part ---

    let activated = body_json(
        harness
            .post(
                &format!("/_liyasa/api/v1/builds/{job}/deploy"),
                serde_json::json!({}),
            )
            .await,
    )
    .await;
    let elapsed = started.elapsed();

    assert_eq!(activated["buildId"], built.to_string());
    assert_eq!(activated["env"], "production");
    assert_eq!(
        harness
            .store
            .deployments_typed()
            .current(&project, "production")
            .await
            .expect("a pointer")
            .map(|record| record.build),
        Some(built),
        "the new build is what production serves"
    );
    assert!(
        elapsed < SERVER_SIDE_BUDGET,
        "the server's share of the deploy took {elapsed:?}, over its {SERVER_SIDE_BUDGET:?} budget"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_queued_build_names_the_previous_build_to_warm_from_and_the_commit_to_verify_against() {
    let harness = harness("deploy-incremental").await;
    let project = harness.project;
    let previous = harness
        .build_record("production", "previous", liyasa_store::now_ms() - 60_000)
        .await;
    harness
        .store
        .deployments_typed()
        .point(&project, "production", &previous)
        .await
        .expect("the previous deploy");

    let before = "a".repeat(40);
    let after = "b".repeat(40);
    let queued = deliver(&harness, "d-inc-1", &push_payload(&before, &after)).await;
    let job = harness
        .store
        .jobs_typed()
        .get(&JobId::parse(queued["jobId"].as_str().expect("a job id")).expect("a ULID"))
        .await
        .expect("the job is readable")
        .expect("the job exists");

    assert_eq!(
        job.payload["cacheFrom"],
        previous.to_string(),
        "the artifact cache is restored from the previous build of this environment"
    );
    assert_eq!(
        job.payload["baseCommit"], before,
        "`verify --changed` diffs against the commit this push moved from"
    );
    assert_eq!(job.payload["commit"], after);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_first_deploy_has_no_previous_build_to_warm_from() {
    let harness = harness("deploy-first").await;
    let queued = deliver(
        &harness,
        "d-first-1",
        &push_payload(&"0".repeat(40), &"c".repeat(40)),
    )
    .await;
    let job = harness
        .store
        .jobs_typed()
        .get(&JobId::parse(queued["jobId"].as_str().expect("a job id")).expect("a ULID"))
        .await
        .expect("the job is readable")
        .expect("the job exists");
    assert!(job.payload["cacheFrom"].is_null());
    assert!(
        job.payload["baseCommit"].is_null(),
        "a zero `before` is not a commit to diff against"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn embedding_is_queued_after_the_swap_rather_than_waited_on() {
    let harness = harness("deploy-embed").await;
    let project = harness.project;
    let queued = deliver(
        &harness,
        "d-embed-1",
        &push_payload(&"a".repeat(40), &"d".repeat(40)),
    )
    .await;
    let job = JobId::parse(queued["jobId"].as_str().expect("a job id")).expect("a ULID");
    let built = harness
        .build_record("production", "embed", liyasa_store::now_ms() - 1_000)
        .await;
    DeployQueue::new(harness.store.clone())
        .complete(&job, &BuildOutcome::new(built.to_string()))
        .await
        .expect("the outcome records");

    let activated = body_json(
        harness
            .post(
                &format!("/_liyasa/api/v1/builds/{job}/deploy"),
                serde_json::json!({}),
            )
            .await,
    )
    .await;

    let embedding = activated["embeddingJobId"]
        .as_str()
        .expect("the post-deploy embedding job");
    let embed_job = harness
        .store
        .jobs_typed()
        .get(&JobId::parse(embedding).expect("a ULID"))
        .await
        .expect("the job is readable")
        .expect("the job exists");
    assert_eq!(embed_job.name, EMBED_JOB);
    assert_eq!(embed_job.state, JobState::Queued, "queued, not run inline");
    assert_eq!(embed_job.payload["buildId"], built.to_string());
    assert!(
        embed_job.priority < 30,
        "embedding never outranks a build; it is the lowest class"
    );

    // The pointer moved before the embedding was queued, which is the whole
    // point: the assistant answers from the old embeddings until this job
    // finishes, and the pages are already live.
    assert_eq!(
        harness
            .store
            .deployments_typed()
            .current(&project, "production")
            .await
            .expect("a pointer")
            .map(|record| record.build),
        Some(built)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deploying_the_same_build_twice_does_not_embed_twice() {
    let harness = harness("deploy-embed-once").await;
    let queued = deliver(
        &harness,
        "d-embed-2",
        &push_payload(&"a".repeat(40), &"e".repeat(40)),
    )
    .await;
    let job = JobId::parse(queued["jobId"].as_str().expect("a job id")).expect("a ULID");
    let built = harness
        .build_record("production", "twice", liyasa_store::now_ms() - 1_000)
        .await;
    DeployQueue::new(harness.store.clone())
        .complete(&job, &BuildOutcome::new(built.to_string()))
        .await
        .expect("the outcome records");

    let first = body_json(
        harness
            .post(
                &format!("/_liyasa/api/v1/builds/{job}/deploy"),
                serde_json::json!({}),
            )
            .await,
    )
    .await;
    assert!(first["embeddingJobId"].is_string());
    let second = body_json(
        harness
            .post(
                &format!("/_liyasa/api/v1/builds/{job}/deploy"),
                serde_json::json!({}),
            )
            .await,
    )
    .await;
    assert!(
        second["embeddingJobId"].is_null(),
        "the same bundle is embedded once"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_build_with_no_recorded_outcome_cannot_be_deployed() {
    let harness = harness("deploy-no-outcome").await;
    let queued = deliver(
        &harness,
        "d-none-1",
        &push_payload(&"a".repeat(40), &"f".repeat(40)),
    )
    .await;
    let job = queued["jobId"].as_str().expect("a job id");
    let response = harness
        .post(
            &format!("/_liyasa/api/v1/builds/{job}/deploy"),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "a build that has not said what it produced cannot be made live"
    );
}
