//! OpenAPI 3.0 into the 3.1 model (API-01).
//!
//! Three keywords changed meaning between 3.0's JSON Schema draft and 3.1's
//! 2020-12: `nullable`, the boolean form of `exclusiveMinimum` and
//! `exclusiveMaximum`, and the singular `example`. Rewriting them here rather
//! than teaching every consumer both spellings is the whole point of API-01's
//! "one shape".
//!
//! The walk is structural rather than a blind search for schema-shaped
//! objects: `properties` may hold a property called `nullable`, and a
//! `nullable` property is not a nullable schema.

use crate::tree::{Map, Value, as_map_mut, as_str};

/// Rewrites a parsed 3.0 document in place so it reads as 3.1.
pub fn normalize(root: &mut Value) {
    if let Some(map) = as_map_mut(root) {
        map.insert(Value::String("openapi".to_owned()), "3.1.0".into());
    }
    for key in ["paths", "webhooks"] {
        if let Some(paths) = child_mut(root, key) {
            for_each_value(paths, path_item);
        }
    }
    if let Some(components) = child_mut(root, "components") {
        if let Some(schemas) = child_mut(components, "schemas") {
            for_each_value(schemas, schema);
        }
        if let Some(parameters) = child_mut(components, "parameters") {
            for_each_value(parameters, parameter);
        }
        if let Some(bodies) = child_mut(components, "requestBodies") {
            for_each_value(bodies, request_body);
        }
        if let Some(responses) = child_mut(components, "responses") {
            for_each_value(responses, response);
        }
        if let Some(headers) = child_mut(components, "headers") {
            for_each_value(headers, header);
        }
        if let Some(callbacks) = child_mut(components, "callbacks") {
            for_each_value(callbacks, |callback| for_each_value(callback, path_item));
        }
    }
}

fn path_item(item: &mut Value) {
    for_each_in_list(item, "parameters", parameter);
    for method in crate::model::Method::ALL {
        if let Some(operation) = child_mut(item, method.lowercase()) {
            for_each_in_list(operation, "parameters", parameter);
            if let Some(body) = child_mut(operation, "requestBody") {
                request_body(body);
            }
            if let Some(responses) = child_mut(operation, "responses") {
                for_each_value(responses, response);
            }
            if let Some(callbacks) = child_mut(operation, "callbacks") {
                for_each_value(callbacks, |callback| for_each_value(callback, path_item));
            }
        }
    }
}

fn parameter(value: &mut Value) {
    if let Some(found) = child_mut(value, "schema") {
        schema(found);
    }
    content(value);
}

fn request_body(value: &mut Value) {
    content(value);
}

fn response(value: &mut Value) {
    content(value);
    if let Some(headers) = child_mut(value, "headers") {
        for_each_value(headers, header);
    }
}

fn header(value: &mut Value) {
    if let Some(found) = child_mut(value, "schema") {
        schema(found);
    }
    content(value);
}

fn content(value: &mut Value) {
    let Some(content) = child_mut(value, "content") else {
        return;
    };
    for_each_value(content, |media| {
        if let Some(found) = child_mut(media, "schema") {
            schema(found);
        }
    });
}

/// The three rewrites, then the same treatment for every nested schema.
fn schema(value: &mut Value) {
    let Some(map) = as_map_mut(value) else { return };
    // A 3.0 `$ref` ignores its siblings, so there is nothing to rewrite.
    if map.contains_key("$ref") {
        return;
    }
    nullable(map);
    exclusive_bound(map, "exclusiveMinimum", "minimum");
    exclusive_bound(map, "exclusiveMaximum", "maximum");
    single_example(map);

    for key in ["items", "not", "additionalProperties", "propertyNames"] {
        if let Some(found) = map.get_mut(key) {
            schema(found);
        }
    }
    for key in ["properties", "patternProperties"] {
        if let Some(found) = map.get_mut(key) {
            for_each_value(found, schema);
        }
    }
    for key in ["allOf", "oneOf", "anyOf"] {
        if let Some(Value::Sequence(items)) = map.get_mut(key) {
            for item in items {
                schema(item);
            }
        }
    }
}

/// `nullable: true` becomes `null` among the permitted types.
///
/// With no `type` to extend, the schema already permits null, so the keyword
/// is only removed. With an `enum` that does not list null, null is added:
/// 3.0's `enum` did not have to list it and 2020-12's does.
fn nullable(map: &mut Map) {
    let Some(flag) = map.shift_remove("nullable").and_then(|v| v.as_bool()) else {
        return;
    };
    if !flag {
        return;
    }
    match map.get_mut("type") {
        Some(Value::String(_)) => {
            let Some(single) = map.get("type").cloned() else {
                return;
            };
            map.insert(
                Value::String("type".to_owned()),
                Value::Sequence(vec![single, "null".into()]),
            );
        }
        Some(Value::Sequence(types)) if !types.iter().any(|t| as_str(t) == Some("null")) => {
            types.push("null".into());
        }
        _ => {}
    }
    if let Some(Value::Sequence(values)) = map.get_mut("enum")
        && !values.contains(&Value::Null)
    {
        values.push(Value::Null);
    }
}

/// 3.0 wrote `exclusiveMinimum: true` beside `minimum: 3`; 2020-12 writes
/// `exclusiveMinimum: 3`.
fn exclusive_bound(map: &mut Map, exclusive: &str, inclusive: &str) {
    let Some(flag) = map.get(exclusive).and_then(Value::as_bool) else {
        return;
    };
    map.shift_remove(exclusive);
    if !flag {
        return;
    }
    if let Some(bound) = map.shift_remove(inclusive) {
        map.insert(Value::String(exclusive.to_owned()), bound);
    }
}

