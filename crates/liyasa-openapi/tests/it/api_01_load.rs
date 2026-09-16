//! API-01: 3.0 and 3.1, JSON and YAML, reach one model; Swagger 2.0 is
//! converted and says so.

use liyasa_core::diagnostics::code;
use liyasa_openapi::load;
use liyasa_openapi::model::{Method, SchemaType};

const YAML_3_1: &str = r##"
openapi: 3.1.0
info:
  title: Widgets
  version: "1.0.0"
servers:
  - url: https://api.example.com/v1
paths:
  /widgets/{id}:
    get:
      operationId: getWidget
      summary: Fetch one widget
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: string }
        - name: verbose
          in: query
          schema: { type: [boolean, "null"] }
      responses:
        "200":
          description: The widget
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Widget" }
components:
  schemas:
    Widget:
      type: object
      required: [id]
      properties:
        id: { type: string }
        size:
          type: [integer, "null"]
          exclusiveMinimum: 0
          examples: [3]
"##;

const YAML_3_0: &str = r##"
openapi: 3.0.3
info:
  title: Widgets
  version: "1.0.0"
servers:
  - url: https://api.example.com/v1
paths:
  /widgets/{id}:
    get:
      operationId: getWidget
      summary: Fetch one widget
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: string }
        - name: verbose
          in: query
          schema: { type: boolean, nullable: true }
      responses:
        "200":
          description: The widget
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Widget" }
components:
  schemas:
    Widget:
      type: object
      required: [id]
      properties:
        id: { type: string }
        size:
          type: integer
          nullable: true
          minimum: 0
          exclusiveMinimum: true
          example: 3
"##;

const JSON_3_1: &str = r##"
{
  "openapi": "3.1.0",
  "info": { "title": "Widgets", "version": "1.0.0" },
  "servers": [{ "url": "https://api.example.com/v1" }],
  "paths": {
    "/widgets/{id}": {
      "get": {
        "operationId": "getWidget",
        "summary": "Fetch one widget",
        "parameters": [
          { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } },
          { "name": "verbose", "in": "query", "schema": { "type": ["boolean", "null"] } }
        ],
        "responses": {
          "200": {
            "description": "The widget",
            "content": {
              "application/json": { "schema": { "$ref": "#/components/schemas/Widget" } }
            }
          }
        }
      }
    }
  },
  "components": {
    "schemas": {
      "Widget": {
        "type": "object",
        "required": ["id"],
        "properties": {
          "id": { "type": "string" },
          "size": { "type": ["integer", "null"], "exclusiveMinimum": 0, "examples": [3] }
        }
      }
    }
  }
}
"##;

/// The model without the dialect it came from, which is the only field two
/// equivalent documents are allowed to differ in.
fn shape(source: &str, origin: &str) -> String {
    let loaded = load::from_bytes("api", origin, source.as_bytes()).expect("the spec loads");
    assert!(
        !loaded.diagnostics.has_errors(),
        "{origin}: {:?}",
        loaded.diagnostics.as_slice()
    );
    let mut spec = loaded.spec;
    spec.version = liyasa_openapi::SpecVersion::V3_1("3.1.0".to_owned());
    serde_json::to_string_pretty(&spec).expect("the model serializes")
}

#[test]
fn three_zero_and_three_one_reach_the_same_model() {
    assert_eq!(shape(YAML_3_0, "3.0.yaml"), shape(YAML_3_1, "3.1.yaml"));
}

#[test]
fn json_and_yaml_reach_the_same_model() {
    assert_eq!(shape(JSON_3_1, "3.1.json"), shape(YAML_3_1, "3.1.yaml"));
}

#[test]
fn the_three_zero_keywords_are_gone_by_the_time_the_model_is_read() {
    let loaded = load::from_bytes("api", "3.0.yaml", YAML_3_0.as_bytes()).expect("loads");
    let widget = loaded
        .spec
        .components
        .schemas
        .get("Widget")
        .expect("the component reads");
    let size = widget.properties.get("size").expect("the property reads");

    assert!(size.is_nullable(), "`nullable: true` became the null type");
    assert_eq!(
        size.shown_types().collect::<Vec<_>>(),
        vec![SchemaType::Integer],
        "null is not shown as a type of its own"
    );
    assert_eq!(size.minimum, None, "the inclusive bound was consumed");
    assert_eq!(
        size.exclusive_minimum.as_ref().and_then(|n| n.as_u64()),
        Some(0)
    );
    assert_eq!(
        size.examples.len(),
        1,
        "the singular example became the list"
    );
}

