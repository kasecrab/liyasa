use std::time::{Duration, SystemTime};

use liyasa_core::conformance::block_on;
use liyasa_core::ids::{FactId, ProjectId, Route};
use liyasa_core::store::{Drift, DriftQuery, DriftRepo, Page, Repo};
use liyasa_core::verify::{ChangeKind, FactValue, StoreError};

use crate::core::config::DriftSeverity;
use crate::drift::record::{Candidate, DriftKey, DriftKind, DriftRecord, Resolution};

use super::{MemoryDrift, RecordStore};

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

fn record(fact: &str, page: &str) -> DriftRecord {
    Candidate::new(
        DriftKind::Fact {
            fact: FactId::new(fact),
            old: Some(FactValue::Num(20.0)),
            new: Some(FactValue::Num(25.0)),
            change: ChangeKind::Changed,
        },
        vec![Route::new(page)],
    )
    .opened(DriftSeverity::Medium, at(0))
}

fn project() -> ProjectId {
    ProjectId::parse("01ARZ3NDEKTSV4RRFFQ69G5FAV").expect("a ULID")
}

#[test]
fn a_second_write_of_one_subject_replaces_the_record_rather_than_adding_one() {
    let store = MemoryDrift::new();
    assert!(store.is_empty());
    store
        .save(&record("plan.pro.price", "/pricing"))
        .expect("a write");
    store
        .save(&record("plan.pro.price", "/plans"))
        .expect("a write");
    assert_eq!(store.len(), 1);
    assert_eq!(
        store
            .find(&DriftKey::Fact(FactId::new("plan.pro.price")))
            .expect("a read")
            .expect("the record")
            .pages,
        vec![Route::new("/plans")]
    );
    assert_eq!(
        store
            .find(&DriftKey::Fact(FactId::new("other")))
            .expect("a read"),
        None
    );
}

#[test]
fn the_identity_the_frozen_contract_names_a_record_by_is_the_subject_not_the_run() {
    let key = DriftKey::Fact(FactId::new("plan.pro.price"));
    // Derived, so a second process agrees without asking a store.
    assert_eq!(
        key.job_id(),
        DriftKey::Fact(FactId::new("plan.pro.price")).job_id()
    );
    assert_ne!(
        key.job_id(),
        DriftKey::Fact(FactId::new("plan.team.price")).job_id()
    );
    // And the tag is part of it, so a fact and a review of the same name are
    // two records.
    assert_ne!(
        DriftKey::Link("/pricing".to_owned()).job_id(),
        DriftKey::Review(Route::new("/pricing")).job_id()
    );

    let store = MemoryDrift::new();
    assert_eq!(block_on(Repo::get(&store, &key.job_id())), Ok(None));
    store
        .save(&record("plan.pro.price", "/pricing"))
        .expect("a write");
    assert_eq!(
        block_on(Repo::get(&store, &key.job_id())),
        Ok(Some(Drift::default()))
    );
}

#[test]
fn open_records_are_the_ones_nothing_has_resolved() {
    let store = MemoryDrift::new();
    store.save(&record("a", "/a")).expect("a write");
    let mut resolved = record("b", "/b");
    resolved.state = crate::drift::record::DriftState::Resolved;
    resolved.resolved_at = Some(at(10));
    resolved.resolution = Some(Resolution::Fixed);
    store.save(&resolved).expect("a write");

    assert_eq!(store.all().expect("a read").len(), 2);
    assert_eq!(store.open_records().expect("a read").len(), 1);

    // The frozen contract cannot carry a record, but it can carry the count,
    // and it counts the same rows.
    assert_eq!(block_on(store.open(&project())).expect("a read").len(), 1);
    let all = DriftQuery::default();
    assert_eq!(
        block_on(Repo::list(&store, &all, Page::default()))
            .expect("a read")
            .len(),
        2
    );
    let only_open = DriftQuery {
        open: Some(true),
        ..DriftQuery::default()
    };
    assert_eq!(
        block_on(Repo::list(&store, &only_open, Page::default()))
            .expect("a read")
            .len(),
        1
    );
}

#[test]
fn a_write_through_the_frozen_repo_is_refused_rather_than_silently_lost() {
    let store = MemoryDrift::new();
    assert_eq!(
        block_on(Repo::put(&store, &Default::default())),
        Err(StoreError::Conflict)
    );
    assert!(store.is_empty());
}

#[test]
fn deleting_by_the_frozen_id_removes_the_record_and_an_unknown_id_is_not_found() {
    let store = MemoryDrift::new();
    store.save(&record("a", "/a")).expect("a write");
    let id = DriftKey::Fact(FactId::new("a")).job_id();
    assert_eq!(block_on(Repo::delete(&store, &id)), Ok(()));
    assert!(store.is_empty());
    assert_eq!(
        block_on(Repo::delete(&store, &id)),
        Err(StoreError::NotFound)
    );
}
