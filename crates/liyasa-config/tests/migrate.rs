//! `liyasa migrate-config` (CLI-14, CFG-91). v0 is defined by RFC 0103.

use liyasa_config::json::SpanIndex;
use liyasa_config::{migrate, schema};
use liyasa_core::span::SourceId;
use serde_json::Value;

const V0: &str = include_str!("fixtures/v0.json");
const GOLDEN: &str = include_str!("fixtures/v0-migrated.json");

fn run(text: &str) -> migrate::Migrated {
    let value: Value = serde_json::from_str(text).expect("the fixture is valid JSON");
    migrate::migrate(&value, &SpanIndex::scan(SourceId(0), text))
}

#[test]
fn a_v0_config_becomes_the_golden_v1_config() {
    let migrated = run(V0);
    assert!(
        !migrated.diagnostics.has_errors(),
        "{:?}",
        migrated.diagnostics
    );
    assert_eq!(migrated.json.trim_end(), GOLDEN.trim_end());
    // The golden carries the published `$id` of the day; where the schemas are
    // hosted has moved once already, so the URL is asserted against the schema
    // rather than against a constant.
    assert_eq!(
        migrated.value.pointer("/$schema").and_then(Value::as_str),
        Some(liyasa_config::schema::config_schema_id())
    );
}

#[test]
fn the_result_validates_against_v1() {
    let migrated = run(V0);
    let spans = SpanIndex::scan(SourceId(0), &migrated.json);
    let report = schema::check(&migrated.value, &spans);
    assert!(
        !report.diagnostics.has_errors() && report.unknown.is_empty(),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn every_rewrite_is_reported() {
    let changes = run(V0).changes;
    let named = |from: &str| {
        changes
            .iter()
            .find(|c| c.from == from)
            .map(|c| c.to.as_str())
    };
    assert_eq!(named("colors"), Some("theme.colors"));
    assert_eq!(named("topbarCtaButton"), Some("navbar.primary"));
    assert_eq!(named("openApi"), Some("openapi"));
    assert_eq!(
        named("somethingRetired"),
        Some(""),
        "a key with no v1 home is recorded as dropped"
    );
}

#[test]
fn a_v1_config_is_returned_unchanged() {
    let text = r##"{ "$schema": "https://liyasa.dev/schema/v1/liyasa.json", "name": "Acme" }"##;
    let migrated = run(text);
    assert!(migrated.changes.is_empty());
    assert_eq!(
        migrated.value,
        serde_json::from_str::<Value>(text).expect("valid")
    );
}

#[test]
fn a_config_without_a_schema_key_is_treated_as_v0() {
    let migrated = run(r##"{ "name": "Acme", "colors": { "primary": "#4F46E5" } }"##);
    assert_eq!(
        migrated
            .value
            .pointer("/theme/colors/primary")
            .and_then(Value::as_str),
        Some("#4F46E5")
    );
}

#[test]
fn a_version_this_build_does_not_know_is_refused() {
    let migrated =
        run(r##"{ "$schema": "https://liyasa.dev/schema/v9/liyasa.json", "name": "A" }"##);
    assert!(migrated.diagnostics.has_errors());
    assert!(migrated.changes.is_empty());
}
