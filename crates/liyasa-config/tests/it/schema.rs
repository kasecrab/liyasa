//! The schema is the source of truth (CFG-94), and what it rejects (CFG-01,
//! CFG-90, CFG-91).

use liyasa_config::json::SpanIndex;
use liyasa_config::schema;
use liyasa_core::diagnostics::{Diagnostics, Severity};
use liyasa_core::span::SourceId;

const SOURCE: SourceId = SourceId(0);
const EXAMPLE: &str = include_str!("../fixtures/example.json");
/// PRD §34.2 byte for byte. One of its keys still does not validate; RFC 0101.
const EXAMPLE_PRD: &str = include_str!("../fixtures/example-prd.json");

fn check(text: &str) -> Diagnostics {
    let value = serde_json::from_str(text).expect("the fixture is valid JSON");
    schema::check(&value, &SpanIndex::scan(SOURCE, text)).diagnostics
}

fn codes(diagnostics: &Diagnostics) -> Vec<&str> {
    diagnostics.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn the_emitted_schema_is_the_file_byte_for_byte() {
    let on_disk = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../schemas/liyasa.schema.json"
    ))
    .expect("the schema file is readable");
    assert_eq!(schema::CONFIG_SCHEMA, on_disk);
}

#[test]
fn the_schema_is_a_2020_12_document_with_the_published_id() {
    let parsed: serde_json::Value =
        serde_json::from_str(schema::CONFIG_SCHEMA).expect("the schema is valid JSON");
    assert_eq!(
        parsed["$schema"], "https://json-schema.org/draft/2020-12/schema",
        "CFG-94 pins JSON Schema 2020-12"
    );
    assert_eq!(parsed["$id"], schema::config_schema_id());
}

#[test]
fn the_example_validates() {
    assert_eq!(codes(&check(EXAMPLE)), Vec::<&str>::new());
}

#[test]
fn the_prd_example_still_drifts_in_one_place() {
    let diagnostics = check(EXAMPLE_PRD);
    let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        codes(&diagnostics),
        vec!["E0102"],
        "RFC 0101: §34.2 writes `weight` as an array and the schema says string \
         or integer. The `versions[].tag` half closed when the schema gained the \
         row (RFC 0105); this one is prose the PRD has to fix, not a schema gap. \
         Got {messages:?}"
    );
    assert!(
        diagnostics.iter().any(|d| d.message.contains("600")),
        "the array weight is the error: {messages:?}"
    );
}

