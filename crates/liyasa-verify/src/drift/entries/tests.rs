use std::time::{Duration, SystemTime};

use liyasa_core::ids::{CheckId, FactId, Route};
use liyasa_core::verify::{ChangeKind, FactValue};

use crate::core::config::DriftSeverity;
use crate::drift::record::{Candidate, DriftKind, DriftRecord, DriftState, Resolution};

use super::{entries, summary};

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

fn record(kind: DriftKind, pages: &[&str], severity: DriftSeverity) -> DriftRecord {
    Candidate::new(kind, pages.iter().map(|p| Route::new(*p)).collect())
        .opened(severity, at(86_400))
}

fn fact() -> DriftKind {
    DriftKind::Fact {
        fact: FactId::new("plan.pro.price"),
        old: Some(FactValue::Num(20.0)),
        new: Some(FactValue::Num(25.0)),
        change: ChangeKind::Changed,
    }
}

#[test]
fn a_record_over_several_pages_is_an_entry_on_each_of_them() {
    let records = [record(
        fact(),
        &["/pricing", "/plans"],
        DriftSeverity::Medium,
    )];
    let out = entries(&records, at(86_400 * 3));
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].0, Route::new("/plans"));
    assert_eq!(out[1].0, Route::new("/pricing"));
    // One record, so one id, whatever page it is filed under.
    assert_eq!(out[0].1.id, out[1].1.id);
    assert_eq!(out[0].1.age, Duration::from_secs(86_400 * 2));
    assert_eq!(out[0].1.severity, DriftSeverity::Medium);
    assert!(
        out[0].1.summary.contains("plan.pro.price"),
        "{:?}",
        out[0].1
    );
}

#[test]
fn a_resolved_record_is_not_in_the_report() {
    let mut closed = record(fact(), &["/pricing"], DriftSeverity::Medium);
    closed.state = DriftState::Resolved;
    closed.resolved_at = Some(at(86_400 * 2));
    closed.resolution = Some(Resolution::Fixed);
    assert!(entries(&[closed], at(86_400 * 3)).is_empty());
}

#[test]
fn a_page_with_two_records_shows_the_worse_one_first() {
    let records = [
        record(fact(), &["/install"], DriftSeverity::Low),
        record(
            DriftKind::Check {
                check: CheckId::new("/install#aabbccddeeff001122334455#0"),
                excerpt: "exit 1".to_owned(),
            },
            &["/install"],
            DriftSeverity::Critical,
        ),
    ];
    let out = entries(&records, at(86_400 * 2));
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].1.severity, DriftSeverity::Critical);
    assert_eq!(out[1].1.severity, DriftSeverity::Low);
}

#[test]
fn a_record_seen_in_the_future_reports_no_age_rather_than_panicking() {
    let records = [record(fact(), &["/pricing"], DriftSeverity::Medium)];
    // A clock that went backwards between the write and the report.
    let out = entries(&records, at(0));
    assert_eq!(out[0].1.age, Duration::ZERO);
}

#[test]
fn every_kind_says_what_a_reader_has_to_do_about_it() {
    assert_eq!(
        summary(&fact()),
        "`plan.pro.price` is 20 here and 25 at its source"
    );
    assert_eq!(
        summary(&DriftKind::Operation {
            spec: "petstore".to_owned(),
            op: "listPets".to_owned(),
            diff: vec!["responses".to_owned(), "auth".to_owned()],
        }),
        "`listPets` in `petstore` moved: responses, auth"
    );
    assert_eq!(
        summary(&DriftKind::Link {
            url: "https://example.com/gone".to_owned(),
            reason: "404 not found".to_owned(),
            failing_since: at(0),
        }),
        "https://example.com/gone has been failing: 404 not found"
    );
    assert_eq!(
        summary(&DriftKind::Check {
            check: CheckId::new("/install#ab#0"),
            excerpt: "exit 1".to_owned(),
        }),
        "a verified example fails: exit 1"
    );

    let review = |reviewed, overdue| DriftKind::Review {
        page: Route::new("/pricing"),
        owners: vec!["docs@example.com".to_owned()],
        reviewed,
        cadence: Duration::from_secs(180 * 86_400),
        overdue_by: overdue,
    };
    assert_eq!(
        summary(&review(None, Duration::ZERO)),
        "never reviewed; the cadence is 180 days"
    );
    assert_eq!(
        summary(&review(Some(at(0)), Duration::from_secs(86_400))),
        "review is 1 day overdue; the cadence is 180 days"
    );
}
