use std::time::{Duration, SystemTime};

use liyasa_core::ids::{CheckId, FactId, Route};
use liyasa_core::verify::{ChangeKind, FactValue};

use crate::core::config::DriftSeverity;

use super::{DriftKey, DriftKind, escalated};

fn fact(change: ChangeKind) -> DriftKind {
    DriftKind::Fact {
        fact: FactId::new("plan.pro.price"),
        old: Some(FactValue::Num(20.0)),
        new: Some(FactValue::Num(25.0)),
        change,
    }
}

fn operation(diff: &[&str]) -> DriftKind {
    DriftKind::Operation {
        spec: "petstore".to_owned(),
        op: "listPets".to_owned(),
        diff: diff.iter().copied().map(ToOwned::to_owned).collect(),
    }
}

fn review(page: &str, overdue: Duration) -> DriftKind {
    DriftKind::Review {
        page: Route::new(page),
        owners: vec!["docs@example.com".to_owned()],
        reviewed: Some(SystemTime::UNIX_EPOCH),
        cadence: Duration::from_secs(180 * 86_400),
        overdue_by: overdue,
    }
}

#[test]
fn a_removed_fact_outranks_a_changed_one_and_an_added_one_is_not_drift_by_default() {
    assert_eq!(
        fact(ChangeKind::Removed).base_severity(),
        DriftSeverity::High
    );
    assert_eq!(
        fact(ChangeKind::Changed).base_severity(),
        DriftSeverity::Medium
    );
    // Below the default `severity_threshold`, which is what makes a fact that
    // appeared for the first time not open a record.
    assert_eq!(fact(ChangeKind::Added).base_severity(), DriftSeverity::Low);
    assert!(DriftSeverity::Low < DriftSeverity::default());
}

#[test]
fn an_operation_is_graded_by_the_worst_facet_that_moved() {
    assert_eq!(
        operation(&["responses"]).base_severity(),
        DriftSeverity::Medium
    );
    assert_eq!(
        operation(&["responses", "auth"]).base_severity(),
        DriftSeverity::High
    );
    assert_eq!(
        operation(&["removed"]).base_severity(),
        DriftSeverity::Critical
    );
    assert_eq!(operation(&["added"]).base_severity(), DriftSeverity::Low);
    // A facet a later differ learns to report grades as the mildest thing
    // rather than crashing or as the worst.
    assert_eq!(operation(&["examples"]).base_severity(), DriftSeverity::Low);
}

#[test]
fn a_review_overdue_by_more_than_its_cadence_again_outranks_one_just_past_it() {
    assert_eq!(
        review("/pricing", Duration::from_secs(86_400)).base_severity(),
        DriftSeverity::Medium
    );
    assert_eq!(
        review("/pricing", Duration::from_secs(200 * 86_400)).base_severity(),
        DriftSeverity::High
    );
}

#[test]
fn escalation_saturates_rather_than_wrapping() {
    assert_eq!(escalated(DriftSeverity::Low), DriftSeverity::Medium);
    assert_eq!(escalated(DriftSeverity::Medium), DriftSeverity::High);
    assert_eq!(escalated(DriftSeverity::High), DriftSeverity::Critical);
    assert_eq!(escalated(DriftSeverity::Critical), DriftSeverity::Critical);
}

#[test]
fn a_keys_identity_is_the_subject_and_a_reviews_subject_is_its_page() {
    // The same fact moving twice is one record, whatever the values were.
    assert_eq!(
        fact(ChangeKind::Changed).key(),
        fact(ChangeKind::Removed).key()
    );
    assert_eq!(
        fact(ChangeKind::Changed).key(),
        DriftKey::Fact(FactId::new("plan.pro.price"))
    );

    // Two pages overdue for review are two records, not one.
    let overdue = Duration::from_secs(1);
    assert_ne!(review("/a", overdue).key(), review("/b", overdue).key());
    assert_eq!(
        review("/a", overdue).key(),
        DriftKey::Review(Route::new("/a"))
    );

    assert_eq!(
        DriftKind::Check {
            check: CheckId::new("/install#ab#0"),
            excerpt: "exit 1".to_owned(),
        }
        .key(),
        DriftKey::Check(CheckId::new("/install#ab#0"))
    );
}

