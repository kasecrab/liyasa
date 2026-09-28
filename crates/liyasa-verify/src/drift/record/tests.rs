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
        owner: "docs@example.com".to_owned(),
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
