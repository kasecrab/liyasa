use std::time::{Duration, SystemTime};

use liyasa_core::ids::Route;

use crate::core::config::DriftSeverity;
use crate::core::duration::DurationSetting;
use crate::drift::owners::Docowners;
use crate::drift::record::{Candidate, DriftKind, DriftRecord};

use super::{Cadence, DEFAULT_CADENCE, PageReview, digest, overdue, parse_date};

const DAY: u64 = 86_400;

fn at(days: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(days * DAY)
}

fn page(route: &str, reviewed: Option<&str>) -> PageReview {
    PageReview {
        reviewed: reviewed.map(ToOwned::to_owned),
        ..PageReview::new(Route::new(route))
    }
}

fn docowners() -> Docowners {
    Docowners::parse("/**  docs@example.com\n/api/**  api@example.com  platform@example.com\n")
}

#[test]
fn a_page_inside_its_cadence_is_not_flagged_and_one_past_it_is() {
    let cadence = Cadence::default();
    assert_eq!(cadence.default_cadence(), DEFAULT_CADENCE);

    // Reviewed on day 1; 180 days later it is exactly due, not yet over.
    let fresh = overdue(
        &[page("/pricing", Some("1970-01-02"))],
        &docowners(),
        &cadence,
        at(100),
    );
    assert!(fresh.candidates.is_empty());

    let due = overdue(
        &[page("/pricing", Some("1970-01-02"))],
        &docowners(),
        &cadence,
        at(1 + 180),
    );
    assert_eq!(due.candidates.len(), 1, "at the cadence it is due");
    assert!(matches!(
        &due.candidates[0].kind,
        DriftKind::Review { overdue_by, .. } if *overdue_by == Duration::ZERO
    ));

    let late = overdue(
        &[page("/pricing", Some("1970-01-02"))],
        &docowners(),
        &cadence,
        at(1 + 180 + 30),
    );
    assert!(matches!(
        &late.candidates[0].kind,
        DriftKind::Review { overdue_by, .. } if *overdue_by == Duration::from_secs(30 * DAY)
    ));
}

#[test]
fn a_page_that_has_never_been_reviewed_is_flagged_but_not_escalated() {
    let found = overdue(
        &[page("/pricing", None)],
        &docowners(),
        &Cadence::default(),
        at(10_000),
    );
    assert_eq!(found.candidates.len(), 1);
    assert!(
        found.problems.is_empty(),
        "an absent date is not a misread one"
    );
    let kind = &found.candidates[0].kind;
    assert!(matches!(
        kind,
        DriftKind::Review { reviewed: None, overdue_by, .. } if *overdue_by == Duration::ZERO
    ));
    // How late it is is unknown, so it grades Medium rather than High.
    assert_eq!(kind.base_severity(), DriftSeverity::Medium);
}

#[test]
fn a_date_nothing_can_read_is_reported_and_the_page_is_still_flagged() {
    let found = overdue(
        &[page("/pricing", Some("last tuesday"))],
        &docowners(),
        &Cadence::default(),
        at(10_000),
    );
    assert_eq!(found.problems.len(), 1);
    let problem = &found.problems[0];
    assert_eq!(problem.code, liyasa_core::diagnostics::code::W0639);
    assert_eq!(
        problem.severity,
        liyasa_core::diagnostics::Severity::Warning
    );
    assert!(problem.message.contains("/pricing"), "{}", problem.message);
    assert!(
        problem.message.contains("last tuesday"),
        "{}",
        problem.message
    );
    assert!(problem.help.is_some(), "and it says what to write instead");
    assert_eq!(
        found.candidates.len(),
        1,
        "a date Liyasa cannot read is not evidence the page is fresh"
    );
}

#[test]
fn the_longest_matching_directory_override_wins() {
    let cadence = Cadence::from_setting(Some(DurationSetting::hours(24 * 180)))
        .with_override("/api", Duration::from_secs(90 * DAY))
        .with_override("/api/webhooks", Duration::from_secs(30 * DAY));

    assert_eq!(cadence.for_route(&Route::new("/pricing")), DEFAULT_CADENCE);
    assert_eq!(
        cadence.for_route(&Route::new("/api/pets")),
        Duration::from_secs(90 * DAY)
    );
    assert_eq!(
        cadence.for_route(&Route::new("/api/webhooks/events")),
        Duration::from_secs(30 * DAY),
        "the deeper directory tightens its parent"
    );
    // The directory itself is inside itself.
    assert_eq!(
        cadence.for_route(&Route::new("/api")),
        Duration::from_secs(90 * DAY)
    );
    // And a sibling whose name merely starts the same is not inside it.
    assert_eq!(
        cadence.for_route(&Route::new("/apiary")),
        DEFAULT_CADENCE,
        "a prefix is a directory, not a string prefix"
    );
}

