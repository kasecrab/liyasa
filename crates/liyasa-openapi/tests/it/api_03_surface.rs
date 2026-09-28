//! API-03, API-04, API-15: what a spec contributes to a build.
//!
//! Defect 151: the crate could read, normalize, overlay and filter a spec, and
//! no build called any of it, so a site declaring `openapi` generated nothing.
//! This is the producing half of the fix — every operation page, every schema
//! page, and the navigation the `{ openapi: "id" }` node expands to, as a list
//! the engine can splice in. RFC 0806 is the call site.

use liyasa_core::conformance::fixtures::MemoryVfs;
use liyasa_openapi::build::{self, Authored, PageKind};

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
servers:
  - url: https://api.example.com
tags:
  - { name: Widgets }
  - { name: Admin }
paths:
  /widgets:
    get:
      operationId: listWidgets
      summary: List widgets
      tags: [Widgets]
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: array
                items: { $ref: "#/components/schemas/Widget" }
  /widgets/{id}:
    get:
      operationId: getWidget
      summary: Fetch one widget
      tags: [Widgets]
      parameters:
        - { name: id, in: path, required: true, schema: { type: string } }
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Widget" }
  /admin/flush:
    post:
      operationId: flush
      tags: [Admin]
      responses: { "204": { description: done } }
components:
  schemas:
    Widget:
      type: object
      description: A widget.
      required: [id]
      properties:
        id: { type: string }
        size: { type: integer }
"##;

