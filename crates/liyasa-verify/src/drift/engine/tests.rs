use std::collections::BTreeSet;
use std::time::{Duration, SystemTime};

use liyasa_core::document::{Edge, EdgeOrigin};
use liyasa_core::ids::{CheckId, FactId, Route};
use liyasa_core::verify::{ChangeKind, FactValue, StoreError, VerifyPolicy};

use crate::core::config::{DriftConfig, DriftSeverity};
use crate::core::policy::{CheckClass, Policy, PolicyLevel};
use crate::drift::record::{Candidate, DriftKey, DriftKind, DriftState, Resolution};
use crate::drift::store::{MemoryDrift, RecordStore};

use super::{Coverage, Engine, Routes, policy_of};

/// The pairing a graph would answer with; the engine never asks for more.
struct Pairs;

impl Routes for Pairs {
    fn routes(&self, blocks: &[(EdgeOrigin, Vec<Edge>)]) -> Result<Vec<Route>, StoreError> {
        assert!(blocks.is_empty(), "these tests build candidates directly");
        Ok(Vec::new())
    }
}

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

fn fact(name: &str, new: f64, change: ChangeKind) -> Candidate {
    Candidate::new(
        DriftKind::Fact {
            fact: FactId::new(name),
            old: Some(FactValue::Num(20.0)),
            new: Some(FactValue::Num(new)),
            change,
        },
        vec![Route::new("/pricing")],
    )
}

fn link(url: &str) -> Candidate {
    Candidate::new(
        DriftKind::Link {
            url: url.to_owned(),
            reason: "404".to_owned(),
            failing_since: at(0),
        },
        vec![Route::new("/install")],
    )
}

fn engine<'a>(
    store: &'a MemoryDrift,
    config: &'a DriftConfig,
    routes: &'a Pairs,
    seconds: u64,
) -> Engine<'a> {
    Engine::new(store, config, routes).at(at(seconds))
}

#[test]
fn a_first_sighting_opens_a_record_and_a_second_identical_one_changes_nothing() {
    let store = MemoryDrift::new();
    let config = DriftConfig::default();
    let routes = Pairs;
    let policy = Policy::new();

    let first = engine(&store, &config, &routes, 100)
        .record(
            &[fact("plan.pro.price", 25.0, ChangeKind::Changed)],
            &policy,
        )
        .expect("a write");
    assert_eq!((first.created, first.updated, first.resolved), (1, 0, 0));

    let again = engine(&store, &config, &routes, 200)
        .record(
            &[fact("plan.pro.price", 25.0, ChangeKind::Changed)],
            &policy,
        )
        .expect("a write");
    assert_eq!((again.created, again.updated, again.resolved), (0, 0, 0));
    assert_eq!(store.len(), 1);

    let stored = store
        .find(&DriftKey::Fact(FactId::new("plan.pro.price")))
        .expect("a read")
        .expect("the record");
    assert_eq!(stored.first_seen, at(100));
    assert_eq!(stored.severity, DriftSeverity::Medium);
}

#[test]
fn a_new_value_for_the_same_fact_updates_the_record_it_already_has() {
    let store = MemoryDrift::new();
    let config = DriftConfig::default();
    let routes = Pairs;
    let policy = Policy::new();

    engine(&store, &config, &routes, 100)
        .record(
            &[fact("plan.pro.price", 25.0, ChangeKind::Changed)],
            &policy,
        )
        .expect("a write");
    let second = engine(&store, &config, &routes, 200)
        .record(
            &[fact("plan.pro.price", 30.0, ChangeKind::Changed)],
            &policy,
        )
        .expect("a write");

    assert_eq!((second.created, second.updated, second.resolved), (0, 1, 0));
    assert_eq!(store.len(), 1);
    let stored = store
        .find(&DriftKey::Fact(FactId::new("plan.pro.price")))
        .expect("a read")
        .expect("the record");
    assert_eq!(stored.first_seen, at(100), "the record is the same record");
    assert_eq!(stored.last_seen, at(200));
    assert!(matches!(
        stored.kind,
        DriftKind::Fact { new: Some(FactValue::Num(value)), .. } if value == 30.0
    ));
}

