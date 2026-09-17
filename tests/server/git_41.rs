//! GIT-41: the instant switch.
//!
//! Given a rollback; when timed; then the pointer switch completes in under
//! one second and the CDN purge is requested by tag.
//!
//! The budget is the *pointer switch*, measured around the store write, not
//! the whole request: what GIT-41 promises is that rolling back does not
//! rebuild or copy anything, and a wall-clock assertion over an HTTP round
//! trip on a loaded CI box would measure the box rather than the promise. The
//! response carries the measured switch so the claim is observable from
//! outside as well.

use std::sync::Arc;
use std::time::Duration;

use http::StatusCode;
use liyasa_server::deploy::rollback::{Rollback, cache_tag};
use liyasa_tests::deploy::{Harness, RecordingPurge, body_json};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rollback_is_a_pointer_change_under_a_second_and_purges_by_tag() {
    let purge = Arc::new(RecordingPurge::default());
    let recorded = purge.clone();
    let harness = Harness::build("switch", move |state, _| {
        let store = state.app.store.clone().expect("a store");
        state.with_rollback(Rollback::new(store).with_purge(recorded))
    })
    .await;
    let project = harness.project;

    let old = harness.build_record("production", "old", 1_000).await;
    let new = harness.build_record("production", "new", 2_000).await;
    for build in [old, new] {
        harness
            .store
            .deployments_typed()
            .point(&project, "production", &build)
            .await
            .expect("a deploy");
    }

    let body = body_json(
        harness
            .post(
                &format!("/_liyasa/api/v1/deployments/production/rollback/{old}?project={project}"),
                serde_json::json!({}),
            )
            .await,
    )
    .await;

    let switch = Duration::from_millis(body["switchMs"].as_u64().expect("the measured switch"));
    assert!(
        switch < Duration::from_secs(1),
        "the pointer switch took {switch:?}, over GIT-41's one-second budget"
    );

    let tag = cache_tag(&project, "production");
    assert_eq!(body["purgedTag"], tag);
    assert_eq!(
        purge.tags(),
        vec![tag],
        "exactly one purge, by tag rather than by URL"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_purge_covers_the_whole_environment_however_many_pages_it_has() {
    let purge = Arc::new(RecordingPurge::default());
    let recorded = purge.clone();
    let harness = Harness::build("switch-one", move |state, _| {
        let store = state.app.store.clone().expect("a store");
        state.with_rollback(Rollback::new(store).with_purge(recorded))
    })
    .await;
    let project = harness.project;
    let old = harness.build_record("production", "a", 1_000).await;
    let new = harness.build_record("production", "b", 2_000).await;
    for build in [old, new] {
        harness
            .store
            .deployments_typed()
            .point(&project, "production", &build)
            .await
            .expect("a deploy");
    }
    harness
        .post(
            &format!("/_liyasa/api/v1/deployments/production/rollback/{old}?project={project}"),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(purge.tags().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_edge_that_refuses_the_purge_does_not_undo_the_switch() {
    let purge = Arc::new(RecordingPurge::failing());
    let recorded = purge.clone();
    let harness = Harness::build("switch-fail", move |state, _| {
        let store = state.app.store.clone().expect("a store");
        state.with_rollback(Rollback::new(store).with_purge(recorded))
    })
    .await;
    let project = harness.project;
    let old = harness.build_record("production", "a", 1_000).await;
    let new = harness.build_record("production", "b", 2_000).await;
    for build in [old, new] {
        harness
            .store
            .deployments_typed()
            .point(&project, "production", &build)
            .await
            .expect("a deploy");
    }

    let response = harness
        .post(
            &format!("/_liyasa/api/v1/deployments/production/rollback/{old}?project={project}"),
            serde_json::json!({}),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::BAD_GATEWAY,
        "the caller is told the edge is stale"
    );
    assert_eq!(
        harness
            .store
            .deployments_typed()
            .current(&project, "production")
            .await
            .expect("a pointer")
            .map(|record| record.build),
        Some(old),
        "and the origin serves the build that was asked for"
    );
    assert_eq!(purge.tags().len(), 1, "the purge was attempted");
}
