//! AST-03: an operation indexed as a structured document, through the real
//! loader and the real endpoint-page renderer.

use liyasa_ai::chunk::{ChunkOptions, PageContext};
use liyasa_ai::index::ChunkKind;
use liyasa_ai::openapi_doc::{self, OperationFacts};
use liyasa_core::ids::{Locale, Route};
use liyasa_openapi::codegen::Registry;
use liyasa_openapi::page::{BuildOptions, Page};
use liyasa_openapi::{Spec, load};

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
servers:
  - url: https://api.example.com/v1
paths:
  /widgets/{widgetId}:
    get:
      operationId: getWidget
      summary: Fetch one widget
      description: Returns a single widget by its identifier.
      parameters:
        - { name: widgetId, in: path, required: true, schema: { type: string }, description: The widget }
        - { name: include, in: query, schema: { type: string }, description: Related records to embed }
      responses:
        "200":
          description: The widget
          content:
            application/json:
              schema:
                type: object
                required: [id, expiresAt]
                properties:
                  id: { type: string }
                  expiresAt: { type: string, format: date-time }
        "404": { description: No such widget }
"##;

fn page() -> Page {
    let loaded = load::from_bytes("api", "api.yaml", SPEC.as_bytes()).expect("the spec loads");
    assert!(
        !loaded.diagnostics.has_errors(),
        "{:?}",
        loaded.diagnostics.as_slice()
    );
    let spec: Spec = loaded.spec;
    let operation = spec
        .by_operation_id("getWidget")
        .expect("the spec has getWidget");
    Page::build(
        &spec,
        &operation,
        &Registry::new(),
        &BuildOptions {
            route: "/api-reference/get-widget".to_owned(),
            ..BuildOptions::default()
        },
    )
}

fn context() -> PageContext {
    PageContext {
        route: Route::new("/api-reference/get-widget"),
        title: "Fetch one widget".to_owned(),
        breadcrumb: vec!["API reference".to_owned()],
        version: None,
        locale: Locale::new("en"),
        groups: Vec::new(),
        regions: Vec::new(),
        product: Some("widgets".to_owned()),
        last_verified: None,
    }
}

#[test]
fn the_method_and_path_are_in_every_chunk() {
    let page = page();
    for chunk in openapi_doc::chunks(&page, &ChunkOptions::default()) {
        assert!(
            chunk.text.contains("`GET /widgets/{widgetId}`"),
            "a fragment must say which operation it is:\n{}",
            chunk.text
        );
    }
}

#[test]
fn the_exact_field_names_reach_the_index() {
    let page = page();
    let text: String = openapi_doc::chunks(&page, &ChunkOptions::default())
        .iter()
        .map(|c| c.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    // The spelling an answer must cite, not a prose paraphrase of it.
    assert!(text.contains("expiresAt"), "{text}");
    assert!(text.contains("widgetId"), "{text}");
    assert!(text.contains("include"), "{text}");
    assert!(text.contains("404"), "{text}");
}

#[test]
fn the_facts_are_read_from_the_page_not_restated() {
    let facts = OperationFacts::of(&page());
    assert_eq!(facts.method, "GET");
    assert_eq!(facts.path, "/widgets/{widgetId}");
    assert!(
        facts.parameters.contains(&"widgetId".to_owned()),
        "{facts:?}"
    );
    assert!(
        facts.parameters.contains(&"include".to_owned()),
        "{facts:?}"
    );
    assert_eq!(facts.responses, ["200", "404"]);
}

#[test]
fn an_operation_is_recorded_as_an_operation_not_as_prose() {
    let records = openapi_doc::records(&page(), &context(), &ChunkOptions::default());
    assert!(!records.is_empty());
    for record in &records {
        assert_eq!(record.kind, ChunkKind::Operation);
        assert_eq!(record.route.as_str(), "/api-reference/get-widget");
        assert_eq!(record.product.as_deref(), Some("widgets"));
        assert_eq!(record.citation(), "/api-reference/get-widget");
    }
}

#[test]
fn a_long_operation_splits_at_its_own_sections() {
    let page = page();
    // A ceiling small enough to force the split, so the rule is exercised on a
    // spec that is otherwise short.
    let options = ChunkOptions {
        max_tokens: 120,
        min_tokens: 30,
        overlap_tokens: 10,
    };
    let chunks = openapi_doc::chunks(&page, &options);
    assert!(chunks.len() > 1, "{} chunks", chunks.len());
    for chunk in &chunks {
        assert!(chunk.text.contains("`GET /widgets/{widgetId}`"));
    }
    // Each piece begins at one of the renderer's own headings.
    let starts: Vec<&str> = chunks
        .iter()
        .filter_map(|c| c.text.lines().find(|l| l.starts_with("## ")))
        .collect();
    assert!(starts.len() > 1, "{starts:?}");
}

#[test]
fn the_ids_of_two_chunks_of_one_operation_differ() {
    let options = ChunkOptions {
        max_tokens: 120,
        min_tokens: 30,
        overlap_tokens: 10,
    };
    let records = openapi_doc::records(&page(), &context(), &options);
    let mut ids: Vec<String> = records.iter().map(|r| r.id.as_str().to_owned()).collect();
    let before = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), before, "two chunks share a row id: {ids:?}");
}