#[test]
fn a_subject_a_run_did_not_look_at_is_not_resolved_by_its_silence() {
    let store = MemoryDrift::new();
    let config = DriftConfig::default();
    let routes = Pairs;
    let policy = Policy::new();

    engine(&store, &config, &routes, 100)
        .record(
            &[fact("plan.pro.price", 25.0, ChangeKind::Changed)],
            &policy,
        )
        .expect("a write");

    // A run that refreshed a different source says nothing about this one.
    let elsewhere = engine(&store, &config, &routes, 200)
        .covering(Coverage::Subjects(BTreeSet::from([DriftKey::Fact(
            FactId::new("plan.team.price"),
        )])))
        .record(&[], &policy)
        .expect("a write");
    assert_eq!(elsewhere.resolved, 0);
    assert_eq!(store.open_records().expect("a read").len(), 1);
    assert_eq!(
        store
            .find(&DriftKey::Fact(FactId::new("plan.pro.price")))
            .expect("a read")
            .expect("the record")
            .gone_since,
        None,
        "and nothing suggests it looks fixed"
    );

    // The default says nothing about anything — and it has to hold with
    // `autoResolve` on, because that is the setting under which reading silence
    // as "fixed" would close the record rather than only flag it.
    let auto = DriftConfig {
        auto_resolve: true,
        ..DriftConfig::default()
    };
    let observed = engine(&store, &auto, &routes, 300)
        .record(&[], &policy)
        .expect("a write");
    assert_eq!(observed.resolved, 0);
    assert_eq!(store.open_records().expect("a read").len(), 1);
    assert_eq!(
        store
            .find(&DriftKey::Fact(FactId::new("plan.pro.price")))
            .expect("a read")
            .expect("the record")
            .gone_since,
        None,
        "a run that looked at nothing has not found anything fixed"
    );
}

#[test]
fn a_covered_subject_that_stopped_drifting_is_marked_and_auto_resolve_decides_whether_it_closes() {
    let store = MemoryDrift::new();
    let config = DriftConfig::default();
    let routes = Pairs;
    let policy = Policy::new();
    let key = DriftKey::Fact(FactId::new("plan.pro.price"));

    engine(&store, &config, &routes, 100)
        .record(
            &[fact("plan.pro.price", 25.0, ChangeKind::Changed)],
            &policy,
        )
        .expect("a write");

    // `autoResolve` defaults to false: the record is flagged as looking fixed
    // and stays open for an owner.
    let swept = engine(&store, &config, &routes, 200)
        .covering(Coverage::Everything)
        .record(&[], &policy)
        .expect("a write");
    assert_eq!(swept.resolved, 0);
    let stored = store.find(&key).expect("a read").expect("the record");
    assert!(stored.is_open());
    assert_eq!(stored.gone_since, Some(at(200)));

    // And a second silent sweep does not move the date it was first seen gone.
    engine(&store, &config, &routes, 300)
        .covering(Coverage::Everything)
        .record(&[], &policy)
        .expect("a write");
    assert_eq!(
        store
            .find(&key)
            .expect("a read")
            .expect("the record")
            .gone_since,
        Some(at(200))
    );

    let auto = DriftConfig {
        auto_resolve: true,
        ..DriftConfig::default()
    };
    let closed = engine(&store, &auto, &routes, 400)
        .covering(Coverage::Everything)
        .record(&[], &policy)
        .expect("a write");
    assert_eq!(closed.resolved, 1);
    let stored = store.find(&key).expect("a read").expect("the record");
    assert!(!stored.is_open());
    assert_eq!(stored.resolution, Some(Resolution::Fixed));
    assert_eq!(stored.resolved_at, Some(at(400)));
}

#[test]
fn a_subject_that_drifts_again_after_it_was_fixed_is_new_drift_not_an_update() {
    let store = MemoryDrift::new();
    let config = DriftConfig {
        auto_resolve: true,
        ..DriftConfig::default()
    };
    let routes = Pairs;
    let policy = Policy::new();
    let key = DriftKey::Fact(FactId::new("plan.pro.price"));

    engine(&store, &config, &routes, 100)
        .record(
            &[fact("plan.pro.price", 25.0, ChangeKind::Changed)],
            &policy,
        )
        .expect("a write");
    engine(&store, &config, &routes, 200)
        .covering(Coverage::Everything)
        .record(&[], &policy)
        .expect("a write");
    assert!(!store.find(&key).expect("a read").expect("it").is_open());

    let again = engine(&store, &config, &routes, 300)
        .record(
            &[fact("plan.pro.price", 40.0, ChangeKind::Changed)],
            &policy,
        )
        .expect("a write");
    assert_eq!((again.created, again.updated, again.resolved), (1, 0, 0));
    let stored = store.find(&key).expect("a read").expect("the record");
    assert!(stored.is_open());
    assert_eq!(stored.first_seen, at(300), "a reopened record starts again");
    assert_eq!(stored.gone_since, None);
    assert_eq!(stored.resolution, None);
    assert_eq!(store.len(), 1, "and it is still one record");
}

