//! API-14: the Markdown representation carries a parameter table and the
//! examples, holds no HTML, and is what search reads.

use super::support;

use liyasa_openapi::markdown;

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
servers:
  - url: https://api.example.com/v1
paths:
  /widgets:
    get:
      operationId: listWidgets
      summary: List widgets
      description: Widgets, newest first.
      tags: [Widgets]
      parameters:
        - { name: limit, in: query, schema: { type: integer, minimum: 1, maximum: 100, default: 20 }, description: How many }
        - { name: cursor, in: query, schema: { type: string } }
      responses:
        "200":
          description: A page of widgets
          content:
            application/json:
              schema:
                type: object
                required: [items]
                properties:
                  items:
                    type: array
                    items:
                      type: object
                      properties:
                        id: { type: string, examples: ["w_1"] }
                  next: { type: [string, "null"] }
"##;

fn rendered(options: &markdown::Options) -> String {
    let spec = support::spec(SPEC);
    let page = support::page(&spec, "listWidgets");
    markdown::render(&page, options)
}

#[test]
fn it_has_a_parameter_table_and_examples_and_no_html() {
    let out = rendered(&markdown::Options::default());
    assert!(out.contains("## Query parameters"), "{out}");
    assert!(
        out.contains("| Name | Type | Required | Description |"),
        "{out}"
    );
    assert!(out.contains("| `limit` | integer |"), "{out}");
    assert!(out.contains("## Response samples"), "{out}");
    assert!(out.contains("```json"), "{out}");
    assert!(
        !out.contains('<'),
        "no HTML in the Markdown representation: {out}"
    );
    assert!(!out.contains("&lt;"), "and nothing HTML-escaped either");
}

#[test]
fn the_full_json_schema_is_optional() {
    assert!(!rendered(&markdown::Options::default()).contains("Schema"));
    let with = rendered(&markdown::Options {
        include_schema: true,
        ..markdown::Options::default()
    });
    assert!(with.contains("Schema"), "{with}");
}

#[test]
fn search_text_is_tokenized_by_method_and_path() {
    let spec = support::spec(SPEC);
    let page = support::page(&spec, "listWidgets");
    let text = markdown::search_text(&page);
    assert!(text.contains("GET"), "{text}");
    assert!(text.contains("/widgets"), "{text}");
    assert!(text.contains("List widgets"), "{text}");
    assert!(text.contains("cursor"), "a parameter is searchable: {text}");
    assert_eq!(markdown::tokens(&page).1, "/widgets");
}

#[test]
fn the_rendered_page_matches_its_golden() {
    support::golden("api_14", &rendered(&markdown::Options::default()));
}
