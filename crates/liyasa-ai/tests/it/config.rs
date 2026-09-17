//! The `ai` object as it appears in a real `liyasa.json`, not one the test
//! wrote for itself.

use std::path::PathBuf;

use liyasa_ai::config::{AiConfig, Role};

fn workspace_file(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn read(relative: &str) -> serde_json::Value {
    let path = workspace_file(relative);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_config_crates_example_site_parses() {
    let config = read("crates/liyasa-config/tests/fixtures/example.json");
    let ai = AiConfig::from_site(&config).expect("ai parses");

    assert!(ai.assistant.enabled);
    assert_eq!(ai.assistant.name, "Acme Assistant");
    assert_eq!(
        ai.model_for(Role::Assistant).map(ToString::to_string),
        Some("anthropic:claude-sonnet-5".to_owned())
    );
    assert_eq!(
        ai.model_for(Role::Embeddings).map(ToString::to_string),
        Some("openai:text-embedding-3-small".to_owned())
    );
    // Not routed by that site, and the row must not invent a fallback.
    assert_eq!(ai.model_for(Role::Rerank), None);
}

#[test]
fn a_site_with_no_ai_object_takes_the_defaults() {
    let config = read("crates/liyasa-cli/templates/starter/liyasa.json");
    let ai = AiConfig::from_site(&config).expect("ai parses");
    assert!(!ai.assistant.enabled);
    assert_eq!(ai.reindex.auto_approve_cents, 500);
}