#[test]
fn a_class_turned_off_produces_no_record_of_the_kinds_that_belong_to_it() {
    let store = MemoryDrift::new();
    let config = DriftConfig::default();
    let routes = Pairs;
    let off = Policy::new().with(CheckClass::Links, PolicyLevel::Off);

    let report = engine(&store, &config, &routes, 100)
        .record(
            &[
                link("https://example.com/gone"),
                fact("plan.pro.price", 25.0, ChangeKind::Changed),
            ],
            &off,
        )
        .expect("a write");
    assert_eq!(report.created, 1, "the fact, not the link");
    assert_eq!(
        store.find(&DriftKey::Link("https://example.com/gone".to_owned())),
        Ok(None)
    );

    // With links on, the same pair records both.
    let store = MemoryDrift::new();
    let report = engine(&store, &config, &routes, 100)
        .record(
            &[
                link("https://example.com/gone"),
                fact("plan.pro.price", 25.0, ChangeKind::Changed),
            ],
            &Policy::new(),
        )
        .expect("a write");
    assert_eq!(report.created, 2);
}

#[test]
fn a_subject_that_reaches_no_page_is_an_answer_and_not_a_record() {
    let store = MemoryDrift::new();
    let config = DriftConfig::default();
    let routes = Pairs;
    let unread = Candidate::new(
        DriftKind::Fact {
            fact: FactId::new("plan.enterprise.price"),
            old: None,
            new: Some(FactValue::Num(99.0)),
            change: ChangeKind::Changed,
        },
        Vec::new(),
    );
    let report = engine(&store, &config, &routes, 100)
        .record(&[unread], &Policy::new())
        .expect("a write");
    assert_eq!(report.created, 0);
    assert!(store.is_empty());
}

#[test]
fn an_owner_closing_a_record_says_who_and_a_record_already_closed_is_not_closed_twice() {
    let store = MemoryDrift::new();
    let config = DriftConfig::default();
    let routes = Pairs;
    let key = DriftKey::Fact(FactId::new("plan.pro.price"));

    engine(&store, &config, &routes, 100)
        .record(
            &[fact("plan.pro.price", 25.0, ChangeKind::Changed)],
            &Policy::new(),
        )
        .expect("a write");

    let approved = Resolution::Approved {
        by: "docs@example.com".to_owned(),
    };
    assert_eq!(
        engine(&store, &config, &routes, 200).resolve(&key, approved.clone()),
        Ok(true)
    );
    let stored = store.find(&key).expect("a read").expect("the record");
    assert_eq!(stored.state, DriftState::Resolved);
    assert_eq!(stored.resolution, Some(approved.clone()));

    assert_eq!(
        engine(&store, &config, &routes, 300).resolve(&key, approved),
        Ok(false)
    );
    assert_eq!(
        engine(&store, &config, &routes, 300).resolve(
            &DriftKey::Check(CheckId::new("/nope#ab#0")),
            Resolution::Vanished
        ),
        Ok(false),
        "and an unknown subject is not an error"
    );
}

#[test]
fn the_frozen_policy_value_is_read_for_its_classes_and_not_for_fail_on() {
    let verify: VerifyPolicy =
        serde_json::from_value(serde_json::json!({ "fail_on": "error", "links": "off" }))
            .expect("a policy");
    let policy = policy_of(&verify);
    assert_eq!(policy.level(CheckClass::Links), PolicyLevel::Off);
    // Untouched classes fall through to VER-71's table.
    assert_eq!(policy.level(CheckClass::Facts), PolicyLevel::Error);
    assert_eq!(policy.declared(CheckClass::Code), None);
}
