//! GIT-40: roll production back to any retained build, in one action.
//!
//! Given three retained builds; when a rollback to the first is requested;
//! then production serves it, history gains a new record, and "return to
//! latest" restores the newest.
//!
//! History never rewrites, which is the part worth pinning: a rollback is an
//! *append* that happens to point backwards, so the record of what was live
//! and when survives the rollback.

use std::sync::Arc;

use http::StatusCode;
use liyasa_core::store::Page;
use liyasa_server::deploy::rollback::{Actor, Policy, Rollback};
use liyasa_tests::deploy::{Harness, RecordingAudit, body_json};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rollback_serves_the_old_build_and_return_to_latest_restores_the_newest() {
    let audit = Arc::new(RecordingAudit::default());
    let recorded = audit.clone();
    let harness = Harness::build("rollback", move |state, _| {
        let store = state.app.store.clone().expect("a store");
        state.with_rollback(Rollback::new(store).with_audit(recorded))
    })
    .await;
    let project = harness.project;

    let first = harness.build_record("production", "b1", 1_000).await;
    let second = harness.build_record("production", "b2", 2_000).await;
    let third = harness.build_record("production", "b3", 3_000).await;

    // Three deploys, in order, as three pushes would have made them.
    for build in [first, second, third] {
        harness
            .store
            .deployments_typed()
            .point(&project, "production", &build)
            .await
            .expect("a deploy");
    }
    assert_eq!(
        harness
            .store
            .deployments_typed()
            .current(&project, "production")
            .await
            .expect("a pointer")
            .map(|record| record.build),
        Some(third)
    );

    // All three are offered as rollback targets.
    let retained = body_json(
        harness
            .get(&format!(
                "/_liyasa/api/v1/deployments/production/retained?project={project}"
            ))
            .await,
    )
    .await;
    let offered: Vec<&str> = retained["items"]
        .as_array()
        .expect("the retained builds")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(offered.len(), 3);
    assert!(offered.contains(&third.to_string().as_str()));
    assert!(offered.contains(&first.to_string().as_str()));

    let before = harness
        .store
        .deployments_typed()
        .history(
            &project,
            "production",
            &Page {
                cursor: None,
                limit: 50,
            },
        )
        .await
        .expect("the history")
        .len();

    // Roll back to the first.
    let rolled = body_json(
        harness
            .post(
                &format!(
                    "/_liyasa/api/v1/deployments/production/rollback/{first}?project={project}"
                ),
                serde_json::json!({}),
            )
            .await,
    )
    .await;
    assert_eq!(rolled["buildId"], first.to_string());
    assert_eq!(rolled["previousBuildId"], third.to_string());

    assert_eq!(
        harness
            .store
            .deployments_typed()
            .current(&project, "production")
            .await
            .expect("a pointer")
            .map(|record| record.build),
        Some(first),
        "production serves the build that was rolled back to"
    );

    let after = harness
        .store
        .deployments_typed()
        .history(
            &project,
            "production",
            &Page {
                cursor: None,
                limit: 50,
            },
        )
        .await
        .expect("the history");
    assert_eq!(
        after.len(),
        before + 1,
        "history gains a record rather than losing one"
    );
    assert_eq!(after[0].build, first, "the newest record is the rollback");
    assert_eq!(
        after[1].build, third,
        "and the record of the build that was live is still there"
    );

    // Return to latest, in one action.
    let restored = body_json(
        harness
            .post(
                &format!("/_liyasa/api/v1/deployments/production/latest?project={project}"),
                serde_json::json!({}),
            )
            .await,
    )
    .await;
    assert_eq!(
        restored["buildId"],
        third.to_string(),
        "the newest build, not the newest history entry"
    );
    assert_eq!(restored["previousBuildId"], first.to_string());

    // Both actions are audited, with who and what.
    let entries = audit.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].action, "deployment.rollback");
    assert_eq!(entries[1].action, "deployment.return_to_latest");
    assert!(
        entries[0].detail.contains(&first.to_string()),
        "{:?}",
        entries[0]
    );
    assert!(entries[0].subject.ends_with("/production"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_build_that_is_no_longer_retained_is_refused_rather_than_pointed_at() {
    let harness = Harness::build("rollback-gone", |state, _| state).await;
    let project = harness.project;
    let live = harness.build_record("production", "live", 2_000).await;
    harness
        .store
        .deployments_typed()
        .point(&project, "production", &live)
        .await
        .expect("a deploy");

    // A build that was never deployed here, and whose record does not exist.
    let stranger = liyasa_core::ids::BuildId(liyasa_core::ids::Fingerprint::of(b"stranger"));
    let response = harness
        .post(
            &format!(
                "/_liyasa/api/v1/deployments/production/rollback/{stranger}?project={project}"
            ),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = body_json(response).await;
    assert_eq!(body["code"], "E0805");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_admin_only_policy_refuses_a_caller_with_no_authenticated_actor() {
    let harness = Harness::build("rollback-admin", |state, _| {
        let store = state.app.store.clone().expect("a store");
        state.with_rollback(Rollback::new(store).with_policy(Policy { admins_only: true }))
    })
    .await;
    let project = harness.project;
    let build = harness.build_record("production", "only", 1_000).await;
    harness
        .store
        .deployments_typed()
        .point(&project, "production", &build)
        .await
        .expect("a deploy");

    let response = harness
        .post(
            &format!("/_liyasa/api/v1/deployments/production/latest?project={project}"),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "an unauthenticated caller is not an administrator"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returning_to_latest_when_already_there_says_so_rather_than_moving_the_pointer() {
    let harness = Harness::build("rollback-noop", |state, _| state).await;
    let project = harness.project;
    let build = harness.build_record("production", "newest", 1_000).await;
    harness
        .store
        .deployments_typed()
        .point(&project, "production", &build)
        .await
        .expect("a deploy");

    let response = harness
        .post(
            &format!("/_liyasa/api/v1/deployments/production/latest?project={project}"),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let history = harness
        .store
        .deployments_typed()
        .history(
            &project,
            "production",
            &Page {
                cursor: None,
                limit: 50,
            },
        )
        .await
        .expect("the history");
    assert_eq!(history.len(), 1, "a refused action writes no history");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rollback_needs_a_project_and_says_so() {
    let harness = Harness::build("rollback-noproject", |state, _| state).await;
    let response = harness
        .post(
            "/_liyasa/api/v1/deployments/production/latest",
            serde_json::json!({}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let _ = Actor::member("x");
}
