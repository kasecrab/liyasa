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

/// The pairing a graph would answer with: one page, so `apply` has somewhere to
/// file what it finds.
struct Pairs;

const ONLY_PAGE: &str = "/pricing";

impl Routes for Pairs {
    fn routes(&self, blocks: &[(EdgeOrigin, Vec<Edge>)]) -> Result<Vec<Route>, StoreError> {
        Ok(if blocks.is_empty() {
            Vec::new()
        } else {
            vec![Route::new(ONLY_PAGE)]
        })
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

#[test]
fn the_frozen_entry_point_wires_fact_impacts_and_check_failures_together() {
    use liyasa_core::ids::{BlockId, Fingerprint, PageId};
    use liyasa_core::verify::{CheckOutcome, CheckResult, DriftEngine as _, FactChange, Impact};

    let store = MemoryDrift::new();
    let config = DriftConfig::default();
    let routes = Pairs;
    let policy = VerifyPolicy::default();

    let impacts = [Impact {
        change: FactChange {
            fact: FactId::new("plan.pro.price"),
            old: Some(FactValue::Num(20.0)),
            new: Some(FactValue::Num(25.0)),
            kind: ChangeKind::Changed,
        },
        blocks: vec![(
            EdgeOrigin::Block(PageId(ulid::Ulid::from_bytes([1; 16])), BlockId([1; 12])),
            Vec::new(),
        )],
    }];
    let checks = [CheckResult {
        id: CheckId::new("/install#aabbccddeeff001122334455#0"),
        outcome: CheckOutcome::Fail {
            excerpt: "exit 1".to_owned(),
        },
        duration: Duration::from_millis(2),
        digest: Fingerprint::of("c"),
    }];

    let engine = engine(&store, &config, &routes, 100);
    let report = engine
        .apply(&impacts, &checks, &policy, &store)
        .expect("the store it holds");
    assert_eq!((report.created, report.updated, report.resolved), (2, 0, 0));
    assert!(
        store
            .find(&DriftKey::Fact(FactId::new("plan.pro.price")))
            .expect("a read")
            .is_some()
    );
    assert!(
        store
            .find(&DriftKey::Check(CheckId::new(
                "/install#aabbccddeeff001122334455#0"
            )))
            .expect("a read")
            .is_some()
    );
}

#[test]
fn the_frozen_entry_point_refuses_a_store_it_is_not_the_engine_for() {
    use liyasa_core::verify::DriftEngine as _;

    let store = MemoryDrift::new();
    let elsewhere = MemoryDrift::new();
    let config = DriftConfig::default();
    let routes = Pairs;
    let engine = engine(&store, &config, &routes, 100);

    assert_eq!(
        engine.apply(&[], &[], &VerifyPolicy::default(), &elsewhere),
        Err(StoreError::Conflict),
        "a record written to the wrong store would be a silently different answer"
    );
    assert!(elsewhere.is_empty());
    // And the one it does hold is accepted.
    assert!(
        engine
            .apply(&[], &[], &VerifyPolicy::default(), &store)
            .is_ok()
    );
}

/// Counts which store method the engine actually calls.
///
/// `save_all` delegates to the *inner* store, so a `save` counted here can only
/// have come from the engine. Without this, batching is invisible: every
/// assertion about records in the store passes either way, which is how a seam
/// gets added and then quietly bypassed.
struct Counting {
    inner: MemoryDrift,
    saves: std::sync::atomic::AtomicUsize,
    batches: std::sync::atomic::AtomicUsize,
    sizes: std::sync::Mutex<Vec<usize>>,
}

impl Counting {
    fn new() -> Self {
        Self {
            inner: MemoryDrift::new(),
            saves: std::sync::atomic::AtomicUsize::new(0),
            batches: std::sync::atomic::AtomicUsize::new(0),
            sizes: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn counts(&self) -> (usize, usize, Vec<usize>) {
        use std::sync::atomic::Ordering::Relaxed;
        (
            self.saves.load(Relaxed),
            self.batches.load(Relaxed),
            self.sizes.lock().expect("not poisoned").clone(),
        )
    }
}

impl RecordStore for Counting {
    fn all(&self) -> Result<Vec<crate::drift::record::DriftRecord>, StoreError> {
        self.inner.all()
    }

    fn find(
        &self,
        key: &DriftKey,
    ) -> Result<Option<crate::drift::record::DriftRecord>, StoreError> {
        self.inner.find(key)
    }

    fn save(&self, record: &crate::drift::record::DriftRecord) -> Result<(), StoreError> {
        self.saves
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.inner.save(record)
    }

    fn save_all(&self, records: &[crate::drift::record::DriftRecord]) -> Result<(), StoreError> {
        self.batches
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.sizes.lock().expect("not poisoned").push(records.len());
        self.inner.save_all(records)
    }
}

#[test]
fn a_reconciliation_is_one_store_write_call_however_many_records_it_touches() {
    let store = Counting::new();
    let config = DriftConfig::default();
    let routes = Pairs;
    let policy = Policy::new();

    let report = Engine::new(&store, &config, &routes)
        .at(at(100))
        .record(
            &[
                fact("plan.pro.price", 25.0, ChangeKind::Changed),
                fact("plan.team.price", 15.0, ChangeKind::Changed),
                link("https://example.com/gone"),
            ],
            &policy,
        )
        .expect("a write");

    assert_eq!(report.created, 3);
    let (saves, batches, sizes) = store.counts();
    assert_eq!(batches, 1, "one call per reconciliation, not per record");
    assert_eq!(sizes, vec![3], "and it carried all three");
    assert_eq!(
        saves, 0,
        "the engine must not reach past the batch to the per-record method"
    );
}

#[test]
fn a_reconciliation_that_changes_nothing_still_makes_exactly_one_call() {
    // An empty batch rather than no call: a store that opens a transaction in
    // `save_all` gets a consistent shape on every run, and a caller counting
    // reconciliations is not left guessing.
    let store = Counting::new();
    let config = DriftConfig::default();
    let routes = Pairs;

    Engine::new(&store, &config, &routes)
        .at(at(100))
        .record(&[], &Policy::new())
        .expect("a write");

    let (saves, batches, sizes) = store.counts();
    assert_eq!((saves, batches), (0, 1));
    assert_eq!(sizes, vec![0]);
}

#[test]
fn closing_a_record_joins_the_same_batch_as_the_candidates() {
    let store = Counting::new();
    let auto = DriftConfig {
        auto_resolve: true,
        ..DriftConfig::default()
    };
    let routes = Pairs;
    let policy = Policy::new();

    // Open two.
    Engine::new(&store, &auto, &routes)
        .at(at(100))
        .record(
            &[
                fact("plan.pro.price", 25.0, ChangeKind::Changed),
                fact("plan.team.price", 15.0, ChangeKind::Changed),
            ],
            &policy,
        )
        .expect("a write");

    // One still drifting, one covered and gone: one update, one close, and both
    // in a single call.
    let report = Engine::new(&store, &auto, &routes)
        .at(at(200))
        .covering(Coverage::Everything)
        .record(
            &[fact("plan.pro.price", 30.0, ChangeKind::Changed)],
            &policy,
        )
        .expect("a write");

    assert_eq!((report.created, report.updated, report.resolved), (0, 1, 1));
    let (saves, batches, sizes) = store.counts();
    assert_eq!(batches, 2, "one per reconciliation");
    assert_eq!(
        sizes,
        vec![2, 2],
        "the second holds the update and the close"
    );
    assert_eq!(saves, 0);
}
