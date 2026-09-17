//! GIT-01: a GitHub App installation, end to end, on recorded bytes.
//!
//! Given a GitHub App installation on a test repository; when a push occurs;
//! then the webhook is verified, a build starts, and a check run reports the
//! result.
//!
//! The delivery below is a real `push` payload with the fields Liyasa reads,
//! and it is posted as bytes rather than as a `serde_json::Value`: the
//! signature is over the bytes GitHub sent, and a body that has been through
//! `serde` and back is a different sequence of bytes with the same meaning.
//! Getting that wrong is the classic webhook bug, so the test is shaped to
//! catch it.

use std::sync::Arc;

use http::StatusCode;
use liyasa_core::ids::JobId;
use liyasa_git::github::{API_VERSION, GitHub};
use liyasa_git::provider::{
    Annotation, CheckConclusion, CheckRun, Endpoint, GitProvider, StaticToken,
};
use liyasa_git::recorder::{Exchange, Recorded};
use liyasa_git::repo::RepoRef;
use liyasa_git::webhook::{Provider, sign};
use liyasa_server::deploy::queue::DeployQueue;
use liyasa_server::deploy::service::{Binding, Hooks};
use liyasa_tests::deploy::{Harness, body_json};

const SECRET: &str = "the-app-webhook-secret-value";

/// A recorded `push` delivery: two commits on `main`, four paths between them.
const PUSH: &[u8] = br#"{
  "ref": "refs/heads/main",
  "before": "0d1a26e3c9e3d2b8f1a4c5b6d7e8f9a0b1c2d3e4",
  "after": "9f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c",
  "repository": { "full_name": "kasecrab/liyasa", "default_branch": "main" },
  "installation": { "id": 44556677 },
  "pusher": { "name": "kasecrab" },
  "commits": [
    { "id": "1111111111111111111111111111111111111111",
      "message": "docs: fix the rate-limit table",
      "added": [], "modified": ["docs/reference/limits.md"], "removed": [] },
    { "id": "9f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c",
      "message": "docs: add a migration note",
      "added": ["docs/guides/migrate.md"], "modified": [], "removed": [] }
  ],
  "head_commit": { "id": "9f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c" }
}"#;

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

fn delivery_headers(id: &str, body: &[u8]) -> Vec<(String, String)> {
    vec![
        ("x-github-event".to_owned(), "push".to_owned()),
        ("x-github-delivery".to_owned(), id.to_owned()),
        ("x-hub-signature-256".to_owned(), sign(SECRET, body)),
    ]
}