#[test]
fn an_override_written_with_or_without_slashes_is_the_same_directory() {
    let cadence = Cadence::new(DEFAULT_CADENCE).with_override("api/", Duration::from_secs(DAY));
    assert_eq!(
        cadence.for_route(&Route::new("/api/pets")),
        Duration::from_secs(DAY)
    );
}

#[test]
fn a_root_override_replaces_the_default_for_everything() {
    let cadence = Cadence::new(DEFAULT_CADENCE).with_override("/", Duration::from_secs(7 * DAY));
    assert_eq!(
        cadence.for_route(&Route::new("/anything/at/all")),
        Duration::from_secs(7 * DAY)
    );
}

#[test]
fn the_owner_is_docowners_first_and_the_last_author_only_when_nothing_matched() {
    let owners = Docowners::parse("/api/**  api@example.com\n/experiments/\n");
    let pages = [
        PageReview {
            last_author: Some("alice@example.com".to_owned()),
            ..page("/api/pets", None)
        },
        PageReview {
            last_author: Some("bob@example.com".to_owned()),
            ..page("/pricing", None)
        },
        PageReview {
            last_author: Some("carol@example.com".to_owned()),
            ..page("/experiments/new", None)
        },
        page("/orphan", None),
    ];
    let found = overdue(&pages, &owners, &Cadence::default(), at(10_000));
    let named: Vec<Vec<String>> = found
        .candidates
        .iter()
        .map(|candidate| match &candidate.kind {
            DriftKind::Review { owners, .. } => owners.clone(),
            other => panic!("{other:?}"),
        })
        .collect();

    assert_eq!(named[0], vec!["api@example.com".to_owned()]);
    assert_eq!(
        named[1],
        vec!["bob@example.com".to_owned()],
        "no rule matched, so the last author is the owner"
    );
    assert_eq!(
        named[2],
        Vec::<String>::new(),
        "a rule that names nobody is a decision and does not fall through"
    );
    assert_eq!(
        named[3],
        Vec::<String>::new(),
        "and nothing knows the author"
    );
}

fn review_record(
    route: &str,
    owners: &[&str],
    weight: Option<f64>,
    overdue_days: u64,
) -> DriftRecord {
    Candidate::new(
        DriftKind::Review {
            page: Route::new(route),
            owners: owners.iter().copied().map(ToOwned::to_owned).collect(),
            reviewed: Some(at(0)),
            cadence: DEFAULT_CADENCE,
            overdue_by: Duration::from_secs(overdue_days * DAY),
        },
        vec![Route::new(route)],
    )
    .with_weight(weight)
    .opened(DriftSeverity::Medium, at(1))
}

#[test]
fn a_digest_per_owner_lists_their_pages_by_traffic_first() {
    let records = [
        review_record("/quiet", &["docs@example.com"], Some(3.0), 10),
        review_record("/busy", &["docs@example.com"], Some(900.0), 1),
        review_record("/unmeasured", &["docs@example.com"], None, 400),
        review_record(
            "/api/pets",
            &["api@example.com", "docs@example.com"],
            Some(50.0),
            5,
        ),
    ];
    let digest = digest(&records);

    assert_eq!(digest.owners.len(), 2);
    let docs = &digest.owners[1];
    assert_eq!(docs.owner, "docs@example.com");
    assert_eq!(
        docs.pages
            .iter()
            .map(|(route, _)| route.as_str())
            .collect::<Vec<_>>(),
        ["/busy", "/api/pets", "/quiet", "/unmeasured"],
        "traffic first, and a page with no figure sorts last rather than above everything"
    );

    // A page with two owners appears in both digests.
    let api = &digest.owners[0];
    assert_eq!(api.owner, "api@example.com");
    assert_eq!(api.pages.len(), 1);
    assert!(digest.unowned.is_empty());
}

