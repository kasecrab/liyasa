//! CFG-91: an unknown key is a warning and never silent, a config from a newer
//! schema is refused, and `migrate-config` upgrades a v0 file to v1.

use liyasa_config::json::SpanIndex;
use liyasa_config::{migrate, schema};
use liyasa_core::diagnostics::Severity;
use liyasa_core::span::SourceId;
use liyasa_tests::config::{codes, one, project};
use serde_json::Value;

/// The fixtures the config crate migrates; RFC 0103 defines what v0 is.
const V0: &str = include_str!("../../crates/liyasa-config/tests/fixtures/v0.json");
const GOLDEN: &str = include_str!("../../crates/liyasa-config/tests/fixtures/v0-migrated.json");

#[test]
fn an_unknown_key_is_a_warning_that_says_what_it_meant() {
    let config = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.dev" },
                       "nvaigation": [] }"##;
    let checked = project(&[("liyasa.json", config), ("index.md", "# Home")]);
    let unknown = one(&checked, "E0103");
    assert_eq!(unknown.severity, Severity::Warning, "CFG-91; RFC 0102");
    assert_eq!(unknown.help.as_deref(), Some("did you mean `navigation`?"));
    assert!(
        checked.config.is_some(),
        "the key is dropped so the rest of the config still loads"
    );
}

#[test]
fn a_newer_schema_version_is_e0121() {
    let config = r##"{ "$schema": "https://liyasa.dev/schema/v2/liyasa.json", "name": "Acme",
                       "seo": { "canonicalOrigin": "https://acme.dev" } }"##;
    let checked = project(&[("liyasa.json", config), ("index.md", "# Home")]);
    assert!(codes(&checked).contains(&"E0121"), "{:?}", codes(&checked));
}

#[test]
fn migrate_config_upgrades_a_v0_file_to_the_golden_v1() {
    let value: Value = serde_json::from_str(V0).expect("the fixture is valid JSON");
    let migrated = migrate::migrate(&value, &SpanIndex::scan(SourceId(0), V0));
    assert!(
        !migrated.diagnostics.has_errors(),
        "{:?}",
        migrated.diagnostics
    );
    assert_eq!(migrated.json.trim_end(), GOLDEN.trim_end());

    let spans = SpanIndex::scan(SourceId(0), &migrated.json);
    let report = schema::check(&migrated.value, &spans);
    assert!(
        !report.diagnostics.has_errors() && report.unknown.is_empty(),
        "the result validates against v1: {:?}",
        report.diagnostics
    );
}
