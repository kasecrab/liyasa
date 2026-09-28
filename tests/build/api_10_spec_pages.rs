//! API-10 and API-11 through a whole build: a spec's operations and schemas are
//! pages, with no source file behind them (RFC 0608).
//!
//! `liyasa_openapi` renders each one as a whole Markdown source document and
//! this crate injects it into the content tree, so the HTML, the `<route>.md`
//! twin and the search index all come from one parse of it. That is the property
//! the test asserts rather than the shape of any one output: a generated page is
//! indistinguishable downstream from a written one.

use std::fs;
use std::path::PathBuf;

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;

struct Project(PathBuf);

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const SPEC: &str = r#"{
  "openapi": "3.1.0",
  "info": { "title": "Widgets", "version": "1.0.0" },
  "paths": {
    "/widgets/{id}": {
      "get": {
        "operationId": "getWidget",
        "summary": "Fetch one widget",
        "parameters": [
          { "name": "id", "in": "path", "required": true,
            "description": "The widget's id.",
            "schema": { "type": "string" } }
        ],
        "responses": { "200": { "description": "One widget." } }
      }
    }
  }
}"#;

fn site() -> Project {
    let root = std::env::temp_dir().join(format!("liyasa-api-10-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a project directory");
    fs::write(
        root.join("liyasa.json"),
        r#"{
          "name": "Acme docs",
          "seo": { "canonicalOrigin": "https://docs.acme.com" },
          "openapi": [{ "id": "api", "source": "openapi.json" }]
        }"#,
    )
    .expect("config");
    fs::write(root.join("openapi.json"), SPEC).expect("a spec");
    fs::write(
        root.join("index.md"),
        "---\ntitle: Home\n---\n# Home\n\nThe landing page.\n",
    )
    .expect("a home page");
    Project(root)
}

fn build(project: &Project) -> engine::Report {
    let vfs = OsVfs::new(&project.0);
    engine::build(
        &vfs,
        &NoGit,
        &project.0,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    )
}

#[test]
fn an_operation_becomes_a_page_with_no_file_behind_it() {
    let project = site();
    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    let routes: Vec<&str> = report
        .manifest
        .as_ref()
        .expect("a manifest")
        .routes
        .iter()
        .map(|entry| entry.route.as_str())
        .collect();
    assert!(
        routes.iter().any(|route| route.contains("getwidget")),
        "the operation has a route: {routes:?}"
    );
}

/// The point of routing a generated page through the pipeline rather than
/// serializing it separately: every downstream surface sees it.
#[test]
fn a_generated_page_reaches_the_html_and_the_markdown_twin() {
    let project = site();
    let report = build(&project);
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    let route = report
        .manifest
        .as_ref()
        .expect("a manifest")
        .routes
        .iter()
        .map(|entry| entry.route.as_str().to_owned())
        .find(|route| route.contains("getwidget"))
        .expect("the operation has a route");
    // Taken from the manifest rather than spelled here: the base is the spec
    // config's to choose, and this test is about the page existing everywhere a
    // written page would, not about where it sits.
    let stem = route.trim_matches('/');
    let dist = project.0.join("dist");
    assert!(
        dist.join(format!("{stem}/index.html")).exists(),
        "the operation has HTML at /{stem}"
    );
    assert!(
        dist.join(format!("{stem}.md")).exists(),
        "the operation has a Markdown twin at /{stem}.md"
    );
    let html = dist.join(format!("{stem}/index.html"));

    let body = fs::read_to_string(&html).expect("the page");
    assert!(
        body.contains("Fetch one widget"),
        "the summary is the page title"
    );
}
