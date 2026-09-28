//! API-03, API-10, API-11: a generated reference page renders.
//!
//! `liyasa_openapi::build::surface` produces each page as Markdown source, and
//! its own tests assert what that source says. They cannot assert that it is
//! *valid* — that the directives parse, the props are the ones the components
//! declare, and the HTML carries what a reader sees. A test that only checks
//! the strings the generator was written to emit passes whether or not anything
//! downstream can read them.
//!
//! So this puts a generated page through the build's own renderer, the same
//! `render::from_expanded` the engine calls for a page on disk.
//!
//! It does not need the engine, which is why it can exist before RFC 0806's
//! call site does.

use std::collections::BTreeSet;

use liyasa_build::render::{self, Options};
use liyasa_components::registry::Registry;
use liyasa_core::conformance::fixtures::MemoryVfs;
use liyasa_core::ids::{Locale, Route};
use liyasa_core::markdown::{Expanded, ExpansionRecord, SiteMeta, SpanMap};
use liyasa_openapi::build::{self, PageKind};
use url::Url;

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
servers:
  - url: https://api.example.com
paths:
  /widgets/{id}:
    get:
      operationId: getWidget
      summary: Fetch one widget
      description: Returns the widget with that id.
      tags: [Widgets]
      parameters:
        - name: id
          in: path
          required: true
          description: The widget's id.
          schema: { type: string }
        - name: verbose
          in: query
          schema: { type: boolean, default: false }
      responses:
        "200":
          description: The widget
          content:
            application/json:
              schema:
                type: object
                required: [id]
                properties:
                  id: { type: string }
                  shipping:
                    type: object
                    properties:
                      postcode: { type: string, minLength: 4 }
    put:
      operationId: replaceWidget
      summary: Replace a widget
      tags: [Widgets]
      parameters:
        - { name: id, in: path, required: true, schema: { type: string } }
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              properties:
                name: { type: string }
      responses: { "200": { description: ok } }
"##;

fn site() -> SiteMeta {
    SiteMeta {
        name: "Acme".to_owned(),
        canonical_origin: Url::parse("https://example.invalid").expect("an origin"),
        llms_txt: Url::parse("https://example.invalid/llms.txt").expect("an llms.txt"),
        version: None,
        locale: Locale::new("en"),
    }
}

/// The generated source for one operation, front matter stripped: front matter
/// is the engine's to read, and the renderer takes the body.
fn body(selector: &str) -> String {
    let surface = build::surface(
        &MemoryVfs::new().with("openapi/api.yaml", SPEC),
        &serde_json::json!({ "openapi": [{ "id": "api", "source": "openapi/api.yaml" }] }),
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
        .find(|page| page.kind == PageKind::Operation && page.selector == selector)
        .unwrap_or_else(|| panic!("no page for `{selector}`"));
    page.source
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(_, body)| body.to_owned())
        .expect("the source carries front matter")
}

fn rendered(source: &str) -> render::Page {
    let registry = Registry::builtins();
    let site = site();
    let expanded = Expanded {
        text: source.to_owned(),
        map: SpanMap::default(),
        record: ExpansionRecord::default(),
    };
    render::from_expanded(&expanded, &Options::new(&registry, &site))
}

#[test]
fn a_generated_page_renders_with_no_diagnostic_at_all() {
    let page = rendered(&body("GET /widgets/{id}"));
    assert!(
        !page.diagnostics.has_errors(),
        "the generated directives and their props are the ones the components \
         declare:\n{:#?}",
        page.diagnostics
    );
    assert!(
        page.diagnostics.as_slice().is_empty(),
        "not even a warning:\n{:#?}",
        page.diagnostics
    );
}

