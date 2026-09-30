//! `content.reviewCadence` in both forms, and the override that matches
//! nothing (VER-77, CFG-80).

use liyasa_config::review::{ReviewCadence, normalize, review_cadence};
use serde_json::json;

#[test]
fn one_duration_is_the_whole_site() {
    let cadence = review_cadence(&json!({ "content": { "reviewCadence": "90d" } }));
    assert_eq!(cadence.default.as_deref(), Some("90d"));
    assert!(cadence.overrides.is_empty());
}

#[test]
fn the_object_form_carries_a_default_and_directories() {
    let cadence = review_cadence(&json!({
        "content": { "reviewCadence": {
            "default": "180d",
            "overrides": { "reference": "30d", "reference/api": "7d" }
        } }
    }));
    assert_eq!(cadence.default.as_deref(), Some("180d"));
    assert_eq!(
        cadence.overrides.get("reference").map(String::as_str),
        Some("30d")
    );
    assert_eq!(
        cadence.overrides.get("reference/api").map(String::as_str),
        Some("7d"),
        "a nested directory is its own entry; the consumer decides precedence"
    );
}

#[test]
fn overrides_without_a_default_leave_the_default_to_the_consumer() {
    let cadence = review_cadence(&json!({
        "content": { "reviewCadence": { "overrides": { "reference": "30d" } } }
    }));
    assert_eq!(
        cadence.default, None,
        "VER-77's 180 days is not this crate's to invent"
    );
    assert_eq!(cadence.overrides.len(), 1);
}

#[test]
fn an_absent_or_unusable_key_reads_as_nothing_rather_than_half_of_something() {
    assert_eq!(
        review_cadence(&json!({ "name": "Acme" })),
        ReviewCadence::default()
    );
    assert!(review_cadence(&json!({ "content": {} })).is_empty());
    // A shape the schema rejects: the validator reports it, and this reads as
    // absent rather than panicking or inventing a value.
    assert!(review_cadence(&json!({ "content": { "reviewCadence": 90 } })).is_empty());
}

#[test]
fn a_directory_is_one_directory_however_it_is_written() {
    for written in ["reference", "/reference", "reference/", "/reference/"] {
        assert_eq!(normalize(written), "/reference", "{written}");
    }
    assert_eq!(normalize(""), "/");
    assert_eq!(normalize("/"), "/");
    assert_eq!(normalize("reference/api"), "/reference/api");
}
