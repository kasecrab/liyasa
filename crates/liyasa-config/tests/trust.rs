//! CFG-95: what an untrusted build reads from the deploy branch.

use liyasa_config::trust::{self, FILES, SECTIONS};
use serde_json::{Value, json};

fn codes(diagnostics: &liyasa_core::diagnostics::Diagnostics) -> Vec<&str> {
    diagnostics.iter().map(|d| d.code.as_str()).collect()
}

fn deploy() -> Value {
    json!({
        "name": "Acme",
        "security": { "styleAttribute": "allowlist" },
        "network": { "allowHosts": { "specRefs": ["api.acme.com"] } },
        "build": { "output": "dist", "env": ["CI"] },
        "openapi": [{ "id": "api", "source": "https://api.acme.com/openapi.json" }]
    })
}

/// One step down a schema, through every branch of a `oneOf`: `redirects` is a
/// choice between an array and an object, and `externalAllow` is in the object.
fn property<'a>(nodes: &[&'a Value], key: &str) -> Vec<&'a Value> {
    let mut found = Vec::new();
    let mut stack: Vec<&Value> = nodes.to_vec();
    while let Some(node) = stack.pop() {
        if let Some(property) = node.pointer("/properties").and_then(|p| p.get(key)) {
            found.push(property);
        }
        for branch in ["oneOf", "anyOf", "allOf"] {
            if let Some(Value::Array(branches)) = node.get(branch) {
                stack.extend(branches);
            }
        }
    }
    found
}

#[test]
fn every_section_is_a_key_the_schema_knows() {
    let schema: Value =
        serde_json::from_str(liyasa_config::schema::CONFIG_SCHEMA).expect("valid JSON");
    for pointer in SECTIONS {
        let mut nodes = vec![&schema];
        for segment in pointer.trim_start_matches('/').split('/') {
            nodes = property(&nodes, segment);
            assert!(!nodes.is_empty(), "`{pointer}` is not in the schema");
        }
    }
}

#[test]
fn a_branch_that_changes_nothing_is_not_a_trust_plane_change() {
    let config = deploy();
    let trusted = trust::apply(&config, &config);
    assert_eq!(trusted.changed, Vec::<&str>::new());
    assert!(trusted.diagnostics.is_empty());
    assert_eq!(trusted.config, config);
}

#[test]
fn a_changed_section_is_replaced_and_reported() {
    let untrusted = json!({
        "name": "Acme",
        "security": { "styleAttribute": "off" },
        "network": { "allowHosts": { "specRefs": ["api.acme.com"] } },
        "build": { "output": "public", "env": ["CI", "SECRET"] },
        "openapi": [{ "id": "api", "source": "https://api.acme.com/openapi.json" }]
    });
    let trusted = trust::apply(&deploy(), &untrusted);

    assert_eq!(trusted.changed, ["/build/env", "/security"]);
    assert_eq!(codes(&trusted.diagnostics), ["W0134", "W0134"]);
    assert_eq!(
        trusted.config.pointer("/security/styleAttribute"),
        Some(&json!("allowlist"))
    );
    assert_eq!(trusted.config.pointer("/build/env"), Some(&json!(["CI"])));
    // Everything outside the trust plane is this branch's to change.
    assert_eq!(
        trusted.config.pointer("/build/output"),
        Some(&json!("public"))
    );
}

#[test]
fn a_section_the_deploy_branch_does_not_set_is_removed() {
    let untrusted = json!({ "name": "Acme", "ai": { "respectNoindex": false } });
    let trusted = trust::apply(&json!({ "name": "Acme" }), &untrusted);
    assert_eq!(trusted.changed, ["/ai"]);
    assert_eq!(trusted.config, json!({ "name": "Acme" }));
}

#[test]
fn a_remote_spec_source_this_branch_invented_is_e0135() {
    let mut untrusted = deploy();
    untrusted["openapi"] = json!([
        { "id": "api", "source": "https://api.acme.com/openapi.json" },
        { "id": "rogue", "source": "https://elsewhere.example/openapi.json" }
    ]);
    let trusted = trust::apply(&deploy(), &untrusted);
    assert_eq!(codes(&trusted.diagnostics), ["E0135"]);
    assert_eq!(trusted.changed, ["/openapi"]);
    assert_eq!(
        trusted.config.pointer("/openapi/1"),
        Some(&json!({ "id": "rogue" })),
        "the entry stays so the id still resolves, without a source to fetch"
    );
}

#[test]
fn a_local_spec_file_in_this_branch_is_allowed() {
    let mut untrusted = deploy();
    untrusted["openapi"] = json!([
        { "id": "api", "source": "https://api.acme.com/openapi.json" },
        { "id": "draft", "source": "openapi/draft.yaml" }
    ]);
    let trusted = trust::apply(&deploy(), &untrusted);
    assert!(trusted.diagnostics.is_empty(), "{:?}", trusted.diagnostics);
    assert_eq!(
        trusted.config.pointer("/openapi/1/source"),
        Some(&json!("openapi/draft.yaml"))
    );
}

#[test]
fn an_overlay_url_is_held_to_the_same_list() {
    let deploy = json!({
        "name": "Acme",
        "openapi": [{ "id": "api", "source": "openapi/api.yaml",
                      "overlays": ["https://acme.com/overlay.yaml"] }]
    });
    let untrusted = json!({
        "name": "Acme",
        "openapi": [{ "id": "api", "source": "openapi/api.yaml",
                      "overlays": ["https://acme.com/overlay.yaml",
                                   "https://elsewhere.example/overlay.yaml"] }]
    });
    let trusted = trust::apply(&deploy, &untrusted);
    assert_eq!(codes(&trusted.diagnostics), ["E0135"]);
    assert_eq!(
        trusted.config.pointer("/openapi/0/overlays"),
        Some(&json!(["https://acme.com/overlay.yaml"]))
    );
}

#[test]
fn the_files_the_deploy_branch_owns() {
    assert!(trust::is_trusted_file("AGENTS.md"));
    assert!(trust::is_trusted_file("DOCOWNERS"));
    assert!(trust::is_trusted_file(".liyasa-aiignore"));
    assert!(trust::is_trusted_file("automations/nightly.md"));
    assert!(!trust::is_trusted_file("docs/AGENTS.md"));
    assert!(!trust::is_trusted_file("automations.md"));
    assert_eq!(FILES.len(), 4);
}