/// A schema's singular `example` becomes the first of `examples`.
fn single_example(map: &mut Map) {
    let Some(single) = map.shift_remove("example") else {
        return;
    };
    match map.get_mut("examples") {
        Some(Value::Sequence(examples)) => {
            if !examples.contains(&single) {
                examples.insert(0, single);
            }
        }
        _ => {
            map.insert(
                Value::String("examples".to_owned()),
                Value::Sequence(vec![single]),
            );
        }
    }
}

fn child_mut<'a>(value: &'a mut Value, key: &str) -> Option<&'a mut Value> {
    as_map_mut(value)?.get_mut(key)
}

fn for_each_value(value: &mut Value, mut each: impl FnMut(&mut Value)) {
    let Some(map) = as_map_mut(value) else { return };
    for (_, item) in map.iter_mut() {
        each(item);
    }
}

fn for_each_in_list(value: &mut Value, key: &str, mut each: impl FnMut(&mut Value)) {
    if let Some(Value::Sequence(items)) = child_mut(value, key) {
        for item in items {
            each(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Pointer, get, parse};

    fn normalized(source: &str) -> Value {
        let mut root = parse(source.as_bytes(), "test").expect("the fixture parses");
        normalize(&mut root);
        root
    }

    fn at(root: &Value, pointer: &str) -> Value {
        get(root, &Pointer::parse(pointer))
            .cloned()
            .unwrap_or(Value::Null)
    }

    #[test]
    fn nullable_becomes_a_type_array() {
        let root = normalized("components:\n  schemas:\n    A: { type: string, nullable: true }\n");
        assert_eq!(
            at(&root, "/components/schemas/A/type"),
            Value::Sequence(vec![Value::from("string"), Value::from("null")])
        );
        assert_eq!(at(&root, "/components/schemas/A/nullable"), Value::Null);
    }

    #[test]
    fn nullable_false_only_drops_the_keyword() {
        let root =
            normalized("components:\n  schemas:\n    A: { type: string, nullable: false }\n");
        assert_eq!(
            at(&root, "/components/schemas/A/type"),
            Value::from("string")
        );
    }

    #[test]
    fn a_nullable_enum_gains_the_null_it_did_not_have_to_list() {
        let root = normalized(
            "components:\n  schemas:\n    A: { type: string, enum: [a, b], nullable: true }\n",
        );
        assert_eq!(
            at(&root, "/components/schemas/A/enum"),
            Value::Sequence(vec![Value::from("a"), Value::from("b"), Value::Null])
        );
    }

    #[test]
    fn the_boolean_exclusive_bounds_take_the_number_beside_them() {
        let root = normalized(
            "components:\n  schemas:\n    A:\n      type: integer\n      minimum: 3\n      exclusiveMinimum: true\n      maximum: 9\n      exclusiveMaximum: false\n",
        );
        assert_eq!(
            at(&root, "/components/schemas/A/exclusiveMinimum"),
            Value::from(3)
        );
        assert_eq!(at(&root, "/components/schemas/A/minimum"), Value::Null);
        assert_eq!(at(&root, "/components/schemas/A/maximum"), Value::from(9));
        assert_eq!(
            at(&root, "/components/schemas/A/exclusiveMaximum"),
            Value::Null
        );
    }

    #[test]
    fn a_schemas_singular_example_becomes_the_list() {
        let root = normalized("components:\n  schemas:\n    A: { type: string, example: hi }\n");
        assert_eq!(
            at(&root, "/components/schemas/A/examples"),
            Value::Sequence(vec![Value::from("hi")])
        );
    }

    #[test]
    fn a_property_called_nullable_is_not_a_nullable_schema() {
        let root = normalized(
            "components:\n  schemas:\n    A:\n      type: object\n      properties:\n        nullable: { type: boolean }\n",
        );
        assert_eq!(
            at(&root, "/components/schemas/A/properties/nullable/type"),
            Value::from("boolean"),
            "the property survived"
        );
        assert_eq!(
            at(&root, "/components/schemas/A/type"),
            Value::from("object")
        );
    }

    #[test]
    fn the_walk_reaches_a_schema_inside_an_operations_response() {
        let root = normalized(
            r##"
paths:
  /a:
    get:
      parameters:
        - { name: q, in: query, schema: { type: string, nullable: true } }
      responses:
        "200":
          content:
            application/json:
              schema:
                type: object
                properties:
                  id: { type: string, nullable: true }
"##,
        );
        assert_eq!(
            at(&root, "/paths/~1a/get/parameters/0/schema/type"),
            Value::Sequence(vec![Value::from("string"), Value::from("null")])
        );
        assert_eq!(
            at(
                &root,
                "/paths/~1a/get/responses/200/content/application~1json/schema/properties/id/type"
            ),
            Value::Sequence(vec![Value::from("string"), Value::from("null")])
        );
    }

    #[test]
    fn a_ref_keeps_its_siblings_untouched() {
        let root = normalized(
            "components:\n  schemas:\n    A: { $ref: \"#/components/schemas/B\", nullable: true }\n",
        );
        assert_eq!(
            at(&root, "/components/schemas/A/nullable"),
            Value::from(true)
        );
    }

    #[test]
    fn the_document_says_three_one_afterwards() {
        assert_eq!(
            normalized("openapi: 3.0.3")["openapi"],
            Value::from("3.1.0")
        );
    }
}
