//! GIT-21: manual triggers, deployment history, and completion webhooks.
//!
//! Given the dashboard, CLI, and REST API; when a deploy is triggered from
//! each; then history shows it with logs and a webhook fires on completion.
//!
//! The three callers are one endpoint on purpose. A dashboard button, `liyasa
//! deploy` and a `curl` in someone's CI all POST `/_liyasa/api/v1/builds`; a
//! second code path per caller is how three surfaces drift apart. What the
//! test pins is that all three shapes are accepted and land in the same queue.

use std::time::Duration;

use http::StatusCode;
use liyasa_core::ids::JobId;
use liyasa_git::repo::RepoRef;
use liyasa_server::deploy::queue::{BuildOutcome, DeployQueue};
use liyasa_server::deploy::service::Binding;
use liyasa_tests::deploy::{Harness, body_json};

async fn harness(name: &str) -> Harness {
    Harness::build(name, |state, project| {
        state.with_binding(Binding::new(
            RepoRef::new("kasecrab", "liyasa"),
            project,
            "main",
        ))
    })
    .await
}

async fn subscribe(harness: &Harness, events: &str) {
    let response = harness
        .post(
            "/_liyasa/api/v1/webhooks",
            serde_json::json!({
                "url": "https://hooks.example.com/liyasa",
                "secret": "a-receiver-secret-of-length",
                "events": [events],
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

/// `notify_webhook` queues its delivery on a spawned task, so a test that read
/// the table immediately would be reading it before the write.
async fn wait_for_delivery(harness: &Harness, event_type: &str) -> bool {
    for _ in 0..100 {
        let due = harness
            .store
            .webhooks()
            .due(50)
            .await
            .expect("the delivery table is readable");
        if due.iter().any(|d| d.event_type == event_type) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deploy_triggered_from_each_surface_lands_in_the_same_queue() {
    let harness = harness("trigger").await;
    let project = harness.project;
    subscribe(&harness, "*").await;

    // The dashboard: a branch, a commit, and nothing else.
    let dashboard = harness
        .post(
            "/_liyasa/api/v1/builds",
            serde_json::json!({
                "project": project.to_string(),
                "branch": "main",
                "commit": "aaaaaaa",
            }),
        )
        .await;
    assert_eq!(dashboard.status(), StatusCode::ACCEPTED);
    let dashboard = body_json(dashboard).await;
    assert_eq!(dashboard["env"], "production");

    // The CLI: `--message`, and an explicit environment.
    let cli = harness
        .post(
            "/_liyasa/api/v1/builds",
            serde_json::json!({
                "project": project.to_string(),
                "branch": "release",
                "commit": "bbbbbbb",
                "env": "preview",
                "message": "docs: rewrite the install guide",
            }),
        )
        .await;
    assert_eq!(cli.status(), StatusCode::ACCEPTED);
    let cli = body_json(cli).await;
    assert_eq!(cli["env"], "preview");

    // The REST API: a preview of a pull request (GIT-33).
    let rest = harness
        .post(
            "/_liyasa/api/v1/builds",
            serde_json::json!({
                "project": project.to_string(),
                "branch": "patch-1",
                "commit": "ccccccc",
                "pullRequest": 7,
            }),
        )
        .await;
    assert_eq!(rest.status(), StatusCode::ACCEPTED);
    let rest = body_json(rest).await;
    assert_eq!(
        rest["previewHost"], "liyasa-pr-7.preview.localhost",
        "a pull-request build is told where its preview will be"
    );

    let queue = DeployQueue::new(harness.store.clone());
    assert_eq!(queue.depth().await.expect("a depth"), 3);
    for triggered in [&dashboard, &cli, &rest] {
        let id = triggered["jobId"].as_str().expect("a job id");
        let job = harness
            .store
            .jobs_typed()
            .get(&JobId::parse(id).expect("a ULID"))
            .await
            .expect("the job is readable")
            .expect("the job exists");
        assert_eq!(job.payload["trigger"], "manual");
    }

    assert!(
        wait_for_delivery(&harness, "deployment.queued").await,
        "a trigger fires deployment.queued"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_shows_the_commit_the_logs_and_the_verification_report() {
    let harness = harness("history").await;
    let project = harness.project;

    let triggered = body_json(
        harness
            .post(
                "/_liyasa/api/v1/builds",
                serde_json::json!({
                    "project": project.to_string(),
                    "branch": "main",
                    "commit": "c0ffee1",
                    "message": "docs: fix the rate-limit table",
                }),
            )
            .await,
    )
    .await;
    let job = JobId::parse(triggered["jobId"].as_str().expect("a job id")).expect("a ULID");

    // The worker builds, records what it saw, and the deployment is pointed.
    // The duration in the history is the build record's own span, and
    // `Builds::put` stamps `updated_at` with now, so the fixture is aged
    // rather than dated from the epoch.
    let started = liyasa_store::now_ms() - 5_000;
    let build = harness.build_record("production", "built", started).await;
    let queue = DeployQueue::new(harness.store.clone());
    queue
        .complete(
            &job,
            &BuildOutcome::new(build.to_string())
                .with_logs("https://liyasa.example/builds/1/logs")
                .with_diagnostics(0, 3)
                .with_verification("41 of 42 checks passed"),
        )
        .await
        .expect("the outcome records");
    harness
        .store
        .deployments_typed()
        .point(&project, "production", &build)
        .await
        .expect("a deploy");

    let history = body_json(
        harness
            .get(&format!(
                "/_liyasa/api/v1/deployments/production/history?project={project}"
            ))
            .await,
    )
    .await;
    let items = history["items"].as_array().expect("the history");
    assert_eq!(items.len(), 1);
    let row = &items[0];
    assert_eq!(row["buildId"], build.to_string());
    assert_eq!(row["commit"], "c0ffee1");
    assert_eq!(row["branch"], "main");
    assert_eq!(row["message"], "docs: fix the rate-limit table");
    assert_eq!(row["trigger"], "manual");
    assert_eq!(row["logsUrl"], "https://liyasa.example/builds/1/logs");
    assert_eq!(row["errors"], 0);
    assert_eq!(row["warnings"], 3);
    assert_eq!(row["verification"], "41 of 42 checks passed");
    assert_eq!(row["status"], "succeeded");
    let duration = row["durationMs"].as_i64().expect("a duration");
    assert!(
        (4_000..60_000).contains(&duration),
        "the build took {duration}ms, which is not the five seconds the fixture spans"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deployment_that_succeeds_fires_a_webhook_to_every_subscriber() {
    let harness = harness("webhook").await;
    let project = harness.project;

    subscribe(&harness, "deployment.succeeded").await;

    let build = harness.build_record("production", "live", 1_000).await;
    let deployed = harness
        .post(
            "/_liyasa/api/v1/deployments",
            serde_json::json!({
                "project": project.to_string(),
                "env": "production",
                "buildId": build.to_string(),
            }),
        )
        .await;
    assert_eq!(deployed.status(), StatusCode::CREATED);

    assert!(
        wait_for_delivery(&harness, "deployment.succeeded").await,
        "completion fires deployment.succeeded"
    );
    let due = harness
        .store
        .webhooks()
        .due(50)
        .await
        .expect("the delivery table is readable");
    let delivery = due
        .iter()
        .find(|d| d.event_type == "deployment.succeeded")
        .expect("the delivery");
    let payload: serde_json::Value =
        serde_json::from_str(&delivery.payload).expect("the envelope is JSON");
    assert_eq!(payload["type"], "deployment.succeeded");
    assert_eq!(payload["data"]["buildId"], build.to_string());
    assert_eq!(payload["data"]["env"], "production");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trigger_for_an_unknown_project_or_environment_is_refused() {
    let harness = harness("trigger-unknown").await;
    let unknown =
        liyasa_core::ids::ProjectId::parse("00000000000000000000000000").expect("a valid ULID");

    let no_project = harness
        .post(
            "/_liyasa/api/v1/builds",
            serde_json::json!({
                "project": unknown.to_string(),
                "branch": "main",
                "commit": "aaa",
            }),
        )
        .await;
    assert_eq!(no_project.status(), StatusCode::NOT_FOUND);

    let no_env = harness
        .post(
            "/_liyasa/api/v1/builds",
            serde_json::json!({
                "project": harness.project.to_string(),
                "branch": "main",
                "commit": "aaa",
                "env": "nowhere",
            }),
        )
        .await;
    assert_eq!(no_env.status(), StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_manual_build_of_an_untrusted_branch_is_still_untrusted() {
    let harness = harness("trigger-untrusted").await;
    let triggered = body_json(
        harness
            .post(
                "/_liyasa/api/v1/builds",
                serde_json::json!({
                    "project": harness.project.to_string(),
                    "branch": "feat/anything",
                    "commit": "ddd",
                }),
            )
            .await,
    )
    .await;
    assert_eq!(
        triggered["untrusted"], true,
        "who pressed the button does not change what the branch contains"
    );
    assert_eq!(triggered["env"], "preview");
}
