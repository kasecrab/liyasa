//! API-02: `$ref` resolves, and one that does not is `E0502` with the pointer
//! of the reference rather than of wherever the failure was noticed.

use liyasa_core::diagnostics::code;
use liyasa_openapi::model::{Method, SchemaType};
use liyasa_openapi::read;
use liyasa_openapi::tree;
use liyasa_openapi::version::SpecVersion;

fn load(source: &str) -> (liyasa_openapi::Spec, liyasa_core::Diagnostics) {
    let root = tree::parse(source.as_bytes(), "api.yaml").expect("the fixture parses");
    read::local(root, "api", SpecVersion::V3_1("3.1.0".to_owned()))
}

const LOCAL: &str = r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths:
  /users/{id}:
    get:
      operationId: getUser
      parameters:
        - $ref: "#/components/parameters/Id"
      responses:
        "200":
          description: One user
          content:
            application/json:
              schema: { $ref: "#/components/schemas/User" }
components:
  parameters:
    Id:
      name: id
      in: path
      required: true
      schema: { type: string }
  schemas:
    User:
      type: object
      properties:
        id: { type: string }
        manager: { $ref: "#/components/schemas/User" }
"##;

#[test]
fn a_local_reference_resolves_to_what_it_names() {
    let (spec, diagnostics) = load(LOCAL);
    assert!(!diagnostics.has_errors(), "{:?}", diagnostics.as_slice());

    let operation = spec
        .operation(Method::Get, "/users/{id}")
        .expect("the operation is there");
    let parameters = operation.parameters();
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0].name, "id");
    assert!(
        parameters[0]
            .schema
            .as_ref()
            .is_some_and(|s| s.is(SchemaType::String)),
        "the referenced parameter brought its schema"
    );
}

#[test]
fn a_referenced_schema_keeps_the_component_name_it_was_reached_by() {
    let (spec, _) = load(LOCAL);
    let operation = spec
        .operation(Method::Get, "/users/{id}")
        .expect("the operation is there");
    let response = operation
        .operation
        .responses
        .get("200")
        .expect("a 200 response");
    let (_, media) = response.preferred().expect("a response body");
    let schema = media.schema.as_ref().expect("the body has a schema");
    assert_eq!(schema.name.as_deref(), Some("User"));
}

#[test]
fn a_recursive_schema_stops_at_a_named_stub_rather_than_expanding_forever() {
    let (spec, diagnostics) = load(LOCAL);
    assert!(!diagnostics.has_errors(), "recursion is not an error");

    let mut schema = spec
        .components
        .schemas
        .get("User")
        .expect("the component is read")
        .clone();
    let mut depth = 0;
    while let Some(manager) = schema.properties.get("manager") {
        schema = manager.clone();
        depth += 1;
        assert!(depth < 100, "the expansion did not terminate");
        if schema.properties.is_empty() {
            break;
        }
    }
    assert!(depth > 0, "the reference was followed at least once");
    assert_eq!(
        schema.name.as_deref(),
        Some("User"),
        "the stub still says which schema to expand"
    );
}

#[test]
fn a_broken_reference_is_e0502_and_names_the_pointer() {
    let (_, diagnostics) = load(
        r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths:
  /a:
    get:
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Missing" }
components:
  schemas: {}
"##,
    );
    let broken: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == code::E0502)
        .collect();
    assert_eq!(broken.len(), 1, "{:?}", diagnostics.as_slice());
    assert!(
        broken[0]
            .message
            .contains("/paths/~1a/get/responses/200/content/application~1json/schema"),
        "the pointer is of the reference: {}",
        broken[0].message
    );
    assert!(broken[0].message.contains("#/components/schemas/Missing"));
}

#[test]
fn a_reference_into_a_document_that_was_not_loaded_is_e0502() {
    let (_, diagnostics) = load(
        r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths: {}
components:
  schemas:
    User: { $ref: "common.yaml#/User" }
"##,
    );
    let out: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == code::E0502)
        .collect();
    assert_eq!(out.len(), 1, "{:?}", diagnostics.as_slice());
    assert!(
        out[0].message.contains("common.yaml#/User"),
        "{}",
        out[0].message
    );
}

#[test]
fn a_type_error_in_a_spec_is_e0501_with_the_pointer_and_the_rest_still_reads() {
    let (spec, diagnostics) = load(
        r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths:
  /a:
    get:
      summary: [not, a, string]
      operationId: listA
      responses:
        "200": { description: ok }
"##,
    );
    let out: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == code::E0501)
        .collect();
    assert_eq!(out.len(), 1, "{:?}", diagnostics.as_slice());
    assert!(
        out[0].message.contains("/paths/~1a/get/summary"),
        "{}",
        out[0].message
    );
    assert!(
        spec.by_operation_id("listA").is_some(),
        "one bad field does not lose the operation"
    );
}
