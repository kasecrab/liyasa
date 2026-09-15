//! API-05: `x-liyasa` on an operation applies its title, its hidden flag, and
//! its examples; another vendor's namespace is carried through the model and
//! interpreted by nothing at build time.

use liyasa_openapi::config::{Display, SpecConfig};
use liyasa_openapi::model::ext::{NAMESPACE, XLiyasa};
use liyasa_openapi::nav::{self, Node};

#[path = "support.rs"]
mod support;

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
servers:
  - url: https://api.example.com
paths:
  /widgets:
    post:
      operationId: createWidget
      summary: The summary the spec wrote
      tags: [Widgets]
      x-liyasa:
        title: The title the operator wrote
        examples:
          - { name: "curated", size: 12 }
      x-mint:
        title: Another product's title
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                name: { type: string }
                size: { type: integer }
      responses: { "201": { description: made } }
  /widgets/{id}:
    delete:
      operationId: deleteWidget
      summary: Remove a widget
      tags: [Widgets]
      x-liyasa: { hidden: true }
      parameters:
        - { name: id, in: path, required: true, schema: { type: string } }
      responses: { "204": { description: gone } }
"##;

fn config() -> SpecConfig {
    SpecConfig::parse(&serde_json::json!({ "id": "api", "source": "openapi/api.yaml" }))
        .expect("the config reads")
}

#[test]
fn the_title_hint_wins_over_the_summary_on_the_page_and_in_navigation() {
    let spec = support::spec(SPEC);
    assert_eq!(
        support::page(&spec, "createWidget").title,
        "The title the operator wrote"
    );

    let (reference, diagnostics) = nav::generate(
        &spec,
        &config(),
        &Node {
            openapi: "api".to_owned(),
            ..Node::default()
        },
    );
    assert!(!diagnostics.has_errors(), "{:?}", diagnostics.as_slice());
    assert_eq!(
        reference
            .entries()
            .map(|entry| entry.title.as_str())
            .collect::<Vec<_>>(),
        vec!["The title the operator wrote"],
        "and the hidden operation is not in navigation at all"
    );
}

#[test]
fn a_hidden_operation_leaves_navigation_and_offers_no_playground() {
    let spec = support::spec(SPEC);
    let page = support::page(&spec, "deleteWidget");
    assert_eq!(page.rail.display, Display::None);

    let (reference, _) = nav::generate(
        &spec,
        &config(),
        &Node {
            openapi: "api".to_owned(),
            ..Node::default()
        },
    );
    assert!(
        !reference
            .entries()
            .any(|entry| entry.selector == "DELETE /widgets/{id}"),
        "a hidden operation is not a navigation entry"
    );
}

#[test]
fn the_examples_hint_replaces_the_body_derived_from_the_schema() {
    let spec = support::spec(SPEC);
    let page = support::page(&spec, "createWidget");

    let example = page.rail.request_example.as_ref().expect("there is a body");
    assert_eq!(example.media_type, "application/json");
    let value: serde_json::Value =
        serde_json::from_str(&example.text).expect("the example is the JSON the operator wrote");
    assert_eq!(value, serde_json::json!({ "name": "curated", "size": 12 }));

    let curl = page
        .rail
        .samples
        .iter()
        .find(|sample| sample.language == "curl")
        .expect("curl is a default language");
    assert!(
        curl.source.contains("curated"),
        "the sample sends what the page shows:\n{}",
        curl.source
    );
}

#[test]
fn another_vendors_namespace_is_carried_through_and_read_by_nothing() {
    const MINT_ONLY: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
paths:
  /widgets:
    get:
      operationId: listWidgets
      summary: The summary the spec wrote
      x-mint: { title: Another product's title, hidden: true }
      responses: { "200": { description: ok } }
"##;
    let spec = support::spec(MINT_ONLY);
    let operation = spec
        .by_operation_id("listWidgets")
        .expect("the spec has it")
        .operation;

    assert!(
        operation.extensions.get("x-mint").is_some(),
        "the extension survives into the model so the importer can rewrite it"
    );
    assert!(
        XLiyasa::read(&operation.extensions).is_empty(),
        "x-mint means nothing to the build; the importer rewrites it once (§28.1)"
    );
    assert_eq!(
        support::page(&spec, "listWidgets").title,
        "The summary the spec wrote",
        "another product's title does not become Liyasa's"
    );

    let both = support::spec(SPEC);
    let operation = both
        .by_operation_id("createWidget")
        .expect("the spec has it")
        .operation;
    assert!(operation.extensions.get("x-mint").is_some());
    assert!(operation.extensions.get(NAMESPACE).is_some());
}