#[test]
fn an_overdue_page_nobody_owns_is_in_the_digest_rather_than_dropped() {
    let records = [
        review_record("/orphan", &[], None, 5),
        review_record("/pricing", &["docs@example.com"], None, 5),
    ];
    let digest = digest(&records);
    assert_eq!(digest.unowned, vec![Route::new("/orphan")]);
    assert_eq!(digest.owners.len(), 1);
}

#[test]
fn a_digest_skips_records_that_are_not_reviews_and_records_that_are_closed() {
    use crate::drift::record::{DriftState, Resolution};
    use liyasa_core::ids::FactId;
    use liyasa_core::verify::{ChangeKind, FactValue};

    let mut closed = review_record("/done", &["docs@example.com"], None, 5);
    closed.state = DriftState::Resolved;
    closed.resolution = Some(Resolution::Approved {
        by: "docs@example.com".to_owned(),
    });

    let fact = Candidate::new(
        DriftKind::Fact {
            fact: FactId::new("plan.pro.price"),
            old: Some(FactValue::Num(20.0)),
            new: Some(FactValue::Num(25.0)),
            change: ChangeKind::Changed,
        },
        vec![Route::new("/pricing")],
    )
    .opened(DriftSeverity::Medium, at(1));

    let digest = digest(&[
        closed,
        fact,
        review_record("/live", &["docs@example.com"], None, 1),
    ]);
    assert_eq!(digest.owners.len(), 1);
    assert_eq!(
        digest.owners[0].pages,
        vec![(Route::new("/live"), None)],
        "a closed review is not a reminder and a fact is not a review"
    );
}

#[test]
fn a_date_is_read_as_a_whole_day_and_a_nonsense_one_is_not_read_at_all() {
    assert_eq!(parse_date("1970-01-01"), Some(SystemTime::UNIX_EPOCH));
    assert_eq!(parse_date("1970-01-02"), Some(at(1)));
    // 2026-09-28 is 20724 days after the epoch.
    assert_eq!(parse_date("2026-09-28"), Some(at(20_724)));
    // A leap day exists in 2024 and not in 2023.
    assert!(parse_date("2024-02-29").is_some());
    assert_eq!(parse_date("2023-02-29"), None);
    // A full timestamp is read for its date part, which is all a cadence uses.
    assert_eq!(parse_date("2026-09-28T13:45:00Z"), Some(at(20_724)));

    for bad in [
        "last tuesday",
        "2026-13-01",
        "2026-00-01",
        "2026-09-32",
        "2026-09-00",
        "2026-9-8",
        "2026/09/28",
        "20260928",
        "",
        "2026-09",
    ] {
        assert_eq!(parse_date(bad), None, "{bad:?} is not a date");
    }
}

#[test]
fn a_date_before_the_epoch_is_read_rather_than_overflowing() {
    let long_ago = parse_date("1969-12-31").expect("a date");
    assert_eq!(
        SystemTime::UNIX_EPOCH
            .duration_since(long_ago)
            .expect("before the epoch"),
        Duration::from_secs(DAY)
    );
}

/// The end of the seam WP-01 opened: config JSON in, cadences out, with the
/// two schema forms unfolded by `liyasa_config::review::review_cadence` so
/// nothing here branches on which one an operator wrote.
fn from_json(config: serde_json::Value) -> Cadence {
    Cadence::from_config(&liyasa_config::review::review_cadence(&config))
}

#[test]
fn the_plain_string_form_sets_one_cadence_for_the_whole_site() {
    let cadence = from_json(serde_json::json!({
        "content": { "reviewCadence": "30d" }
    }));
    assert_eq!(cadence.default_cadence(), Duration::from_secs(30 * DAY));
    assert_eq!(
        cadence.for_route(&Route::new("/reference/api/pets")),
        Duration::from_secs(30 * DAY),
        "a plain string has no overrides to find"
    );
}

#[test]
fn the_object_form_reaches_with_override_and_the_longest_prefix_still_wins() {
    let cadence = from_json(serde_json::json!({
        "content": {
            "reviewCadence": {
                "default": "180d",
                "overrides": { "reference": "30d", "reference/api": "7d" }
            }
        }
    }));

    assert_eq!(cadence.default_cadence(), DEFAULT_CADENCE);
    assert_eq!(cadence.for_route(&Route::new("/pricing")), DEFAULT_CADENCE);
    assert_eq!(
        cadence.for_route(&Route::new("/reference/glossary")),
        Duration::from_secs(30 * DAY)
    );
    assert_eq!(
        cadence.for_route(&Route::new("/reference/api/pets")),
        Duration::from_secs(7 * DAY),
        "the deeper override tightens what the shallower one set"
    );
}

