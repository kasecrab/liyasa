//! The frozen `Store` contract over the typed repositories (RFC 1400).

use std::time::Duration;

use liyasa_core::ids::{BuildId, Fingerprint, ProjectId};
use liyasa_core::store::{BuildQuery, BuildStatus, JobQuery, Page, Project, ProjectQuery, Store};
use liyasa_store::records::BuildRecord;
use liyasa_store::{Enqueue, IngestQueue, MasterKey, SqliteStore};

use crate::support::TempDir;

async fn store(name: &str) -> (TempDir, SqliteStore) {
    let dir = TempDir::new(name);
    let store = SqliteStore::open(
        &dir.join("liyasa.db"),
        MasterKey::generate().expect("a key"),
        IngestQueue::new(100, 10),
    )
    .await
    .expect("a store");
    (dir, store)
}

#[tokio::test]
async fn the_contract_answers_existence_listing_and_deletion() {
    let (_dir, store) = store("facade-basics").await;
    let project = store
        .projects_typed()
        .create("acme-docs", "Acme")
        .await
        .expect("a project");

    let repo = store.projects();
    assert!(repo.get(&project.id).await.expect("a read").is_some());
    assert!(
        repo.get(&ProjectId(liyasa_store::new_ulid()))
            .await
            .expect("a read")
            .is_none()
    );
    assert_eq!(
        repo.list(&ProjectQuery::default(), Page::default())
            .await
            .expect("a listing")
            .len(),
        1
    );

    let build = BuildId(Fingerprint::of("one"));
    store
        .builds_typed()
        .put(&BuildRecord {
            id: build,
            project: project.id,
            env: "production".to_owned(),
            status: BuildStatus::Succeeded,
            dist: "bundles/one".to_owned(),
            created_at: liyasa_store::now_ms(),
            updated_at: 0,
            version: 1,
        })
        .await
        .expect("a build");
    assert!(
        store
            .builds()
            .latest_for(&project.id, "production")
            .await
            .expect("a read")
            .is_some()
    );
    assert_eq!(
        store
            .builds()
            .list(&BuildQuery::default(), Page::default())
            .await
            .expect("a listing")
            .len(),
        1
    );

    store
        .deployments()
        .point("production", &build)
        .await
        .expect("a pointer swap");
    assert!(
        store
            .deployments()
            .get(&build)
            .await
            .expect("a read")
            .is_some()
    );

    repo.delete(&project.id).await.expect("a delete");
    assert!(repo.get(&project.id).await.expect("a read").is_none());
}

#[tokio::test]
async fn writing_a_field_less_entity_says_to_use_the_typed_api() {
    let (_dir, store) = store("facade-put").await;
    let error = store
        .projects()
        .put(&Project::default())
        .await
        .expect_err("a marker type carries nothing to write");
    assert!(error.to_string().contains("RFC 1400"), "{error}");
}

#[tokio::test]
async fn a_consumer_holding_the_contract_can_drive_the_job_queue() {
    let (_dir, store) = store("facade-jobs").await;
    let queue: &dyn Store = &store;
    let id = store
        .jobs_typed()
        .enqueue(&Enqueue::new("verify.nightly", "site"))
        .await
        .expect("a job")
        .id();

    assert!(
        queue
            .jobs()
            .claim("replica-a", Duration::from_secs(60))
            .await
            .expect("a claim")
            .is_some()
    );
    queue.jobs().heartbeat(&id).await.expect("a heartbeat");
    assert_eq!(
        queue
            .jobs()
            .list(&JobQuery::default(), Page::default())
            .await
            .expect("a listing")
            .len(),
        1
    );
}

#[tokio::test]
async fn the_truth_graph_repositories_are_empty_until_their_tables_land() {
    let (_dir, store) = store("facade-graph").await;
    assert!(
        store
            .drift()
            .open(&ProjectId(liyasa_store::new_ulid()))
            .await
            .expect("a read")
            .is_empty()
    );
    assert!(
        store
            .claims()
            .for_fact(&liyasa_core::ids::FactId::new("pricing.pro"))
            .await
            .expect("a read")
            .is_empty()
    );
    assert!(
        store
            .blocks()
            .lineage(&liyasa_core::ids::BlockId([0; 12]))
            .await
            .expect("a read")
            .is_empty()
    );
}

#[tokio::test]
async fn there_is_no_in_process_vector_index() {
    let (_dir, store) = store("facade-vectors").await;
    let error = store
        .vectors()
        .query(&[0.0; 4], 3, &liyasa_core::store::ChunkFilter::default())
        .await
        .expect_err("no index is configured");
    assert!(error.to_string().contains("vector index"), "{error}");
}
