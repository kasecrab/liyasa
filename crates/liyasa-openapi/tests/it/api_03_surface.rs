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
        page.source
            .contains(r#":::endpoint{method="get" path="/widgets/{id}""#),
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

/// API-02 and API-07: which documents a spec's `$ref`s may reach when its pages
/// are built.
mod documents {
    use super::*;

    const SPLIT: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
paths:
  /widgets:
    get:
      operationId: listWidgets
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema: { $ref: "shared.yaml#/components/schemas/Money" }
"##;

    const SHARED: &str = r##"
components:
  schemas:
    Money:
      type: object
      properties:
        amount: { type: integer }
        currency: { type: string }
"##;

    const BILLING: &str = r##"
openapi: 3.1.0
info: { title: Billing, version: "1" }
paths: {}
components:
  schemas:
    Money:
      type: object
      properties:
        cents: { type: integer }
"##;

    /// A spec may be split across a directory of files (API-02), and until this
    /// was tested `surface` inserted only the root document, so every `$ref`
    /// leaving the file failed and the page rendered without those fields.
    #[test]
    fn a_ref_into_another_file_of_the_same_spec_resolves() {
        let vfs = MemoryVfs::new()
            .with("openapi/api.yaml", SPLIT)
            .with("openapi/shared.yaml", SHARED);
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
        assert!(
            page.source.contains("amount") && page.source.contains("currency"),
            "the referenced schema's fields reach the page:\n{}",
            page.source
        );
    }

    /// API-07: cross-spec `$ref` is disabled by default. A sibling spec's source
    /// is a readable local file, so this has to be refused deliberately — the
    /// traversal above would otherwise have turned the default off.
    #[test]
    fn a_ref_into_another_declared_spec_is_refused() {
        let vfs = MemoryVfs::new()
            .with(
                "openapi/api.yaml",
                SPLIT.replace("shared.yaml", "billing.yaml").as_str(),
            )
            .with("openapi/billing.yaml", BILLING);
        let config = serde_json::json!({ "openapi": [
            { "id": "api", "source": "openapi/api.yaml" },
            { "id": "billing", "source": "openapi/billing.yaml" }
        ]});
        let surface = build::surface(&vfs, &config, &[]);

        let refused = surface
            .diagnostics
            .as_slice()
            .iter()
            .find(|d| d.message.contains("another declared spec"))
            .expect("the cross-spec reference is named");
        assert_eq!(refused.code.as_str(), "E0502");
        assert!(
            refused
                .help
                .as_deref()
                .is_some_and(|help| help.contains("off by default")),
            "and the help says what to do instead: {:?}",
            refused.help
        );

        let page = surface
            .pages
            .iter()
            .find(|page| page.spec == "api" && page.selector == "GET /widgets")
            .expect("the rest of the spec still renders");
        assert!(
            !page.source.contains("cents"),
            "the other spec's schema did not leak into this page:\n{}",
            page.source
        );
    }

    /// The same file reached twice is read once, and a cycle between documents
    /// terminates.
    #[test]
    fn a_reference_cycle_between_files_terminates() {
        let a = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
paths:
  /widgets:
    get:
      operationId: listWidgets
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema: { $ref: "b.yaml#/components/schemas/Node" }
"##;
        let b = r##"
components:
  schemas:
    Node:
      type: object
      properties:
        next: { $ref: "b.yaml#/components/schemas/Node" }
        name: { type: string }
"##;
        let vfs = MemoryVfs::new()
            .with("openapi/api.yaml", a)
            .with("openapi/b.yaml", b);

        let started = std::time::Instant::now();
        let surface = build::surface(&vfs, &config(""), &[]);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "it took {:?}",
            started.elapsed()
        );
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
        assert!(page.source.contains("name"), "{}", page.source);
    }
}

/// `endpoint` is declared `kind = Container`, and `ast/build.rs` cannot tell a
/// container written with no body from a leaf except by the span it occupies —
/// so an empty one is `E0317`, a kind mismatch. Every generated endpoint must
/// therefore carry a body, whatever the spec says about it.
#[test]
fn the_endpoint_container_is_never_written_empty() {
    // An operation with no description and no summary: the worst case.
    let bare = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
paths:
  /widgets:
    get:
      operationId: listWidgets
      responses: { "204": { description: "" } }
"##;
    let surface = build::surface(
        &MemoryVfs::new().with("openapi/api.yaml", bare),
        &config(""),
        &[],
    );
    let page = surface
        .pages
        .iter()
        .find(|page| page.selector == "GET /widgets")
        .expect("the operation is there");

    let open = page
        .source
        .find(":::endpoint")
        .expect("the endpoint directive is there");
    let after = &page.source[open..];
    let first_line_end = after.find('\n').expect("the directive has a line");
    let rest = after[first_line_end + 1..].trim_start();
    assert!(
        !rest.starts_with(":::"),
        "the container closes immediately, so it reads as a leaf and is E0317:\n{}",
        page.source
    );
}