async fn post_delivery(
    harness: &Harness,
    id: &str,
    body: &[u8],
) -> http::Response<axum::body::Body> {
    let owned = delivery_headers(id, body);
    let headers: Vec<(&str, &str)> = owned
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    harness
        .post_raw("/_liyasa/hooks/github", &headers, body)
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_signed_push_is_verified_and_starts_a_build() {
    let harness = harness("git01-push").await;

    let response = post_delivery(&harness, "d-0001", PUSH).await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = body_json(response).await;
    assert_eq!(body["env"], "production");
    assert_eq!(body["branch"], "main");
    assert_eq!(body["commit"], "9f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c");
    assert_eq!(body["untrusted"], false, "the deploy branch is trusted");

    let queue = DeployQueue::new(harness.store.clone());
    assert_eq!(queue.depth().await.expect("a depth"), 1);
    let job = harness
        .store
        .jobs_typed()
        .get(&JobId::parse(body["jobId"].as_str().expect("a job id")).expect("a ULID"))
        .await
        .expect("the job is readable")
        .expect("the job exists");
    assert_eq!(job.payload["trigger"], "push");
    assert_eq!(job.payload["repo"], "kasecrab/liyasa");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_body_edited_in_flight_is_refused_and_nothing_is_built() {
    let harness = harness("git01-tampered").await;

    // Signed over the real payload, delivered with one field changed.
    let signature = sign(SECRET, PUSH);
    let tampered = String::from_utf8_lossy(PUSH)
        .replace("kasecrab/liyasa", "attacker/liyasa")
        .into_bytes();
    let response = harness
        .post_raw(
            "/_liyasa/hooks/github",
            &[
                ("x-github-event", "push"),
                ("x-github-delivery", "d-0002"),
                ("x-hub-signature-256", &signature),
            ],
            &tampered,
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(response).await;
    assert_eq!(body["code"], "E0808");

    let queue = DeployQueue::new(harness.store.clone());
    assert_eq!(
        queue.depth().await.expect("a depth"),
        0,
        "a refused delivery must never reach the queue"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unsigned_delivery_is_refused() {
    let harness = harness("git01-unsigned").await;
    let response = harness
        .post_raw(
            "/_liyasa/hooks/github",
            &[("x-github-event", "push"), ("x-github-delivery", "d-0003")],
            PUSH,
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        DeployQueue::new(harness.store.clone())
            .depth()
            .await
            .expect("a depth"),
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_delivery_twice_builds_once() {
    let harness = harness("git01-replay").await;

    assert_eq!(
        post_delivery(&harness, "d-0004", PUSH).await.status(),
        StatusCode::ACCEPTED
    );
    let replay = post_delivery(&harness, "d-0004", PUSH).await;
    assert_eq!(replay.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(replay).await["code"], "E0808");

    assert_eq!(
        DeployQueue::new(harness.store.clone())
            .depth()
            .await
            .expect("a depth"),
        1,
        "the replay queued nothing"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tag_push_and_a_starred_repository_are_verified_and_ignored() {
    let harness = harness("git01-ignored").await;

    let tag = String::from_utf8_lossy(PUSH)
        .replace("refs/heads/main", "refs/tags/v1.0.0")
        .into_bytes();
    let response = post_delivery(&harness, "d-0005", &tag).await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(body_json(response).await["status"], "ignored");

    let star = br#"{"action":"created","repository":{"full_name":"kasecrab/liyasa"}}"#;
    let signature = sign(SECRET, star);
    let response = harness
        .post_raw(
            "/_liyasa/hooks/github",
            &[
                ("x-github-event", "star"),
                ("x-github-delivery", "d-0006"),
                ("x-hub-signature-256", &signature),
            ],
            star,
        )
        .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(body_json(response).await["status"], "ignored");

    assert_eq!(
        DeployQueue::new(harness.store.clone())
            .depth()
            .await
            .expect("a depth"),
        0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_push_to_a_repository_no_project_is_bound_to_is_ignored_rather_than_refused() {
    let harness = Harness::build("git01-unbound", |state, _| {
        state.with_hooks(Hooks::new().with_secret(Provider::GitHub, SECRET))
    })
    .await;
    let response = post_delivery(&harness, "d-0007", PUSH).await;
    assert_eq!(
        response.status(),
        StatusCode::ACCEPTED,
        "a verified delivery for someone else's repository is not an error"
    );
    assert_eq!(body_json(response).await["status"], "ignored");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_provider_with_no_secret_configured_says_so_rather_than_failing_the_signature() {
    let harness = Harness::build("git01-unconfigured", |state, _| state).await;
    let response = post_delivery(&harness, "d-0008", PUSH).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let listed = body_json(harness.get("/_liyasa/hooks").await).await;
    assert_eq!(
        listed["providers"].as_array().map(Vec::len),
        Some(0),
        "and the endpoint says which providers are in service"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_check_run_the_build_reports_carries_its_conclusion_and_annotations() {
    // What the worker does once the build the push started has finished.
    let recorded = Arc::new(Recorded::new(vec![Exchange::post(
        "/repos/kasecrab/liyasa/check-runs",
        201,
        r#"{"id":1122334455,"html_url":"https://github.com/kasecrab/liyasa/runs/1122334455"}"#,
    )]));
    let github = GitHub::new(
        Endpoint::github_com(),
        recorded.clone(),
        Arc::new(StaticToken::new("ghs_installation_token")),
    );

    let run = CheckRun::completed(
        "liyasa",
        "9f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c",
        CheckConclusion::Failure,
    )
    .with_output("1 error, 3 warnings", "E0401 link target does not exist")
    .with_details_url("https://liyasa.example/builds/1")
    .with_annotations(vec![Annotation {
        path: "docs/reference/limits.md".to_owned(),
        start_line: 12,
        end_line: 12,
        level: "failure".to_owned(),
        message: "E0401 link target does not exist: /guides/gone".to_owned(),
        title: Some("E0401".to_owned()),
    }]);

    let reference = github
        .report_check(&RepoRef::new("kasecrab", "liyasa"), &run)
        .await
        .expect("the check run posts");
    assert_eq!(reference.id, "1122334455");
    assert_eq!(
        reference.url,
        "https://github.com/kasecrab/liyasa/runs/1122334455"
    );

    let made = recorded.made();
    assert_eq!(made.len(), 1, "one call, not a poll loop");
    assert_eq!(
        made[0].url,
        "https://api.github.com/repos/kasecrab/liyasa/check-runs"
    );
    assert_eq!(
        made[0].header("authorization"),
        Some("Bearer ghs_installation_token")
    );
    assert_eq!(made[0].header("x-github-api-version"), Some(API_VERSION));
    let body = made[0].json();
    assert_eq!(body["head_sha"], "9f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c");
    assert_eq!(body["conclusion"], "failure");
    assert_eq!(
        body["output"]["annotations"][0]["path"],
        "docs/reference/limits.md"
    );
    assert_eq!(
        body["output"]["annotations"][0]["annotation_level"],
        "failure"
    );
    assert_eq!(recorded.unused(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_passing_build_reports_success_against_the_same_commit() {
    let recorded = Arc::new(Recorded::new(vec![Exchange::post(
        "/repos/kasecrab/liyasa/check-runs",
        201,
        r#"{"id":1,"html_url":"https://github.com/kasecrab/liyasa/runs/1"}"#,
    )]));
    let github = GitHub::new(
        Endpoint::github_com(),
        recorded.clone(),
        Arc::new(StaticToken::new("ghs_installation_token")),
    );
    github
        .report_check(
            &RepoRef::new("kasecrab", "liyasa"),
            &CheckRun::completed(
                "liyasa",
                "9f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c",
                CheckConclusion::Success,
            )
            .with_output("42 of 42 checks passed", ""),
        )
        .await
        .expect("the check run posts");
    let body = recorded.made()[0].json();
    assert_eq!(body["conclusion"], "success");
    assert!(
        CheckConclusion::Success.passes(),
        "a required check with this conclusion lets the merge through"
    );
}