#[test]
fn swagger_two_is_converted_and_warns() {
    let loaded = load::from_bytes(
        "api",
        "swagger.yaml",
        br##"
swagger: "2.0"
info: { title: Widgets, version: "1.0.0" }
host: api.example.com
basePath: /v1
schemes: [https]
paths:
  /widgets:
    post:
      operationId: createWidget
      parameters:
        - { name: body, in: body, required: true, schema: { $ref: "#/definitions/Widget" } }
      responses:
        "201": { description: Made, schema: { $ref: "#/definitions/Widget" } }
definitions:
  Widget:
    type: object
    properties:
      id: { type: string }
"##,
    )
    .expect("a 2.0 document loads");

    assert!(
        !loaded.diagnostics.has_errors(),
        "{:?}",
        loaded.diagnostics.as_slice()
    );
    let warned: Vec<_> = loaded
        .diagnostics
        .iter()
        .filter(|d| d.code == code::W0509)
        .collect();
    assert_eq!(warned.len(), 1, "the conversion announces itself once");

    let spec = &loaded.spec;
    assert_eq!(spec.servers.len(), 1);
    assert_eq!(spec.servers[0].url, "https://api.example.com/v1");

    let operation = spec
        .operation(Method::Post, "/widgets")
        .expect("the operation converted");
    let body = operation
        .operation
        .request_body
        .as_ref()
        .expect("the body parameter became a request body");
    assert!(body.required);
    let (media, content) = body.preferred().expect("the body has content");
    assert_eq!(media, "application/json");
    assert_eq!(
        content.schema.as_ref().and_then(|s| s.name.as_deref()),
        Some("Widget"),
        "`#/definitions/Widget` was retargeted under components"
    );
}

#[test]
fn a_later_version_is_refused_rather_than_read_as_three_one() {
    let error = load::from_bytes("api", "api.yaml", b"openapi: 4.0.0\ninfo: {}\n")
        .expect_err("4.0 is not supported");
    assert_eq!(error.code, code::E0504);
}

/// A YAML author who leaves a status code unquoted has written an integer
/// key; real specs do it constantly. RFC 0803 reads it as the string OpenAPI
/// meant rather than refusing the document.
#[test]
fn an_unquoted_status_code_is_read_as_the_string_it_means() {
    const UNQUOTED: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
paths:
  /widgets:
    get:
      operationId: listWidgets
      responses:
        200:
          description: ok
        404:
          description: gone
        default:
          description: otherwise
"##;
    let loaded = load::from_bytes("api", "api.yaml", UNQUOTED.as_bytes()).expect("loads");
    assert!(
        !loaded.diagnostics.has_errors(),
        "{:?}",
        loaded.diagnostics.as_slice()
    );
    let operation = loaded
        .spec
        .by_operation_id("listWidgets")
        .expect("the operation survived");
    assert_eq!(
        operation
            .operation
            .responses
            .iter()
            .map(|(status, _)| status)
            .collect::<Vec<_>>(),
        vec!["200", "404", "default"]
    );
}

#[test]
fn a_mapping_used_as_a_key_is_still_refused() {
    const ODD: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
paths:
  /widgets:
    get:
      operationId: listWidgets
      responses:
        ? { a: 1 }
        : { description: nonsense }
        "200": { description: ok }
"##;
    let loaded = load::from_bytes("api", "api.yaml", ODD.as_bytes()).expect("parses");
    assert!(
        loaded.diagnostics.has_errors(),
        "a key with no reading at all is not coerced"
    );
    let operation = loaded
        .spec
        .by_operation_id("listWidgets")
        .expect("the rest of the operation is kept");
    assert_eq!(operation.operation.responses.len(), 1);
}

