//! API-13: the controls above a long table — a required-only quick filter, a
//! search within the schema's fields, and the JSON schema of any object row.

use liyasa_openapi::field::Field;
use liyasa_openapi::pills;

use super::support;

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
paths:
  /widgets:
    post:
      operationId: createWidget
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [shipping]
              properties:
                nickname:
                  type: string
                  description: What the team calls it
                colour:
                  type: string
                  enum: [vermilion, cerulean]
                shipping:
                  type: object
                  required: [postcode]
                  properties:
                    postcode: { type: string, minLength: 4 }
                    instructions: { type: string }
      responses: { "201": { description: made } }
"##;

fn body_fields() -> Vec<Field> {
    let spec = support::spec(SPEC);
    let page = support::page(&spec, "createWidget");
    page.body
        .expect("the operation has a body")
        .media_types
        .into_iter()
        .next()
        .expect("the body has a media type")
        .fields
}

fn names(fields: &[Field]) -> Vec<String> {
    fields.iter().map(|field| field.name.clone()).collect()
}

#[test]
fn the_required_filter_keeps_a_row_whose_descendant_is_required() {
    let fields = body_fields();
    assert_eq!(names(&fields), vec!["nickname", "colour", "shipping"]);

    let filtered = pills::required_only(&fields);
    assert_eq!(
        names(&filtered),
        vec!["shipping"],
        "the two optional scalars go"
    );
    assert_eq!(
        names(&filtered[0].children),
        vec!["postcode"],
        "and the filter reaches inside the object it kept"
    );
}

#[test]
fn a_row_survives_the_filter_only_to_carry_a_required_child() {
    let spec = support::spec(
        r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
paths:
  /widgets:
    post:
      operationId: createWidget
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                shipping:
                  type: object
                  required: [postcode]
                  properties:
                    postcode: { type: string }
      responses: { "201": { description: made } }
"##,
    );
    let page = support::page(&spec, "createWidget");
    let fields = page
        .body
        .expect("there is a body")
        .media_types
        .into_iter()
        .next()
        .expect("there is a media type")
        .fields;

    assert!(!fields[0].required, "`shipping` itself is optional");
    let filtered = pills::required_only(&fields);
    assert_eq!(
        names(&filtered),
        vec!["shipping"],
        "but hiding it would hide the required field under it"
    );
}

#[test]
fn the_search_matches_a_name_a_description_or_an_enum_value() {
    let fields = body_fields();

    assert_eq!(names(&pills::search(&fields, "nick")), vec!["nickname"]);
    assert_eq!(
        names(&pills::search(&fields, "what the team")),
        vec!["nickname"],
        "the description is part of the haystack"
    );
    assert_eq!(
        names(&pills::search(&fields, "CERULEAN")),
        vec!["colour"],
        "and so are the enum values, case-insensitively"
    );
    assert!(pills::search(&fields, "nothing here").is_empty());
}

#[test]
fn a_hit_on_a_nested_field_keeps_the_path_down_to_it() {
    let fields = body_fields();
    let hit = pills::search(&fields, "postcode");
    assert_eq!(names(&hit), vec!["shipping"]);
    assert_eq!(
        names(&hit[0].children),
        vec!["postcode"],
        "the parent is kept so the reader can see where the hit lives"
    );
}

#[test]
fn a_search_box_is_offered_only_where_the_table_is_long() {
    assert!(!pills::wants_search(&body_fields()));

    let mut properties = String::new();
    for index in 0..pills::SEARCH_THRESHOLD + 1 {
        properties.push_str(&format!(
            "                field{index}: {{ type: string }}\n"
        ));
    }
    let spec = support::spec(&format!(
        r##"
openapi: 3.1.0
info: {{ title: Widgets, version: "1" }}
paths:
  /widgets:
    post:
      operationId: createWidget
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
{properties}
      responses: {{ "201": {{ description: made }} }}
"##
    ));
    let page = support::page(&spec, "createWidget");
    let fields = page
        .body
        .expect("there is a body")
        .media_types
        .into_iter()
        .next()
        .expect("there is a media type")
        .fields;
    assert!(pills::wants_search(&fields));
}

#[test]
fn an_object_row_carries_the_json_schema_a_reader_can_copy() {
    let fields = body_fields();
    let shipping = fields
        .iter()
        .find(|field| field.name == "shipping")
        .expect("the object row is there");

    let copied: serde_json::Value = serde_json::from_str(
        shipping
            .json_schema
            .as_deref()
            .expect("an object row offers its schema"),
    )
    .expect("what is copied is JSON");

    assert_eq!(
        copied["$schema"], "https://json-schema.org/draft/2020-12/schema",
        "so it can be pasted into a validator as it stands"
    );
    assert_eq!(copied["type"], "object", "`type`, not the model's `types`");
    assert_eq!(copied["required"], serde_json::json!(["postcode"]));
    assert_eq!(copied["properties"]["postcode"]["minLength"], 4);
    assert!(
        copied["properties"]["postcode"].get("$schema").is_none(),
        "only the root is annotated"
    );

    assert!(
        fields
            .iter()
            .find(|field| field.name == "nickname")
            .is_some_and(|field| field.json_schema.is_none()),
        "a scalar row has nothing worth copying"
    );
}

#[test]
fn the_copied_schema_spells_json_schema_keywords_not_the_models_field_names() {
    let spec = support::spec(
        r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
paths:
  /widgets:
    post:
      operationId: createWidget
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                kind:
                  type: object
                  properties:
                    tag: { const: widget }
                    colour: { type: string, enum: [red, blue] }
                    nested: { type: [string, "null"] }
                  additionalProperties: false
                  x-liyasa: { title: Kind }
      responses: { "201": { description: made } }
"##,
    );
    let page = support::page(&spec, "createWidget");
    let fields = page
        .body
        .expect("there is a body")
        .media_types
        .into_iter()
        .next()
        .expect("there is a media type")
        .fields;

    let copied: serde_json::Value =
        serde_json::from_str(fields[0].json_schema.as_deref().expect("it is an object"))
            .expect("what is copied is JSON");

    assert_eq!(copied["properties"]["tag"]["const"], "widget");
    assert_eq!(
        copied["properties"]["colour"]["enum"],
        serde_json::json!(["red", "blue"])
    );
    assert_eq!(
        copied["properties"]["nested"]["type"],
        serde_json::json!(["string", "null"]),
        "a union of types stays a list"
    );
    assert_eq!(copied["additionalProperties"], serde_json::json!(false));
    for absent in ["types", "enumeration", "constant", "rest", "name"] {
        assert!(
            copied.get(absent).is_none(),
            "`{absent}` is a name in the model, not a JSON Schema keyword"
        );
    }
}
