use std::time::{Duration, UNIX_EPOCH};

use liyasa_core::verify::{ChangeKind, FactValue};

use super::*;

fn values(pairs: &[(&str, f64)]) -> BTreeMap<FactId, FactValue> {
    pairs
        .iter()
        .map(|(fact, value)| (FactId::new(*fact), FactValue::Num(*value)))
        .collect()
}

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

fn snapshot(source: &str, pairs: &[(&str, f64)], seconds: u64) -> Snapshot {
    build(source, at(seconds), values(pairs), &Scrubber::new())
}

#[test]
fn the_digest_is_over_the_values_and_not_over_the_clock() {
    let first = snapshot("pricing", &[("plan.pro.price", 20.0)], 1_000);
    let later = snapshot("pricing", &[("plan.pro.price", 20.0)], 9_999);
    assert_eq!(first.digest, later.digest);
    assert_ne!(first.taken_at, later.taken_at);

    let moved = snapshot("pricing", &[("plan.pro.price", 25.0)], 1_000);
    assert_ne!(first.digest, moved.digest);
}

#[test]
fn two_sources_with_the_same_values_have_different_digests() {
    let a = snapshot("a", &[("plan.pro.price", 20.0)], 1);
    let b = snapshot("b", &[("plan.pro.price", 20.0)], 1);
    assert_ne!(a.digest, b.digest);
}

#[test]
fn a_fact_id_and_a_value_cannot_be_confused_for_each_other() {
    // Length-prefixed parts: `ab` = `1` and `a` = `b1` must not collide.
    let one = build(
        "s",
        at(0),
        BTreeMap::from([(FactId::new("ab"), FactValue::Str("1".to_owned()))]),
        &Scrubber::new(),
    );
    let other = build(
        "s",
        at(0),
        BTreeMap::from([(FactId::new("a"), FactValue::Str("b1".to_owned()))]),
        &Scrubber::new(),
    );
    assert_ne!(one.digest, other.digest);
}

#[test]
fn a_secret_that_comes_back_in_a_value_never_reaches_the_snapshot() {
    // At least `MIN_SECRET_LEN`: the scrubber refuses to redact a value short
    // enough to blank ordinary prose, so a shorter fixture would prove nothing.
    let scrubber = Scrubber::with_secrets(["s3cret-value-9f8e7d"]);
    let taken = build(
        "pricing",
        at(0),
        BTreeMap::from([(
            FactId::new("echo"),
            FactValue::Object(BTreeMap::from([(
                "token".to_owned(),
                FactValue::List(vec![FactValue::Str(
                    "Bearer s3cret-value-9f8e7d".to_owned(),
                )]),
            )])),
        )]),
        &scrubber,
    );
    let rendered = serde_json::to_string(&taken.values).expect("a snapshot serializes");
    assert!(!rendered.contains("s3cret-value-9f8e7d"), "{rendered}");
}

#[test]
fn a_diff_names_what_was_added_removed_and_changed() {
    let old = snapshot("s", &[("kept", 1.0), ("gone", 2.0), ("moved", 3.0)], 0);
    let new = snapshot("s", &[("kept", 1.0), ("moved", 4.0), ("fresh", 5.0)], 1);
    let changes = ValueDiffer.diff(&old, &new);

    let by_fact: BTreeMap<&str, &FactChange> = changes
        .iter()
        .map(|change| (change.fact.as_str(), change))
        .collect();
    assert_eq!(
        by_fact.keys().copied().collect::<Vec<_>>(),
        ["fresh", "gone", "moved"]
    );
    assert_eq!(by_fact["fresh"].kind, ChangeKind::Added);
    assert_eq!(by_fact["fresh"].old, None);
    assert_eq!(by_fact["gone"].kind, ChangeKind::Removed);
    assert_eq!(by_fact["gone"].new, None);
    assert_eq!(by_fact["moved"].kind, ChangeKind::Changed);
    assert_eq!(by_fact["moved"].old, Some(FactValue::Num(3.0)));
    assert_eq!(by_fact["moved"].new, Some(FactValue::Num(4.0)));
    // `kept` did not change, so it is not a change.
    assert!(!by_fact.contains_key("kept"));
}

#[test]
fn refreshing_twice_with_a_changed_value_leaves_two_attributable_snapshots() {
    let log = SnapshotLog::new();
    let first = log
        .record(
            snapshot("pricing", &[("plan.pro.price", 20.0)], 0),
            "main@1",
            true,
        )
        .expect("a first refresh");
    assert!(first.is_empty(), "nothing to diff against yet");

    let second = log
        .record(
            snapshot("pricing", &[("plan.pro.price", 25.0)], 60),
            "main@2",
            true,
        )
        .expect("a second refresh");
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].kind, ChangeKind::Changed);

    let history = log.history("pricing").expect("a history");
    assert_eq!(history.len(), 2);
    assert_ne!(history[0].snapshot.digest, history[1].snapshot.digest);
    assert_eq!(history[0].by, "main@1");
    assert_eq!(history[1].by, "main@2");
    assert_eq!(log.sources().expect("the sources"), ["pricing"]);
}

#[test]
fn an_untrusted_refresh_does_not_become_the_production_snapshot() {
    let log = SnapshotLog::new();
    log.record(
        snapshot("pricing", &[("plan.pro.price", 20.0)], 0),
        "main",
        true,
    )
    .expect("a trusted refresh");
    log.record(
        snapshot("pricing", &[("plan.pro.price", 999.0)], 60),
        "fork/pr-7",
        false,
    )
    .expect("an untrusted refresh");

    let latest = log
        .latest("pricing")
        .expect("a latest")
        .expect("one exists");
    assert_eq!(latest.by, "fork/pr-7");

    let production = log
        .latest_production("pricing")
        .expect("a production latest")
        .expect("one exists");
    assert_eq!(production.by, "main");
    assert_eq!(
        production.snapshot.values[&FactId::new("plan.pro.price")],
        FactValue::Num(20.0)
    );
}

#[test]
fn a_source_that_was_never_refreshed_has_no_snapshot() {
    let log = SnapshotLog::new();
    assert_eq!(log.latest("nope").expect("a query"), None);
    assert_eq!(log.latest_production("nope").expect("a query"), None);
    assert!(log.history("nope").expect("a query").is_empty());
}