#[test]
fn an_override_key_is_normalized_the_way_w0140_normalizes_it() {
    // The keys here are written three ways and name two directories. If this
    // module normalized differently from `liyasa_config::review::normalize`, a
    // key W0140 accepted could be one nothing here matches.
    for key in ["reference", "/reference", "reference/", "/reference/"] {
        let cadence = from_json(serde_json::json!({
            "content": { "reviewCadence": { "default": "180d", "overrides": { key: "30d" } } }
        }));
        assert_eq!(
            cadence.for_route(&Route::new("/reference/glossary")),
            Duration::from_secs(30 * DAY),
            "{key:?} is the same directory as the others"
        );
        assert_eq!(
            cadence.for_route(&Route::new("/referendum")),
            DEFAULT_CADENCE,
            "{key:?} is a directory, not a string prefix"
        );
    }
    // And the function itself is the one config exports, not a copy.
    assert_eq!(liyasa_config::review::normalize("reference/"), "/reference");
}

#[test]
fn a_duration_the_schema_would_have_rejected_falls_through_rather_than_becoming_zero() {
    // `^\d+(ms|s|m|h|d)$` is on every duration in the key, so this shape is
    // already `E0102` from validation. A consumer that ran anyway gets the
    // site-wide default, not a cadence of nothing.
    let cadence = from_json(serde_json::json!({
        "content": {
            "reviewCadence": {
                "default": "180d",
                "overrides": { "reference": "a fortnight", "reference/api": "7d" }
            }
        }
    }));
    assert_eq!(
        cadence.for_route(&Route::new("/reference/glossary")),
        DEFAULT_CADENCE,
        "not Duration::ZERO, which would flag every page in the directory"
    );
    assert_ne!(
        cadence.for_route(&Route::new("/reference/glossary")),
        Duration::ZERO
    );
    assert_eq!(
        cadence.for_route(&Route::new("/reference/api/pets")),
        Duration::from_secs(7 * DAY),
        "and the readable sibling is unaffected"
    );

    // The same for the default itself.
    let bad_default = from_json(serde_json::json!({
        "content": { "reviewCadence": { "default": "soon" } }
    }));
    assert_eq!(bad_default.default_cadence(), DEFAULT_CADENCE);
}

#[test]
fn an_absent_key_is_ver_77s_180_days_and_so_is_a_shape_the_schema_rejects() {
    assert_eq!(
        from_json(serde_json::json!({})).default_cadence(),
        DEFAULT_CADENCE
    );
    assert_eq!(
        from_json(serde_json::json!({ "content": {} })).default_cadence(),
        DEFAULT_CADENCE
    );
    // `review_cadence` reads a wrong-typed node as absent rather than panicking.
    assert_eq!(
        from_json(serde_json::json!({ "content": { "reviewCadence": 180 } })).default_cadence(),
        DEFAULT_CADENCE
    );
}

#[test]
fn a_page_in_an_overridden_directory_is_flagged_on_the_tighter_cadence() {
    // The whole point of the key, end to end: the same page and the same date,
    // overdue under the override and not under the site default.
    let cadence = from_json(serde_json::json!({
        "content": {
            "reviewCadence": { "default": "180d", "overrides": { "reference": "30d" } }
        }
    }));
    let pages = [
        page("/reference/api", Some("1970-01-02")),
        page("/pricing", Some("1970-01-02")),
    ];
    let found = overdue(&pages, &docowners(), &cadence, at(1 + 40));

    assert_eq!(found.candidates.len(), 1, "{:#?}", found.candidates);
    assert_eq!(
        found.candidates[0].pages,
        vec![Route::new("/reference/api")]
    );
    assert!(matches!(
        &found.candidates[0].kind,
        DriftKind::Review { cadence, overdue_by, .. }
            if *cadence == Duration::from_secs(30 * DAY)
                && *overdue_by == Duration::from_secs(10 * DAY)
    ));
}
