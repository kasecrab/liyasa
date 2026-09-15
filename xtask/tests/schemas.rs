//! `schemas/` is a published contract (PRD §34.9, CFG-94).

use std::path::{Path, PathBuf};

fn schemas_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../schemas")
}

fn read(name: &str) -> serde_json::Value {
    let path = schemas_dir().join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn every_schema() -> Vec<(String, serde_json::Value)> {
    let mut out = Vec::new();
    let dir = std::fs::read_dir(schemas_dir()).expect("schemas/ exists");
    for entry in dir {
        let name = entry
            .expect("readable entry")
            .file_name()
            .to_string_lossy()
            .into_owned();
        if name.ends_with(".json") {
            let value = read(&name);
            out.push((name, value));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(
        out.len() >= 7,
        "expected every published schema, found {}",
        out.len()
    );
    out
}

#[test]
fn every_schema_declares_2020_12_and_an_id() {
    for (name, schema) in every_schema() {
        assert_eq!(
            schema["$schema"], "https://json-schema.org/draft/2020-12/schema",
            "{name} must declare JSON Schema 2020-12"
        );
        let id = schema["$id"].as_str().unwrap_or_default();
        assert!(
            id.starts_with("https://liyasa.dev/schema/v1/"),
            "{name} has $id `{id}`"
        );
        assert!(schema["title"].is_string(), "{name} has no title");
    }
}

#[test]
fn every_ref_resolves() {
    for (name, schema) in every_schema() {
        let defs = &schema["$defs"];
        let mut stack = vec![&schema];
        while let Some(node) = stack.pop() {
            match node {
                serde_json::Value::Object(map) => {
                    match map.get("$ref").and_then(serde_json::Value::as_str) {
                        // The root ref is how a recursive type refers to itself.
                        Some("#") | None => {}
                        Some(reference) => {
                            let target = reference.strip_prefix("#/$defs/").unwrap_or_else(|| {
                                panic!("{name}: unsupported $ref `{reference}`")
                            });
                            assert!(
                                !defs[target].is_null(),
                                "{name}: $ref `{reference}` has no definition"
                            );
                        }
                    }
                    stack.extend(map.values());
                }
                serde_json::Value::Array(items) => stack.extend(items),
                _ => {}
            }
        }
    }
}

#[test]
fn generated_schemas_match_the_frozen_rust_types() {
    // The Rust declarations are the source of truth for these four. If this
    // fails, run `cargo run -p xtask -- schemas` and commit the result.
    for generated in xtask::schemas::generate() {
        let current = std::fs::read_to_string(schemas_dir().join(generated.file))
            .unwrap_or_else(|e| panic!("{}: {e}", generated.file));
        assert_eq!(current, generated.json, "{} is stale", generated.file);
    }
}

#[test]
fn the_config_schema_names_a_requirement_for_every_top_level_key() {
    // CFG-94: the §8 tables are generated from this file, so a key with no
    // requirement annotation has nothing to generate a row from.
    let schema = read("liyasa.schema.json");
    let properties = schema["properties"].as_object().expect("has properties");
    let annotated = |value: &serde_json::Value| {
        value.get("x-liyasa-requirement").is_some()
            || value.get("description").is_some()
            || value.get("oneOf").is_some_and(|v| {
                v.as_array()
                    .is_some_and(|a| a.iter().any(|x| x.get("description").is_some()))
            })
    };
    for (key, value) in properties {
        assert!(
            annotated(value),
            "`{key}` has neither a requirement id nor a description"
        );
    }
    assert_eq!(
        schema["required"],
        serde_json::json!(["name"]),
        "only `name` is required"
    );
}

#[test]
fn the_manifest_schema_covers_what_consumers_read() {
    let schema = read("manifest.json");
    let route = &schema["properties"]["routes"]["items"]["properties"];
    for key in [
        "route", "pageId", "source", "html", "markdown", "variants", "hashes", "blocks",
    ] {
        assert!(
            !route[key].is_null(),
            "manifest route entry is missing `{key}`"
        );
    }
    for key in [
        "buildId",
        "builtAt",
        "redirects",
        "assets",
        "agentSurfaces",
        "diagnostics",
    ] {
        assert!(
            !schema["properties"][key].is_null(),
            "manifest is missing `{key}`"
        );
    }
}

#[test]
fn the_webhook_schema_lists_every_documented_event_type() {
    let schema = read("webhook.json");
    let types = schema["properties"]["type"]["enum"]
        .as_array()
        .expect("event types are an enum");
    for expected in [
        "deployment.queued",
        "deployment.rolled_back",
        "drift.created",
        "proposal.merged",
        "feedback.received",
        "automation.failed",
        "build.queue_full",
    ] {
        assert!(
            types.iter().any(|t| t == expected),
            "webhook schema does not list `{expected}`"
        );
    }
}