/// OpenAI's published spec bounds a `seed` with an integer below `i64::MIN`,
/// which the document tree cannot hold. RFC 0804 keeps the rest of the
/// document rather than losing 3.4 MB of it to one rounded bound.
#[test]
fn an_integer_too_wide_for_the_tree_does_not_lose_the_document() {
    const WIDE: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
paths:
  /widgets:
    get:
      operationId: listWidgets
      parameters:
        - name: seed
          in: query
          schema:
            type: integer
            minimum: -9223372036854776000
            maximum: 9223372036854776000
      responses: { "200": { description: ok } }
"##;
    let loaded = load::from_bytes("api", "api.yaml", WIDE.as_bytes())
        .expect("the document survives the bound");
    assert!(
        !loaded.diagnostics.has_errors(),
        "{:?}",
        loaded.diagnostics.as_slice()
    );
    let operation = loaded
        .spec
        .by_operation_id("listWidgets")
        .expect("the operation is still there");
    let seed = operation
        .parameters()
        .into_iter()
        .find(|parameter| parameter.name == "seed")
        .expect("so is the parameter");
    let schema = seed.schema.as_ref().expect("and its schema");
    let minimum = schema
        .minimum
        .as_ref()
        .and_then(serde_norway::Number::as_f64)
        .expect("the bound is kept as the nearest float");
    assert!(minimum < -9.0e18, "{minimum}");
}

#[test]
fn an_ordinary_document_is_unchanged_by_the_second_pass() {
    let strict = load::from_bytes("api", "3.1.yaml", YAML_3_1.as_bytes()).expect("loads");
    assert!(!strict.diagnostics.has_errors());
    assert!(
        strict.spec.by_operation_id("getWidget").is_some(),
        "the first pass still reads what it always read"
    );
}

/// Draft-04 spelled tuple validation `items: [A, B]`; 2020-12 calls it
/// `prefixItems`. Slack's published document uses the old form.
#[test]
fn draft_four_tuple_items_become_prefix_items() {
    const TUPLE: &str = r##"
openapi: 3.0.3
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
              schema:
                type: object
                properties:
                  pair:
                    type: array
                    items:
                      - { type: string, nullable: true }
                      - { type: integer }
"##;
    let loaded = load::from_bytes("api", "api.yaml", TUPLE.as_bytes()).expect("loads");
    assert!(
        !loaded.diagnostics.has_errors(),
        "{:?}",
        loaded.diagnostics.as_slice()
    );
    let operation = loaded
        .spec
        .by_operation_id("listWidgets")
        .expect("the operation is there");
    let (_, media) = operation
        .operation
        .responses
        .values()
        .next()
        .and_then(|response| response.preferred())
        .expect("the response has a body");
    let pair = media
        .schema
        .as_ref()
        .and_then(|schema| schema.properties.get("pair"))
        .expect("the property survived");

    assert!(pair.items.is_none(), "the array form is not `items`");
    assert_eq!(pair.prefix_items.len(), 2);
    assert!(
        pair.prefix_items[0].is(SchemaType::String) && pair.prefix_items[0].is_nullable(),
        "and the members are still normalized in turn"
    );
    assert!(pair.prefix_items[1].is(SchemaType::Integer));
}

/// Generated specs emit `summary: ""` for every operation they have nothing to
/// say about. Twilio's does, and an empty title is worse than the selector.
#[test]
fn a_key_written_but_left_empty_is_read_as_not_written() {
    const BLANK: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
paths:
  /widgets:
    get:
      operationId: listWidgets
      summary: ""
      description: "   "
      responses: { "200": { description: ok } }
"##;
    let loaded = load::from_bytes("api", "api.yaml", BLANK.as_bytes()).expect("loads");
    assert!(
        !loaded.diagnostics.has_errors(),
        "{:?}",
        loaded.diagnostics.as_slice()
    );
    let operation = loaded
        .spec
        .by_operation_id("listWidgets")
        .expect("the operation is there");
    assert_eq!(operation.operation.summary, None);
    assert_eq!(operation.operation.description, None);
}