#[test]
fn a_config_without_a_name_is_e0102() {
    let diagnostics = check(r#"{ "description": "no name here" }"#);
    assert_eq!(codes(&diagnostics), vec!["E0102"]);
    assert!(diagnostics.iter().any(|d| d.message.contains("name")));
}

#[test]
fn an_unknown_key_is_a_warning_and_is_dropped() {
    let text = r#"{ "name": "Acme", "nvaigation": [] }"#;
    let value = serde_json::from_str(text).expect("valid JSON");
    let report = schema::check(&value, &SpanIndex::scan(SOURCE, text));

    assert_eq!(codes(&report.diagnostics), vec!["E0103"], "RFC 0102");
    let diagnostic = report.diagnostics.iter().next().expect("one diagnostic");
    assert_eq!(diagnostic.severity, Severity::Warning, "RFC 0102");
    let span = diagnostic.span.expect("the key is located");
    assert_eq!(
        &text[span.start as usize..span.end as usize],
        "\"nvaigation\""
    );
    assert_eq!(
        diagnostic.help.as_deref(),
        Some("did you mean `navigation`?")
    );
    assert_eq!(report.unknown, vec!["/nvaigation".to_owned()]);

    let pruned = schema::without(&value, &report.unknown);
    assert_eq!(pruned, serde_json::json!({ "name": "Acme" }));
}

#[test]
fn a_wrong_type_is_e0102_at_the_value() {
    let text = r#"{ "name": "Acme", "seo": { "trailingSlash": "yes" } }"#;
    let value = serde_json::from_str(text).expect("valid JSON");
    let diagnostics = schema::check(&value, &SpanIndex::scan(SOURCE, text)).diagnostics;
    assert_eq!(codes(&diagnostics), vec!["E0102"]);
    let span = diagnostics
        .iter()
        .next()
        .and_then(|d| d.span)
        .expect("located");
    assert_eq!(&text[span.start as usize..span.end as usize], "\"yes\"");
}

#[test]
fn an_unknown_theme_preset_is_e0102() {
    let diagnostics = check(r#"{ "name": "Acme", "theme": { "preset": "neon" } }"#);
    assert_eq!(codes(&diagnostics), vec!["E0102"]);
}

#[test]
fn a_newer_schema_version_is_e0121() {
    let text = r#"{ "$schema": "https://liyasa.dev/schema/v2/liyasa.json", "name": "Acme" }"#;
    let value = serde_json::from_str(text).expect("valid JSON");
    let spans = SpanIndex::scan(SOURCE, text);
    let mut diagnostics = Diagnostics::new();
    assert_eq!(
        schema::declared_version(&value, &spans, &mut diagnostics),
        Some(2)
    );
    assert_eq!(codes(&diagnostics), vec!["E0121"]);
}

#[test]
fn the_current_schema_version_is_silent() {
    let text = r#"{ "$schema": "https://liyasa.dev/schema/v1/liyasa.json", "name": "Acme" }"#;
    let value = serde_json::from_str(text).expect("valid JSON");
    let spans = SpanIndex::scan(SOURCE, text);
    let mut diagnostics = Diagnostics::new();
    assert_eq!(
        schema::declared_version(&value, &spans, &mut diagnostics),
        Some(1)
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn an_absent_schema_key_is_not_a_diagnostic() {
    let value = serde_json::json!({ "name": "Acme" });
    let mut diagnostics = Diagnostics::new();
    let spans = SpanIndex::scan(SOURCE, "{}");
    assert_eq!(
        schema::declared_version(&value, &spans, &mut diagnostics),
        None
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn a_plaintext_smtp_password_is_refused_by_the_schema() {
    let plaintext = r#"{ "name": "Acme", "mail": { "from": "docs@acme.dev",
      "smtp": { "host": "smtp.acme.dev", "password": "hunter2" } } }"#;
    assert_eq!(
        codes(&check(plaintext)),
        vec!["E0102"],
        "a password written into `liyasa.json` is a validation error, not a convention"
    );

    for reference in ["secret:smtp-password", "env:SMTP_PASSWORD"] {
        let config = format!(
            r#"{{ "name": "Acme", "mail": {{ "from": "docs@acme.dev",
              "smtp": {{ "host": "smtp.acme.dev", "password": "{reference}" }} }} }}"#
        );
        assert_eq!(codes(&check(&config)), Vec::<&str>::new(), "{reference}");
    }
}

#[test]
fn a_mail_block_that_could_not_send_is_refused() {
    let no_sender = r#"{ "name": "Acme", "mail": { "smtp": { "host": "smtp.acme.dev" } } }"#;
    assert_eq!(
        codes(&check(no_sender)),
        vec!["E0102"],
        "`from` is required"
    );

    let no_transport = r#"{ "name": "Acme", "mail": { "from": "docs@acme.dev" } }"#;
    assert_eq!(
        codes(&check(no_transport)),
        vec!["E0102"],
        "`smtp` is required"
    );

    let no_host = r#"{ "name": "Acme", "mail": { "from": "docs@acme.dev", "smtp": {} } }"#;
    assert_eq!(
        codes(&check(no_host)),
        vec!["E0102"],
        "`smtp.host` is required"
    );
}

#[test]
fn an_operator_needs_both_a_subject_and_a_role() {
    let bare = r#"{ "name": "Acme", "auth": { "mode": "oidc", "operators": ["ana"] } }"#;
    assert_eq!(
        codes(&check(bare)),
        vec!["E0102"],
        "a bare string would read as an address; an operator says which subject and which role"
    );

    let no_role = r#"{ "name": "Acme", "auth": { "mode": "oidc",
      "operators": [{ "subject": "8f14e45fce" }] }}"#;
    assert_eq!(codes(&check(no_role)), vec!["E0102"]);

    let reader = r#"{ "name": "Acme", "auth": { "mode": "oidc",
      "operators": [{ "subject": "8f14e45fce", "role": "reader" }] }}"#;
    assert_eq!(
        codes(&check(reader)),
        vec!["E0102"],
        "`reader` carries no permission at all, so granting it is a no-op the schema refuses"
    );
}
