//! API-50: the processed spec is downloadable as JSON and YAML.
//!
//! "Processed" is the whole of the requirement: after overlays, after the
//! visibility filter. A build that published the source document would be
//! easy to mistake for this one and would hand over the operations
//! `x-liyasa.hidden` exists to withhold.
//!
//! This is also the first output `liyasa-openapi` has ever contributed to a
//! build. Until 2026-09-21 the crate was reachable only from `liyasa
//! validate` and the assistant, so a site declaring `openapi` had its config
//! checked and generated nothing (defect 151) — while 37 of the 47 API rows
//! read `implemented`.

use std::path::PathBuf;

use axum::body::Body;
use http::{Request, StatusCode, header};
use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;
use liyasa_tests::server::{Harness, Setup};
use serde_json::json;

const CONFIG: &str = r#"{
  "name": "Acme docs",
  "seo": { "canonicalOrigin": "https://docs.acme.com" },
  "openapi": [{ "id": "petstore", "source": "petstore.json" }]
}"#;

/// Two operations: one ordinary, one the visibility filter must remove. The
/// summary of the first is what the overlay rewrites.
const SPEC: &str = r#"{
  "openapi": "3.1.0",
  "info": { "title": "Petstore", "version": "1.0.0" },
  "servers": [{ "url": "https://api.petstore.example" }],
  "paths": {
    "/pets": {
      "get": {
        "operationId": "listPets",
        "summary": "SUMMARY FROM THE SOURCE",
        "responses": { "200": { "description": "ok" } }
      }
    },
    "/internal/flush": {
      "post": {
        "operationId": "flushCaches",
        "summary": "Flush the caches",
        "x-liyasa": { "hidden": true },
        "responses": { "204": { "description": "done" } }
      }
    }
  }
}"#;

/// Discovered by the `<spec>.overlay.yaml` convention, not named in config
/// (API-06), so this covers both halves of "processed" at once.
const OVERLAY: &str = r#"overlay: 1.0.0
info: { title: Petstore overlay, version: "1" }
actions:
  - target: $.paths['/pets'].get
    update:
      summary: SUMMARY FROM THE OVERLAY
"#;

fn build_site(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("liyasa-api50-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a project directory");
    for (path, body) in [
        ("liyasa.json", CONFIG),
        ("petstore.json", SPEC),
        ("petstore.overlay.yaml", OVERLAY),
        ("index.md", "---\ntitle: Home\n---\n# Home\n"),
    ] {
        std::fs::write(root.join(path), body).expect("a fixture file");
    }
    let report = engine::build(
        &OsVfs::new(&root),
        &NoGit,
        &root,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    assert!(
        !report.failed(false),
        "the fixture failed to build: {:?}",
        report.diagnostics
    );
    root.join("dist")
}

#[test]
fn the_build_writes_the_processed_spec_as_json_and_yaml() {
    let dist = build_site("written");

    let json_text =
        std::fs::read_to_string(dist.join("openapi/petstore.json")).expect("the JSON download");
    let yaml_text =
        std::fs::read_to_string(dist.join("openapi/petstore.yaml")).expect("the YAML download");

    let doc: serde_json::Value = serde_json::from_str(&json_text).expect("valid JSON");
    assert_eq!(doc["info"]["title"], "Petstore");

    // Overlaid, not the source document.
    assert_eq!(
        doc["paths"]["/pets"]["get"]["summary"], "SUMMARY FROM THE OVERLAY",
        "the discovered overlay was not applied:\n{json_text}"
    );
    assert!(
        !json_text.contains("SUMMARY FROM THE SOURCE"),
        "the source summary survived the overlay:\n{json_text}"
    );

    // Filtered. A hidden operation in a file a static host hands to anyone is
    // the failure this requirement exists to prevent.
    let paths: Vec<&str> = doc["paths"]
        .as_object()
        .expect("a paths object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        paths,
        ["/pets"],
        "the visible operation must survive and the hidden one must not — an \
         empty document would satisfy a bare `!contains` and prove nothing"
    );
    assert!(
        !json_text.contains("flushCaches"),
        "a hidden operation is in the download:\n{json_text}"
    );
    assert!(!yaml_text.contains("flushCaches"), "{yaml_text}");

    // The YAML is the same document, not a second rendering of the source.
    assert!(
        yaml_text.contains("SUMMARY FROM THE OVERLAY"),
        "{yaml_text}"
    );
}

#[tokio::test]
async fn the_server_serves_both_with_their_own_content_types() {
    let dist = build_site("served");
    let (harness, _) = Harness::new(Setup {
        dist: Some(dist),
        site_config: Some(json!({
            "name": "Acme docs",
            "seo": { "canonicalOrigin": "https://docs.acme.com" }
        })),
        ..Setup::new("api50")
    })
    .await;

    for (path, content_type) in [
        ("/openapi/petstore.json", "application/json"),
        ("/openapi/petstore.yaml", "application/yaml"),
    ] {
        let response = harness
            .send(
                Request::builder()
                    .method("GET")
                    .uri(path)
                    .body(Body::empty())
                    .expect("a request"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some(content_type),
            "{path}"
        );
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("a complete body");
        assert!(!bytes.is_empty(), "{path} is empty");
        assert!(
            !String::from_utf8_lossy(&bytes).contains("flushCaches"),
            "{path} serves a hidden operation"
        );
    }
}
