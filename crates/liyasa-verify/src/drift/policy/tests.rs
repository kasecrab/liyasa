use liyasa_core::ids::{FactId, Route};
use liyasa_core::verify::{ChangeKind, FactValue};

use crate::core::config::{DriftConfig, DriftSeverity};

use super::DriftPolicy;
use crate::drift::record::{Candidate, DriftKind};

fn routes(n: usize) -> Vec<Route> {
    (0..n).map(|i| Route::new(format!("/p{i:03}"))).collect()
}

fn candidate(change: ChangeKind, pages: usize) -> Candidate {
    Candidate::new(
        DriftKind::Fact {
            fact: FactId::new("plan.pro.price"),
            old: Some(FactValue::Num(20.0)),
            new: Some(FactValue::Num(25.0)),
            change,
        },
        routes(pages),
    )
}

#[test]
fn the_default_threshold_keeps_a_changed_fact_and_drops_an_added_one() {
    let config = DriftConfig::default();
    let policy = DriftPolicy::new(&config);
    assert_eq!(
        policy.grade(&candidate(ChangeKind::Changed, 1)),
        Some(DriftSeverity::Medium)
    );
    assert_eq!(policy.grade(&candidate(ChangeKind::Added, 1)), None);
}

#[test]
fn lowering_the_threshold_is_what_admits_an_added_fact() {
    let config = DriftConfig {
        severity_threshold: DriftSeverity::Low,
        ..DriftConfig::default()
    };
    assert_eq!(
        DriftPolicy::new(&config).grade(&candidate(ChangeKind::Added, 1)),
        Some(DriftSeverity::Low)
    );
}

#[test]
fn the_blast_radius_escalates_one_level_and_that_can_cross_the_threshold() {
    let config = DriftConfig::default();
    let policy = DriftPolicy::new(&config);
    // 25 pages is the default batch size, so 25 is not yet a split.
    assert_eq!(
        policy.grade(&candidate(ChangeKind::Changed, 25)),
        Some(DriftSeverity::Medium)
    );
    assert_eq!(
        policy.grade(&candidate(ChangeKind::Changed, 26)),
        Some(DriftSeverity::High)
    );
    // An added fact reaching a whole site is worth a record; one page is not.
    assert_eq!(
        policy.grade(&candidate(ChangeKind::Added, 26)),
        Some(DriftSeverity::Medium)
    );
}

#[test]
fn a_record_under_the_batch_size_is_one_batch_and_a_bigger_one_is_chunked() {
    let config = DriftConfig {
        batch_size: 2,
        ..DriftConfig::default()
    };
    let policy = DriftPolicy::new(&config);
    let pages = routes(5);

    assert!(!policy.splits(&pages[..2]));
    assert_eq!(policy.batches(&pages[..2]).len(), 1);

    assert!(policy.splits(&pages));
    let batches = policy.batches(&pages);
    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0], &pages[..2]);
    assert_eq!(batches[2], &pages[4..]);
    // Nothing is lost or repeated by the split.
    let flat: Vec<Route> = batches.concat();
    assert_eq!(flat, pages);
}

#[test]
fn a_zero_batch_size_never_splits_rather_than_panicking_in_chunks() {
    let config = DriftConfig {
        batch_size: 0,
        ..DriftConfig::default()
    };
    let policy = DriftPolicy::new(&config);
    let pages = routes(40);
    assert!(!policy.splits(&pages));
    assert_eq!(policy.batches(&pages), vec![pages.as_slice()]);
    // And with no split there is no escalation either.
    assert_eq!(
        policy.grade(&candidate(ChangeKind::Changed, 40)),
        Some(DriftSeverity::Medium)
    );
}
