//! API-11: choices are selectable variants with discriminator labels,
//! recursion is depth-limited with something to expand, and 3.1's
//! `type: [string, "null"]` reads as nullability.

use super::support;

use liyasa_openapi::field::Field;
use liyasa_openapi::markdown;

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Zoo, version: "1" }
paths:
  /pets:
    post:
      operationId: addPet
      requestBody:
        required: true
        content:
          application/json:
            schema: { $ref: "#/components/schemas/Pet" }
      responses:
        "201":
          description: made
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Node" }
components:
  schemas:
    Pet:
      oneOf:
        - $ref: "#/components/schemas/Cat"
        - $ref: "#/components/schemas/Dog"
      discriminator:
        propertyName: kind
        mapping:
          cat: "#/components/schemas/Cat"
          dog: "#/components/schemas/Dog"
    Cat:
      type: object
      required: [kind]
      properties:
        kind: { type: string, const: cat }
        livesLeft: { type: [integer, "null"] }
    Dog:
      type: object
      required: [kind]
      properties:
        kind: { type: string, const: dog }
        goodBoy: { type: boolean }
    Node:
      type: object
      properties:
        label: { type: [string, "null"] }
        children:
          type: array
          items: { $ref: "#/components/schemas/Node" }
"##;

fn body_field() -> Field {
    let spec = support::spec(SPEC);
    let page = support::page(&spec, "addPet");
    let body = page.body.expect("there is a body");
    body.media_types[0].fields[0].clone()
}

#[test]
fn a_one_of_becomes_variants_labelled_by_the_discriminator() {
    let field = body_field();
    assert_eq!(field.type_label, "one of");
    assert_eq!(
        field
            .variants
            .iter()
            .map(|v| v.label.as_str())
            .collect::<Vec<_>>(),
        vec!["cat", "dog"]
    );
    let cat = &field.variants[0].field;
    assert_eq!(
        cat.children
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["kind", "livesLeft"]
    );
}

#[test]
fn a_three_one_type_array_reads_as_nullability_not_as_a_union() {
    let field = body_field();
    let lives = field.variants[0]
        .field
        .children
        .iter()
        .find(|c| c.name == "livesLeft")
        .expect("the property is there");
    assert_eq!(lives.type_label, "integer");
    assert!(lives.nullable);
}

#[test]
fn a_recursive_schema_is_depth_limited_and_says_what_to_expand() {
    let spec = support::spec(SPEC);
    let page = support::page(&spec, "addPet");
    let response = page
        .responses
        .iter()
        .find(|r| r.status == "201")
        .expect("the 201 is there");
    let root = &response.media_types[0].fields;
    let names: Vec<&str> = root.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["label", "children"]);

    let deepest = root
        .iter()
        .flat_map(Field::flatten)
        .find(|field| field.truncated)
        .expect("the expansion stopped somewhere");
    assert_eq!(
        deepest.schema_name.as_deref(),
        Some("Node"),
        "the stub names the schema an expand control would fetch"
    );
}

#[test]
fn the_rendered_page_matches_its_golden() {
    let spec = support::spec(SPEC);
    let page = support::page(&spec, "addPet");
    support::golden(
        "api_11",
        &markdown::render(&page, &markdown::Options::default()),
    );
}