// ---- persistence (RFC 2066) ----

use liyasa_core::document::{DepTarget, Edge, EdgeKind, EdgeOrigin};
use liyasa_core::ids::{BlockId, PageId};

use super::{Candidate, DriftRecord, DriftState, Resolution};

/// One of every `DriftKind`.
///
/// Kept beside [`the_kind_list_is_exhaustive_by_construction`], which is what
/// makes adding a variant without a round-trip case a compile error rather than
/// an untested arm.
fn one_of_every_kind() -> Vec<DriftKind> {
    vec![
        fact(ChangeKind::Changed),
        operation(&["responses", "auth"]),
        DriftKind::Link {
            url: "https://example.com/gone".to_owned(),
            reason: "404 not found".to_owned(),
            failing_since: SystemTime::UNIX_EPOCH + Duration::from_millis(1_500),
        },
        DriftKind::Check {
            check: CheckId::new("/install#aabbccddeeff001122334455#0"),
            excerpt: "exit 1".to_owned(),
        },
        review("/pricing", Duration::from_secs(90 * 86_400)),
    ]
}

/// The guard WP-14 asked for: a new `DriftKind` cannot be persisted silently.
///
/// `#[non_exhaustive]` has no effect inside the defining crate, so this match is
/// exhaustive and has deliberately **no wildcard arm**. Adding a variant fails
/// to compile here, which is the signal to add it to [`one_of_every_kind`] and
/// give it a round-trip case.
///
/// This is not the only thing that stops a new variant slipping through, and it
/// is not even the first to fire. Verified 2026-10-01 by adding a variant:
/// compilation fails in four places in the library before it reaches this file
/// — `DriftKind::key`, `DriftKind::base_severity`, `engine::class_of` and
/// `entries::summary`, none of which has a wildcard either. What this test adds
/// is the serde-specific obligation: those four force you to *classify* a new
/// kind, and this one forces you to prove it survives a round trip.
///
/// The alternative WP-14 was weighing — a hand-rolled column mapping — needs a
/// catch-all arm, and a catch-all is how a new kind round-trips as something
/// else or vanishes. Derived serde has no catch-all, so a new variant persists
/// correctly the moment it compiles.
#[test]
fn the_kind_list_is_exhaustive_by_construction() {
    let kinds = one_of_every_kind();
    for kind in &kinds {
        match kind {
            DriftKind::Fact { .. } => {}
            DriftKind::Operation { .. } => {}
            DriftKind::Link { .. } => {}
            DriftKind::Check { .. } => {}
            DriftKind::Review { .. } => {}
        }
    }
    assert_eq!(kinds.len(), 5, "one case per variant, and no more");
}

#[test]
fn every_kind_round_trips_through_json() {
    for kind in one_of_every_kind() {
        let json = serde_json::to_string(&kind).expect("a kind serializes");
        let back: DriftKind = serde_json::from_str(&json).expect("and comes back");
        assert_eq!(back, kind, "{json}");
        // The key is derived from the kind, so a round trip that lost a field
        // would be visible here even if `PartialEq` were ever relaxed.
        assert_eq!(back.key(), kind.key());
        assert_eq!(back.base_severity(), kind.base_severity());
    }
}