fn config(extra: &str) -> serde_json::Value {
    let text =
        format!(r#"{{ "openapi": [{{ "id": "api", "source": "openapi/api.yaml"{extra} }}] }}"#);
    serde_json::from_str(&text).expect("the config parses")
}

fn vfs() -> MemoryVfs {
    MemoryVfs::new().with("openapi/api.yaml", SPEC)
}

#[test]
fn every_operation_becomes_a_page_the_engine_can_write() {
    let surface = build::surface(&vfs(), &config(""), &[]);
    assert!(
        !surface.diagnostics.has_errors(),
        "{:?}",
        surface.diagnostics.as_slice()
    );

    let operations: Vec<_> = surface
        .pages
        .iter()
        .filter(|page| page.kind == PageKind::Operation)
        .collect();
    assert_eq!(operations.len(), 3, "one page per operation");

    let one = operations
        .iter()
        .find(|page| page.selector == "GET /widgets/{id}")
        .expect("the operation is there");
    assert_eq!(one.route, "/api-reference/getwidget");
    assert_eq!(one.title, "Fetch one widget");
    assert_eq!(one.spec, "api");
}

#[test]
fn a_page_is_a_whole_markdown_document_front_matter_and_all() {
    let surface = build::surface(&vfs(), &config(""), &[]);
    let page = surface
        .pages
        .iter()
        .find(|page| page.selector == "GET /widgets/{id}")
        .expect("the operation is there");

    let (front, body) = page
        .source
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .expect("the source opens with front matter");

    let front: serde_json::Value = serde_norway::from_str(front).expect("the front matter is YAML");
    assert_eq!(front["title"], "Fetch one widget");
    assert_eq!(
        front["openapi"], "api GET /widgets/{id}",
        "the engine already reads this key, and it is what makes the page an \
         endpoint in search (DocKind::Endpoint)"
    );

    assert!(
        !body.contains("<div") && !body.contains("<span"),
        "the source is Markdown; HTML is the pipeline's job"
    );
}

#[test]
fn the_body_is_the_components_a_manual_page_would_use() {
    let surface = build::surface(&vfs(), &config(""), &[]);
    let page = surface
        .pages
        .iter()
        .find(|page| page.selector == "GET /widgets/{id}")
        .expect("the operation is there");

    assert!(
        page.source.contains(r#":::endpoint{method="GET" path="/widgets/{id}""#),
        "the method pill and the path come from the endpoint component (CMP-43), \
         which the audit of 2026-09-21 checked works end to end:\n{}",
        page.source
    );
    assert!(
        page.source.contains(r#"spec="api""#) && page.source.contains(r#"operation="getWidget""#),
        "and it names the operation, so the dependency graph has the edge"
    );
    assert!(
        page.source.contains(r#":::param{name="id" in="path""#),
        "a parameter is a param row, not a table cell:\n{}",
        page.source
    );

    let opens = page.source.matches("\n:::").count() + usize::from(page.source.starts_with(":::"));
    assert_eq!(
        opens % 2,
        0,
        "every container directive is closed:\n{}",
        page.source
    );
}

#[test]
fn a_value_holding_a_quote_is_left_out_rather_than_breaking_the_document() {
    // Directive props have no escape: `value` in
    // crates/liyasa-markdown/src/directives/props.rs ends the string at the
    // first `"`, so emitting one would silently truncate the prop and swallow
    // whatever followed it.
    let quoted = SPEC.replace(
        r#"        - { name: id, in: path, required: true, schema: { type: string } }"#,
        r#"        - { name: id, in: path, required: true, schema: { type: string }, example: 'say "hello"' }"#,
    );
    let surface = build::surface(
        &MemoryVfs::new().with("openapi/api.yaml", quoted.as_str()),
        &config(""),
        &[],
    );
    assert!(
        !surface.diagnostics.has_errors(),
        "{:?}",
        surface.diagnostics.as_slice()
    );
    let page = surface
        .pages
        .iter()
        .find(|page| page.selector == "GET /widgets/{id}")
        .expect("the operation is there");

    let directive = page
        .source
        .lines()
        .find(|line| line.starts_with(":::param"))
        .expect("the parameter is a param row");
    assert!(
        !directive.contains("hello"),
        "an unescapable value is dropped from the props: {directive}"
    );
    assert!(
        directive.matches('"').count() % 2 == 0,
        "so the quotes stay balanced: {directive}"
    );
}

#[test]
fn the_navigation_node_expands_to_the_groups_the_spec_declares() {
    let surface = build::surface(&vfs(), &config(""), &[]);
    let reference = surface
        .navigations
        .iter()
        .find(|reference| reference.spec == "api")
        .expect("the spec contributes navigation");

    assert_eq!(
        reference
            .groups
            .iter()
            .map(|group| group.title.as_str())
            .collect::<Vec<_>>(),
        vec!["Widgets", "Admin"]
    );
    for entry in reference.entries() {
        assert!(
            surface.pages.iter().any(|page| page.route == entry.route),
            "navigation points at {} and no page has that route",
            entry.route
        );
    }
}

#[test]
fn schema_pages_are_off_by_default_and_on_when_asked() {
    let without = build::surface(&vfs(), &config(""), &[]);
    assert!(
        !without
            .pages
            .iter()
            .any(|page| page.kind == PageKind::Schema),
        "a site with six schemas does not want six more pages"
    );

    let with = build::surface(&vfs(), &config(r#", "schemaPages": true"#), &[]);
    let schema = with
        .pages
        .iter()
        .find(|page| page.kind == PageKind::Schema)
        .expect("a page per component schema");
    assert_eq!(
        schema.route, "/api-reference/schemas/widget",
        "slugged and lowercase, the same shape an operation route has"
    );
    assert_eq!(schema.selector, "Widget");
    assert!(
        schema.source.contains("Used by"),
        "API-15 asks for the usage links: {}",
        schema.source
    );
}

#[test]
fn an_authored_page_keeps_its_route_and_its_body_goes_above_the_parameters() {
    let authored = [Authored {
        route: "/guides/fetching".to_owned(),
        selector: "api GET /widgets/{id}".to_owned(),
        body: "Read this first.".to_owned(),
    }];
    let surface = build::surface(&vfs(), &config(""), &authored);

    assert!(
        !surface
            .pages
            .iter()
            .any(|page| page.route == "/api-reference/getwidget"),
        "the authored page is that operation's page; generating a second one \
         would give the operation two routes"
    );
    let augmented = surface
        .pages
        .iter()
        .find(|page| page.route == "/guides/fetching")
        .expect("the authored route is kept");
    let body = augmented
        .source
        .split_once("\n---\n")
        .map(|(_, body)| body)
        .expect("front matter");
    let intro = body
        .find("Read this first.")
        .expect("the intro is rendered");
    let parameters = body.find(":::param").expect("and the reference");
    assert!(
        intro < parameters,
        "the body renders above the parameters, below the method and path"
    );
    let endpoint = body.find(":::endpoint").expect("the header is there");
    assert!(endpoint < intro, "and below the method and path");

    assert_eq!(
        surface
            .pages
            .iter()
            .filter(|page| page.kind == PageKind::Operation)
            .count(),
        3,
        "still one page per operation, one of them the authored route"
    );
}

#[test]
fn a_hidden_operation_contributes_no_page_at_all() {
    let hidden = SPEC.replace(
        "      operationId: flush\n",
        "      operationId: flush\n      x-liyasa: { hidden: true }\n",
    );
    let surface = build::surface(
        &MemoryVfs::new().with("openapi/api.yaml", hidden.as_str()),
        &config(""),
        &[],
    );
    assert!(
        !surface
            .pages
            .iter()
            .any(|page| page.selector.contains("flush")),
        "x-liyasa.hidden removes it from navigation, search and Markdown"
    );
    assert_eq!(
        surface
            .pages
            .iter()
            .filter(|page| page.kind == PageKind::Operation)
            .count(),
        2
    );
}

#[test]
fn an_overlay_beside_the_spec_is_applied_before_any_page_is_built() {
    let overlay = r#"
overlay: 1.0.0
info: { title: Fixes, version: "1" }
actions:
  - target: "$.paths['/widgets'].get"
    update: { summary: "Every widget we have" }
"#;
    let vfs = MemoryVfs::new()
        .with("openapi/api.yaml", SPEC)
        .with("openapi/api.overlay.yaml", overlay);
    let surface = build::surface(&vfs, &config(""), &[]);
    assert!(
        !surface.diagnostics.has_errors(),
        "{:?}",
        surface.diagnostics.as_slice()
    );
    let page = surface
        .pages
        .iter()
        .find(|page| page.selector == "GET /widgets")
        .expect("the operation is there");
    assert_eq!(
        page.title, "Every widget we have",
        "the page is built from the processed document, not the source one"
    );
}

#[test]
fn a_spec_that_cannot_be_read_is_a_diagnostic_and_not_a_panic() {
    let surface = build::surface(&MemoryVfs::new(), &config(""), &[]);
    assert!(surface.pages.is_empty());
    assert!(
        surface.diagnostics.has_errors(),
        "a missing source is worth saying"
    );
}

#[test]
fn a_remote_source_says_so_rather_than_generating_nothing_silently() {
    let config = serde_json::json!({
        "openapi": [{ "id": "api", "source": "https://example.com/api.yaml" }]
    });
    let surface = build::surface(&MemoryVfs::new(), &config, &[]);
    assert!(surface.pages.is_empty());
    assert!(
        surface
            .diagnostics
            .as_slice()
            .iter()
            .any(|diagnostic| diagnostic.message.contains("remote")),
        "{:?}",
        surface.diagnostics.as_slice()
    );
}
