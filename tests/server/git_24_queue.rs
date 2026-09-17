//! GIT-24: deploy and preview builds on the build-worker queue (§6.13).
//!
//! Given two projects each pushing ten commits at once; when builds are
//! queued; then per-project concurrency is one, superseded builds are
//! cancelled, priorities order production before preview, and the queue-depth
//! metric and position API report correctly.
//!
//! The ten commits arrive on ten different branches. Ten pushes to *one*
//! branch supersede each other down to one build by design, which is a
//! different requirement and is exercised below on its own.

use http::StatusCode;
use liyasa_core::ids::ProjectId;
use liyasa_core::store::JobState;
use liyasa_git::repo::RepoRef;
use liyasa_server::deploy::queue::{self, Accepted, BuildRequest, Class, Limits, QueueError};
use liyasa_server::deploy::service::Binding;
use liyasa_tests::deploy::{Harness, body_json};

async fn harness(name: &str) -> (Harness, ProjectId) {
    let harness = Harness::build(name, |state, _| state).await;
    let second = harness.another_project(&format!("{name}-second")).await;
    (harness, second)
}

fn push(project: ProjectId, branch: &str, class: Class) -> BuildRequest {
    BuildRequest::new(
        project,
        match class {
            Class::Preview => "preview",
            _ => "production",
        },
        class,
        "kasecrab/liyasa",
        branch,
        format!("sha-{branch}"),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_projects_pushing_ten_commits_at_once_take_turns_and_report_their_position() {
    let (harness, second) = harness("queue-fairness").await;
    let first = harness.project;
    let queue = queue::DeployQueue::new(harness.store.clone());

    for n in 0..10 {
        for project in [first, second] {
            queue
                .submit(&push(project, &format!("feat/{n}"), Class::Production))
                .await
                .expect("the build queues");
        }
    }

    assert_eq!(queue.depth().await.expect("a depth"), 20);

    let (pending, running) = queue.pending().await.expect("the queue");
    assert!(running.is_empty(), "nothing is leased yet");
    let order = queue::order(&pending);
    assert_eq!(order.len(), 20);

    let owner = |id| {
        pending
            .iter()
            .find(|job| job.id == id)
            .map(|job| job.project)
            .expect("a queued job")
    };
    // Round-robin: the two projects alternate rather than one project holding
    // the head of the queue for ten places.
    for pair in order.chunks(2) {
        if pair.len() == 2 {
            assert_ne!(
                owner(pair[0]),
                owner(pair[1]),
                "one project took two turns in a row"
            );
        }
    }

    // The position API agrees with the order a worker will honour.
    for (index, id) in order.iter().enumerate() {
        assert_eq!(
            queue.position(*id).await.expect("a position"),
            Some(index as u32 + 1)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn per_project_concurrency_is_one() {
    let (harness, second) = harness("queue-concurrency").await;
    let first = harness.project;
    let queue = queue::DeployQueue::new(harness.store.clone());
    assert_eq!(queue.limits().concurrency_per_project, 1);

    for branch in ["a", "b"] {
        queue
            .submit(&push(first, branch, Class::Production))
            .await
            .expect("the build queues");
    }
    queue
        .submit(&push(second, "a", Class::Production))
        .await
        .expect("the build queues");

    let (pending, _) = queue.pending().await.expect("the queue");
    let head = queue::next(&pending, &[], &Limits::default()).expect("something to run");
    // With the first project building, its second build waits and the other
    // project's goes instead.
    let next = queue::next(&pending, &[first], &Limits::default()).expect("something to run");
    assert_ne!(next, head);
    assert_eq!(
        pending
            .iter()
            .find(|job| job.id == next)
            .map(|job| job.project),
        Some(second)
    );
    assert_eq!(
        queue::next(&pending, &[first, second], &Limits::default()),
        None,
        "both projects at their limit leaves nothing runnable"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_newer_push_to_a_branch_cancels_the_build_queued_for_the_older_commit() {
    let (harness, _) = harness("queue-supersede").await;
    let queue = queue::DeployQueue::new(harness.store.clone());

    let first = queue
        .submit(&BuildRequest::new(
            harness.project,
            "production",
            Class::Production,
            "kasecrab/liyasa",
            "main",
            "old-commit",
        ))
        .await
        .expect("the first build queues");
    let Accepted::Queued(old) = first else {
        panic!("the first build supersedes nothing");
    };

    let second = queue
        .submit(&BuildRequest::new(
            harness.project,
            "production",
            Class::Production,
            "kasecrab/liyasa",
            "main",
            "new-commit",
        ))
        .await
        .expect("the second build queues");
    assert_eq!(second.superseded(), Some(old));

    let cancelled = harness
        .store
        .jobs_typed()
        .get(&old)
        .await
        .expect("the old job is readable")
        .expect("the old job still exists as a record");
    assert_eq!(cancelled.state, JobState::Failed);
    assert_eq!(cancelled.error.as_deref(), Some("cancelled"));
    assert_eq!(
        queue.depth().await.expect("a depth"),
        1,
        "the branch has exactly one live build"
    );

    let live = harness
        .store
        .jobs_typed()
        .get(&second.id())
        .await
        .expect("the new job is readable")
        .expect("the new job exists");
    assert_eq!(live.payload["commit"], "new-commit");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pull_request_preview_queues_behind_every_production_deploy() {
    let (harness, second) = harness("queue-priority").await;
    let queue = queue::DeployQueue::new(harness.store.clone());

    // The preview is queued first, so age alone would put it at the head.
    queue
        .submit(&push(harness.project, "patch-1", Class::Preview).for_pull_request(7))
        .await
        .expect("the preview queues");
    queue
        .submit(&push(second, "main", Class::Production))
        .await
        .expect("the deploy queues");

    let (pending, _) = queue.pending().await.expect("the queue");
    let order = queue::order(&pending);
    let head = pending
        .iter()
        .find(|job| job.id == order[0])
        .expect("the head of the queue");
    assert_eq!(
        head.project, second,
        "production outranks a waiting preview"
    );
    assert_eq!(Class::from_priority(head.priority), Class::Production);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_full_queue_is_refused_rather_than_accepted_and_dropped() {
    let (harness, _) = harness("queue-full").await;
    let queue = queue::DeployQueue::new(harness.store.clone()).with_limits(Limits {
        queue: 2,
        concurrency_per_project: 1,
    });

    for branch in ["a", "b"] {
        queue
            .submit(&push(harness.project, branch, Class::Production))
            .await
            .expect("the build queues");
    }
    let refused = queue
        .submit(&push(harness.project, "c", Class::Production))
        .await
        .expect_err("the queue is full");
    assert_eq!(refused, QueueError::Full { depth: 2, cap: 2 });

    // A branch already in the queue is still accepted at the cap, because it
    // replaces a job rather than adding one.
    assert!(
        queue
            .submit(&push(harness.project, "a", Class::Production))
            .await
            .is_ok(),
        "superseding does not grow the queue"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_queue_endpoint_reports_depth_position_and_an_estimated_start() {
    let harness = Harness::build("queue-api", |state, project| {
        state.with_binding(Binding::new(
            RepoRef::new("kasecrab", "liyasa"),
            project,
            "main",
        ))
    })
    .await;
    let queue = queue::DeployQueue::new(harness.store.clone());
    for branch in ["a", "b", "c"] {
        queue
            .submit(&push(harness.project, branch, Class::Production))
            .await
            .expect("the build queues");
    }

    let body = body_json(
        harness
            .get("/_liyasa/api/v1/builds?averageBuildMs=8000&workers=2")
            .await,
    )
    .await;
    assert_eq!(body["depth"], 3);
    assert_eq!(body["cap"], queue::DEFAULT_QUEUE_CAP);
    assert_eq!(body["concurrencyPerProject"], 1);
    let items = body["items"].as_array().expect("the queued builds");
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["position"], 1);
    assert_eq!(items[0]["estimatedStartMs"], 0);
    assert_eq!(items[2]["position"], 3);
    assert_eq!(
        items[2]["estimatedStartMs"], 8000,
        "two builds ahead at eight seconds each across two workers"
    );

    // And one build's own status carries the same position.
    let job_id = items[2]["jobId"].as_str().expect("a job id");
    let status = body_json(
        harness
            .get(&format!("/_liyasa/api/v1/builds/{job_id}"))
            .await,
    )
    .await;
    assert_eq!(status["position"], 3);
    assert_eq!(status["class"], "production");
    assert_eq!(status["state"], "queued");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_build_is_a_404_rather_than_an_empty_body() {
    let harness = Harness::build("queue-missing", |state, _| state).await;
    let response = harness
        .get("/_liyasa/api/v1/builds/00000000000000000000000000")
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let malformed = harness.get("/_liyasa/api/v1/builds/not-a-ulid").await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
}