#[test]
fn the_description_goes_inside_the_endpoint_container() {
    let surface = build::surface(&vfs(), &config(""), &[]);
    let page = surface
        .pages
        .iter()
        .find(|page| page.selector == "GET /widgets/{id}")
        .expect("the operation is there");

    let open = page.source.find(":::endpoint").expect("the directive");
    let close = page.source[open..]
        .find("\n:::")
        .map(|at| open + at)
        .expect("the directive closes");
    let inside = &page.source[open..close];
    assert!(
        inside.lines().count() > 1,
        "the container holds its prose rather than closing empty:\n{inside}"
    );
}

/// API-04's second clause: `:::slot{name="after-params"}` and its siblings
/// inject content at defined points.
///
/// This was missing until 2026-09-30 and the miss was mine: `markdown::render`
/// emitted the slots, and when the page SOURCE became component directives
/// instead, the generator stopped emitting them. The clause quietly went from
/// met to unmet with no test to notice.
mod slots {
    use super::*;

    fn authored() -> Vec<Authored> {
        vec![Authored {
            route: "/guides/fetching".to_owned(),
            selector: "api GET /widgets/{id}".to_owned(),
            body: "Read this first.".to_owned(),
        }]
    }

    fn page_source(slots: Vec<(String, String)>) -> String {
        let filled = [build::AuthoredSlots {
            route: "/guides/fetching".to_owned(),
            slots,
        }];
        let surface = build::surface_with_slots(&vfs(), &config(""), &authored(), &filled);
        surface
            .pages
            .iter()
            .find(|page| page.route == "/guides/fetching")
            .expect("the authored route is kept")
            .source
            .clone()
    }

    #[test]
    fn after_params_lands_between_the_parameters_and_the_responses() {
        let source = page_source(vec![(
            "after-params".to_owned(),
            "See the tenant guide.".to_owned(),
        )]);
        let injected = source
            .find("See the tenant guide.")
            .expect("it is injected");
        let last_param = source.rfind(":::param").expect("there are parameters");
        let responses = source.find("## Responses").expect("and responses");
        assert!(
            last_param < injected && injected < responses,
            "after-params sits after the parameters and before the responses:\n{source}"
        );
    }

    #[test]
    fn every_documented_slot_reaches_the_source_at_its_own_point() {
        let source = page_source(vec![
            ("before-request".to_owned(), "BEFORE_REQUEST".to_owned()),
            ("after-params".to_owned(), "AFTER_PARAMS".to_owned()),
            ("before-responses".to_owned(), "BEFORE_RESPONSES".to_owned()),
            ("after-responses".to_owned(), "AFTER_RESPONSES".to_owned()),
        ]);
        let at = |needle: &str| {
            source
                .find(needle)
                .unwrap_or_else(|| panic!("{needle} missing:\n{source}"))
        };
        assert!(at("AFTER_PARAMS") < at("BEFORE_RESPONSES"));
        assert!(at("BEFORE_RESPONSES") < at("AFTER_RESPONSES"));
        assert!(
            at("BEFORE_REQUEST") < at("BEFORE_RESPONSES"),
            "before-request precedes the responses:\n{source}"
        );
    }

    /// The rail's two slots are not body content, so they must NOT be written
    /// into the source — they travel in the Augmentation for the reader.
    #[test]
    fn a_rail_slot_is_not_written_into_the_markdown() {
        let source = page_source(vec![
            ("rail-top".to_owned(), "RAIL_TOP".to_owned()),
            ("rail-bottom".to_owned(), "RAIL_BOTTOM".to_owned()),
        ]);
        assert!(!source.contains("RAIL_TOP"), "{source}");
        assert!(!source.contains("RAIL_BOTTOM"), "{source}");
    }

    #[test]
    fn a_name_that_is_not_a_slot_is_dropped_rather_than_injected() {
        let source = page_source(vec![("after-everything".to_owned(), "NOPE".to_owned())]);
        assert!(
            !source.contains("NOPE"),
            "an unknown slot name injects nothing; reporting it is the caller's, \
             with the list of names that exist:\n{source}"
        );
    }

    #[test]
    fn a_generated_page_with_no_authored_slots_is_unchanged() {
        let with = page_source(Vec::new());
        let plain = build::surface(&vfs(), &config(""), &[])
            .pages
            .iter()
            .find(|page| page.selector == "GET /widgets/{id}")
            .expect("the operation is there")
            .source
            .clone();
        // The authored page carries an intro the plain one does not, so compare
        // the part that should not move.
        assert_eq!(
            with.matches(":::param").count(),
            plain.matches(":::param").count(),
            "no slots means no change to the rows"
        );
    }
}