#[test]
fn the_method_pill_and_the_path_reach_the_html() {
    let page = rendered(&body("GET /widgets/{id}"));
    assert!(
        page.html.contains(r#"data-liyasa="endpoint""#),
        "the endpoint component rendered:\n{}",
        page.html
    );
    assert!(
        page.html.contains(r#"data-method="get""#),
        "the prop carries the spec's lowercase method, which is the set \
         `endpoint.method`'s `one_of` declares:\n{}",
        page.html
    );
    assert!(
        page.html.contains(">GET<"),
        "and the component upper-cases it for the pill a reader sees:\n{}",
        page.html
    );
    assert!(page.html.contains("/widgets/{id}"), "{}", page.html);
    assert!(
        page.html.contains(r#"data-operation="getWidget""#),
        "and it names the operation, which is the dependency edge:\n{}",
        page.html
    );
}

#[test]
fn every_parameter_is_a_row_a_reader_can_link_to() {
    let page = rendered(&body("GET /widgets/{id}"));
    for name in ["id", "verbose"] {
        assert!(
            page.html.contains(name),
            "`{name}` is missing from the page:\n{}",
            page.html
        );
    }
    assert!(
        page.html.contains("The widget's id."),
        "a parameter's description is its content block, so it renders as \
         Markdown:\n{}",
        page.html
    );
}

#[test]
fn a_nested_object_reaches_the_page_as_its_own_row() {
    let page = rendered(&body("GET /widgets/{id}"));
    assert!(
        page.html.contains("shipping.postcode"),
        "a field inside an object is flattened onto its own row rather than \
         nested in a longer fence:\n{}",
        page.html
    );
}

#[test]
fn a_request_body_renders_under_its_own_heading() {
    let page = rendered(&body("PUT /widgets/{id}"));
    assert!(!page.diagnostics.has_errors(), "{:#?}", page.diagnostics);
    assert!(page.html.contains("Body"), "{}", page.html);
    assert!(
        page.html.contains("name"),
        "the body's fields are rows:\n{}",
        page.html
    );
}

/// API-14: the Markdown twin, rendered through the surface that actually builds
/// it.
///
/// `render::from_expanded`'s `page.markdown` is NOT the twin: it goes through
/// `liyasa_markdown`'s generic AST walk, which does not consult the registry.
/// The twin is `agents::markdown::render_page`, which builds a
/// `liyasa_components::Reference` under `Audience::Agent`, and that is what
/// reaches `api::table_markdown` for an RX-61 field run. Testing the wrong one
/// of the two is how this test first concluded the table was unreachable.
#[test]
fn the_markdown_twin_is_a_table_and_carries_no_html() {
    let registry = Registry::builtins();
    let site = site();
    let page = rendered(&body("GET /widgets/{id}"));
    let document = page.document.as_ref().expect("the render keeps the AST");
    let route = Route::new("/api-reference/getwidget");
    let routes = BTreeSet::new();

    let twin = liyasa_build::agents::markdown::render_page(
        document,
        &liyasa_build::agents::markdown::Options {
            site: &site,
            registry: &registry,
            route: &route,
            frontmatter: None,
            routes: &routes,
            site_instructions: None,
            openapi_schema: None,
        },
    );

    assert!(
        !twin.diagnostics.has_errors(),
        "{:#?}",
        twin.diagnostics
    );
    for tag in ["<div", "<span", "<table"] {
        assert!(
            !twin.markdown.contains(tag),
            "the twin holds no HTML, so nothing has to be stripped at the \
             other end; found `{tag}`:\n{}",
            twin.markdown
        );
    }
    assert!(
        twin.markdown.contains('|'),
        "a run of field components merges into one table (RX-61):\n{}",
        twin.markdown
    );
    assert!(
        twin.markdown.contains("id") && twin.markdown.contains("verbose"),
        "every parameter is a row in it:\n{}",
        twin.markdown
    );
    assert!(
        twin.markdown.contains("/widgets/{id}"),
        "and the path is a token the search index reads:\n{}",
        twin.markdown
    );
}

/// The other serializer, for contrast, so the difference is recorded rather
/// than rediscovered: it is HTML-free and keeps the prose, and it is not the
/// twin.
#[test]
fn the_page_renders_markdown_that_is_not_the_twin() {
    let page = rendered(&body("GET /widgets/{id}"));
    for tag in ["<div", "<span", "<table"] {
        assert!(!page.markdown.contains(tag), "{}", page.markdown);
    }
    assert!(
        page.markdown.contains("The widget's id."),
        "a parameter's own prose survives:\n{}",
        page.markdown
    );
}
