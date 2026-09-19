//! CFG-96: the config key that names an instance's bootstrap operators, and
//! the one thing about it that is not in this package's hands.
//!
//! An empty membership table elevates nobody, which is the safe direction and
//! also means a fresh instance has nobody who can add the first member
//! (defect 65, RFC 2802). `auth.operators` is the escape hatch. The schema and
//! the semantic rules are WP-01's; reading the key is WP-15's, and their own
//! ratchet — `auth::config::tests::every_key_the_schema_declares_is_a_key_this_deserializer_accepts`
//! — is red until they do, which is a better pin than a second test here
//! asserting the same gap from the other side.

use liyasa_config::json::SpanIndex;
use liyasa_config::schema;
use liyasa_core::span::SourceId;
use serde_json::{Value, json};

const OPERATORS: &str = r#"{
  "name": "Acme docs",
  "seo": { "canonicalOrigin": "https://docs.acme.com" },
  "auth": { "mode": "oidc", "operators": [{ "subject": "8f14e45fce", "role": "owner" }] }
}"#;

fn codes(text: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(text).expect("valid JSON");
    schema::check(&value, &SpanIndex::scan(SourceId(0), text))
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_owned())
        .collect()
}

#[test]
fn an_operator_is_a_subject_and_a_role_and_the_generated_type_reads_it() {
    assert_eq!(codes(OPERATORS), Vec::<String>::new());
    let config: liyasa_config::SiteConfig =
        serde_json::from_str(OPERATORS).expect("the generated type reads the key");
    let written = serde_json::to_value(&config).expect("it serializes");
    assert_eq!(
        written.pointer("/auth/operators/0/subject"),
        Some(&json!("8f14e45fce"))
    );
    assert_eq!(
        written.pointer("/auth/operators/0/role"),
        Some(&json!("owner"))
    );
}
