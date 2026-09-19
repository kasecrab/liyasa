//! CFG-96: the config key that names an instance's bootstrap operators, and
//! the one thing about it that is not in this package's hands.
//!
//! An empty membership table elevates nobody, which is the safe direction and
//! also means a fresh instance has nobody who can add the first member
//! (defect 65, RFC 2802). `auth.operators` is the escape hatch. The schema and
//! the semantic rules are WP-01's; reading the key is WP-15's, and the second
//! test here is what says so.

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

/// The server's own `AuthConfig` is `deny_unknown_fields`, so it refuses the
/// whole `auth` block the first time an operator writes this key — not just
/// the key. The schema is WP-01's and that mirror is WP-15's, so this pins the
/// gap rather than papering over it: the day WP-15 adds the field, this test
/// fails and should be turned round to assert the operators are read.
#[test]
fn the_servers_auth_config_does_not_read_the_key_yet() {
    let value: Value = serde_json::from_str(OPERATORS).expect("valid JSON");
    let error = liyasa_server::auth::config::AuthConfig::from_site_config(&value)
        .expect_err("WP-15 added `operators`: assert it is read rather than refused");
    assert!(
        error.message.contains("operators"),
        "the refusal names the key: {}",
        error.message
    );

    // Everything else in the block still loads, so this is the key alone.
    let without = json!({ "mode": "oidc" });
    assert!(liyasa_server::auth::config::AuthConfig::from_value(&without).is_ok());
}