fn full_record() -> DriftRecord {
    let origin = EdgeOrigin::Block(PageId(ulid::Ulid::from_bytes([3; 16])), BlockId([4; 12]));
    let mut record = Candidate::new(
        fact(ChangeKind::Removed),
        vec![Route::new("/pricing"), Route::new("/plans")],
    )
    .with_blocks(vec![(
        origin.clone(),
        vec![Edge {
            from: origin,
            to: DepTarget::Fact(FactId::new("plan.pro.price")),
            kind: EdgeKind::Reads,
        }],
    )])
    .with_weight(Some(912.5))
    .opened(
        DriftSeverity::High,
        SystemTime::UNIX_EPOCH + Duration::from_secs(10),
    );
    record.last_seen = SystemTime::UNIX_EPOCH + Duration::from_secs(99);
    record.gone_since = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(50));
    record.state = DriftState::Resolved;
    record.resolved_at = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(60));
    record.resolution = Some(Resolution::Approved {
        by: "docs@example.com".to_owned(),
    });
    record
}

#[test]
fn a_record_round_trips_with_its_evidence_and_every_optional_field_set() {
    let record = full_record();
    let json = serde_json::to_string(&record).expect("a record serializes");
    let back: DriftRecord = serde_json::from_str(&json).expect("and comes back");
    assert_eq!(back, record, "{json}");
    assert_eq!(back.blocks.len(), 1, "the evidence path survives");
    assert_eq!(back.weight, Some(912.5));
}

#[test]
fn a_record_round_trips_with_every_optional_field_absent() {
    // The open case, which is the one that actually gets stored.
    let open = Candidate::new(fact(ChangeKind::Changed), vec![Route::new("/pricing")])
        .opened(DriftSeverity::Medium, SystemTime::UNIX_EPOCH);
    assert_eq!(open.gone_since, None);
    assert_eq!(open.resolution, None);
    assert_eq!(open.weight, None);

    let json = serde_json::to_string(&open).expect("serializes");
    let back: DriftRecord = serde_json::from_str(&json).expect("comes back");
    assert_eq!(back, open, "{json}");
}

#[test]
fn an_instant_is_milliseconds_since_the_epoch_like_every_other_one_in_the_project() {
    // Not serde's own `{secs_since_epoch, nanos_since_epoch}` shape. A drift
    // record sits beside `Snapshot::taken_at` and `CheckResult::duration` in a
    // report, and one timestamp spelled differently from the others is a trap
    // for whoever reads them together (RFC 2066).
    let json = serde_json::to_value(full_record()).expect("serializes");
    assert_eq!(json["firstSeen"], serde_json::json!(10_000));
    assert_eq!(json["lastSeen"], serde_json::json!(99_000));
    assert_eq!(json["goneSince"], serde_json::json!(50_000));
    assert_eq!(json["resolvedAt"], serde_json::json!(60_000));

    // And absent really is null, not a zero instant — "never resolved" and
    // "resolved at the epoch" must not serialise the same.
    let open = Candidate::new(fact(ChangeKind::Changed), vec![Route::new("/p")])
        .opened(DriftSeverity::Medium, SystemTime::UNIX_EPOCH);
    let json = serde_json::to_value(open).expect("serializes");
    assert_eq!(json["resolvedAt"], serde_json::Value::Null);
    assert_eq!(json["firstSeen"], serde_json::json!(0));

    // A review's durations are milliseconds too.
    let json = serde_json::to_value(review("/pricing", Duration::from_secs(90 * 86_400)))
        .expect("serializes");
    assert_eq!(
        json["review"]["cadence"],
        serde_json::json!(180 * 86_400_000_u64)
    );
    assert_eq!(
        json["review"]["overdueBy"],
        serde_json::json!(90 * 86_400_000_u64)
    );
}

#[test]
fn a_key_round_trips_so_a_stored_row_can_be_found_by_it() {
    for key in one_of_every_kind().iter().map(DriftKind::key) {
        let json = serde_json::to_string(&key).expect("a key serializes");
        let back: DriftKey = serde_json::from_str(&json).expect("and comes back");
        assert_eq!(back, key, "{json}");
        assert_eq!(back.job_id(), key.job_id(), "and names the same record");
    }
}
