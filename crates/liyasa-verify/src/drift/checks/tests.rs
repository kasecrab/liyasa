use std::time::Duration;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::ids::{BlockId, CheckId, Fingerprint, Route};
use liyasa_core::verify::{CheckOutcome, CheckResult};

use crate::drift::record::{DriftKey, DriftKind};
use crate::runners::check_id;

use super::{candidates, coverage, route_of};

fn result(id: &str, outcome: CheckOutcome) -> CheckResult {
    CheckResult {
        id: CheckId::new(id),
        outcome,
        duration: Duration::from_millis(3),
        digest: Fingerprint::of(id),
    }
}

fn failing(id: &str) -> CheckResult {
    result(
        id,
        CheckOutcome::Fail {
            excerpt: "exit 1".to_owned(),
        },
    )
}

#[test]
fn only_a_failing_check_is_drift() {
    let results = [
        failing("/install#aabbccddeeff001122334455#0"),
        result("/start#aabbccddeeff001122334455#0", CheckOutcome::Pass),
        result(
            "/tour#aabbccddeeff001122334455#0",
            CheckOutcome::Skip {
                reason: "no runner".to_owned(),
            },
        ),
        // The one that matters: an error is the checker failing, not the page
        // being wrong, and a test asserting "not clean" would have passed on it.
        result(
            "/deploy#aabbccddeeff001122334455#0",
            CheckOutcome::Error(Diagnostic::new(code::E0610, "the image is not pinned")),
        ),
    ];

    let found = candidates(&results);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(
        found[0].key(),
        DriftKey::Check(CheckId::new("/install#aabbccddeeff001122334455#0"))
    );
    assert_eq!(found[0].pages, vec![Route::new("/install")]);
    assert!(matches!(
        &found[0].kind,
        DriftKind::Check { excerpt, .. } if excerpt == "exit 1"
    ));
}

#[test]
fn coverage_is_every_check_the_run_examined_not_only_the_failures() {
    let results = [
        failing("/install#aabbccddeeff001122334455#0"),
        result("/start#aabbccddeeff001122334455#0", CheckOutcome::Pass),
    ];
    let covered = coverage(&results);
    assert_eq!(covered.len(), 2);
    assert!(covered.contains(&DriftKey::Check(CheckId::new(
        "/start#aabbccddeeff001122334455#0"
    ))));
}

#[test]
fn the_route_comes_back_out_of_every_id_check_id_puts_in() {
    // The coupling this module has to VER-01's id format, pinned so a change to
    // the format fails here rather than producing records attributed to nothing.
    for route in ["/install", "/guides/deploy/aws", "/"] {
        let id = check_id(&Route::new(route), &BlockId([7; 12]), 3);
        assert_eq!(route_of(&id), Some(Route::new(route)), "{id}");
    }
}

#[test]
fn an_id_with_no_route_in_it_is_no_page_rather_than_an_empty_one() {
    assert_eq!(route_of(&CheckId::new("#abc#0")), None);
    assert_eq!(route_of(&CheckId::new("no-separator")), None);
    // And a check with no page attributed to it is not a candidate at all.
    assert!(candidates(&[failing("#abc#0")]).is_empty());
}
