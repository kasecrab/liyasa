//! Swagger 2.0 into OpenAPI 3.0's shape, which [`super::v30`] then takes the
//! rest of the way (API-01).
//!
//! The conversion is mechanical and covers what a reference page needs: the
//! server block, the component sections, body and form parameters, response
//! bodies, and the security schemes. It is announced with `W0509` because a
//! converted document is a second-hand reading of the API.

use crate::tree::{Map, Value, as_map, as_map_mut, as_seq, as_str};

/// Rewrites a parsed 2.0 document in place into the 3.0 shape.
pub fn convert(root: &mut Value) {
    let Some(map) = as_map_mut(root) else { return };
    map.shift_remove("swagger");
    map.insert(Value::String("openapi".to_owned()), "3.0.3".into());

    let servers = servers(map);
    let consumes = media_types(map, "consumes", "application/json");
    let produces = media_types(map, "produces", "application/json");
    map.shift_remove("consumes");
    map.shift_remove("produces");
    map.shift_remove("host");
    map.shift_remove("basePath");
    map.shift_remove("schemes");
    if !servers.is_empty() {
        map.insert(
            Value::String("servers".to_owned()),
            Value::Sequence(servers),
        );
    }

    let mut components = Map::new();
    for (from, to) in [
        ("definitions", "schemas"),
        ("parameters", "parameters"),
        ("responses", "responses"),
    ] {
        if let Some(section) = map.shift_remove(from) {
            components.insert(Value::String(to.to_owned()), section);
        }
    }
    if let Some(schemes) = map.shift_remove("securityDefinitions") {
        components.insert(
            Value::String("securitySchemes".to_owned()),
            security_schemes(&schemes),
        );
    }
    if !components.is_empty() {
        map.insert(
            Value::String("components".to_owned()),
            Value::Mapping(components),
        );
    }

    if let Some(paths) = map.get_mut("paths")
        && let Some(paths) = as_map_mut(paths)
    {
        for (_, item) in paths.iter_mut() {
            path_item(item, &consumes, &produces);
        }
    }

    // The component sections moved, so every pointer into them moved too.
    retarget(root);
}

fn servers(map: &Map) -> Vec<Value> {
    let host = map.get("host").and_then(as_str).unwrap_or_default();
    let base = map.get("basePath").and_then(as_str).unwrap_or_default();
    if host.is_empty() && base.is_empty() {
        return Vec::new();
    }
    let schemes: Vec<&str> = map
        .get("schemes")
        .and_then(as_seq)
        .map(|items| items.iter().filter_map(as_str).collect())
        .unwrap_or_default();
    let schemes = if schemes.is_empty() {
        vec!["https"]
    } else {
        schemes
    };
    if host.is_empty() {
        return vec![server(base)];
    }
    schemes
        .into_iter()
        .map(|scheme| server(&format!("{scheme}://{host}{base}")))
        .collect()
}

fn server(url: &str) -> Value {
    let mut map = Map::new();
    map.insert(Value::String("url".to_owned()), url.into());
    Value::Mapping(map)
}

fn media_types(map: &Map, key: &str, fallback: &str) -> Vec<String> {
    let listed: Vec<String> = map
        .get(key)
        .and_then(as_seq)
        .map(|items| items.iter().filter_map(as_str).map(str::to_owned).collect())
        .unwrap_or_default();
    if listed.is_empty() {
        vec![fallback.to_owned()]
    } else {
        listed
    }
}

fn path_item(item: &mut Value, consumes: &[String], produces: &[String]) {
    let Some(map) = as_map_mut(item) else { return };
    let shared = map
        .get("parameters")
        .and_then(as_seq)
        .map(<[Value]>::to_vec)
        .unwrap_or_default();
    // A body parameter on the path item has nowhere to go in 3.x, so it is
    // pushed down onto each operation before the split.
    map.shift_remove("parameters");
    let mut path_level: Vec<Value> = Vec::new();
    for parameter in &shared {
        if location(parameter) == Some("body") || location(parameter) == Some("formData") {
            continue;
        }
        path_level.push(parameter.clone());
    }
    if !path_level.is_empty() {
        map.insert(
            Value::String("parameters".to_owned()),
            Value::Sequence(path_level.clone()),
        );
    }

    for method in crate::model::Method::ALL {
        let Some(operation) = map.get_mut(method.lowercase()) else {
            continue;
        };
        let inherited: Vec<Value> = shared
            .iter()
            .filter(|p| matches!(location(p), Some("body" | "formData")))
            .cloned()
            .collect();
        self_operation(operation, &inherited, consumes, produces);
    }
}

fn location(parameter: &Value) -> Option<&str> {
    crate::tree::field_str(parameter, "in")
}

fn self_operation(
    operation: &mut Value,
    inherited: &[Value],
    consumes: &[String],
    produces: &[String],
) {
    let Some(map) = as_map_mut(operation) else {
        return;
    };
    let consumes = own_media_types(map, "consumes", consumes);
    let produces = own_media_types(map, "produces", produces);
    map.shift_remove("consumes");
    map.shift_remove("produces");

    let mut parameters: Vec<Value> = inherited.to_vec();
    parameters.extend(
        map.get("parameters")
            .and_then(as_seq)
            .map(<[Value]>::to_vec)
            .unwrap_or_default(),
    );

    let mut kept = Vec::new();
    let mut body = None;
    let mut form: Vec<Value> = Vec::new();
    for parameter in parameters {
        match location(&parameter) {
            Some("body") => body = Some(parameter),
            Some("formData") => form.push(parameter),
            _ => kept.push(simple_parameter(parameter)),
        }
    }
    if kept.is_empty() {
        map.shift_remove("parameters");
    } else {
        map.insert(
            Value::String("parameters".to_owned()),
            Value::Sequence(kept),
        );
    }
    if let Some(body) = body {
        map.insert(
            Value::String("requestBody".to_owned()),
            request_body(&body, &consumes),
        );
    } else if !form.is_empty() {
        map.insert(
            Value::String("requestBody".to_owned()),
            form_body(&form, &consumes),
        );
    }

    if let Some(responses) = map.get_mut("responses")
        && let Some(responses) = as_map_mut(responses)
    {
        for (_, response) in responses.iter_mut() {
            self_response(response, &produces);
        }
    }
}

fn own_media_types(map: &Map, key: &str, fallback: &[String]) -> Vec<String> {
    let listed: Vec<String> = map
        .get(key)
        .and_then(as_seq)
        .map(|items| items.iter().filter_map(as_str).map(str::to_owned).collect())
        .unwrap_or_default();
    if listed.is_empty() {
        fallback.to_vec()
    } else {
        listed
    }
}

/// A 2.0 non-body parameter carried its type inline; 3.x moves it into
/// `schema`.
fn simple_parameter(parameter: Value) -> Value {
    let Some(mut map) = as_map(&parameter).cloned() else {
        return parameter;
    };
    if map.contains_key("schema") || map.contains_key("$ref") {
        return Value::Mapping(map);
    }
    const INLINE: &[&str] = &[
        "type",
        "format",
        "items",
        "default",
        "enum",
        "maximum",
        "exclusiveMaximum",
        "minimum",
        "exclusiveMinimum",
        "maxLength",
        "minLength",
        "pattern",
        "maxItems",
        "minItems",
        "uniqueItems",
        "multipleOf",
    ];
    let mut schema = Map::new();
    for key in INLINE {
        if let Some(value) = map.shift_remove(*key) {
            schema.insert(Value::String((*key).to_owned()), value);
        }
    }
    if let Some(format) = map.shift_remove("collectionFormat").and_then(|v| {
        as_str(&v).map(|text| match text {
            "ssv" => "spaceDelimited",
            "pipes" => "pipeDelimited",
            _ => "form",
        })
    }) {
        map.insert(Value::String("style".to_owned()), format.into());
    }
    if !schema.is_empty() {
        map.insert(Value::String("schema".to_owned()), Value::Mapping(schema));
    }
    Value::Mapping(map)
}

fn request_body(parameter: &Value, consumes: &[String]) -> Value {
    let mut body = Map::new();
    if let Some(description) = crate::tree::field(parameter, "description") {
        body.insert(Value::String("description".to_owned()), description.clone());
    }
    if crate::tree::field(parameter, "required").and_then(Value::as_bool) == Some(true) {
        body.insert(Value::String("required".to_owned()), true.into());
    }
    let schema = crate::tree::field(parameter, "schema")
        .cloned()
        .unwrap_or_else(|| Value::Mapping(Map::new()));
    body.insert(
        Value::String("content".to_owned()),
        content(consumes, &schema),
    );
    Value::Mapping(body)
}

/// Form parameters become one object schema, which is what 3.x models a form
/// body as.
fn form_body(parameters: &[Value], consumes: &[String]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    let mut multipart = false;
    for parameter in parameters {
        let Some(name) = crate::tree::field_str(parameter, "name") else {
            continue;
        };
        // `type: file` has no 3.x spelling; it is a binary string, and it is
        // what makes the body multipart rather than form-urlencoded.
        if crate::tree::field_str(parameter, "type") == Some("file") {
            multipart = true;
            let mut file = Map::new();
            file.insert(Value::String("type".to_owned()), "string".into());
            file.insert(Value::String("format".to_owned()), "binary".into());
            properties.insert(Value::String(name.to_owned()), Value::Mapping(file));
        } else if let Some(schema) =
            as_map(&simple_parameter(parameter.clone())).and_then(|map| map.get("schema").cloned())
        {
            properties.insert(Value::String(name.to_owned()), schema);
        }
        if crate::tree::field(parameter, "required").and_then(Value::as_bool) == Some(true) {
            required.push(Value::String(name.to_owned()));
        }
    }

    let mut schema = Map::new();
    schema.insert(Value::String("type".to_owned()), "object".into());
    schema.insert(
        Value::String("properties".to_owned()),
        Value::Mapping(properties),
    );
    if !required.is_empty() {
        schema.insert(
            Value::String("required".to_owned()),
            Value::Sequence(required),
        );
    }

    let media = if multipart {
        vec!["multipart/form-data".to_owned()]
    } else if consumes.iter().any(|m| m.starts_with("multipart/")) {
        consumes.to_vec()
    } else {
        vec!["application/x-www-form-urlencoded".to_owned()]
    };

    let mut body = Map::new();
    body.insert(
        Value::String("content".to_owned()),
        content(&media, &Value::Mapping(schema)),
    );
    Value::Mapping(body)
}

fn self_response(response: &mut Value, produces: &[String]) {
    let Some(map) = as_map_mut(response) else {
        return;
    };
    if let Some(schema) = map.shift_remove("schema") {
        map.insert(
            Value::String("content".to_owned()),
            content(produces, &schema),
        );
    }
    if let Some(headers) = map.get_mut("headers")
        && let Some(headers) = as_map_mut(headers)
    {
        for (_, header) in headers.iter_mut() {
            *header = simple_parameter(header.clone());
        }
    }
}

fn content(media_types: &[String], schema: &Value) -> Value {
    let mut content = Map::new();
    let mut entry = Map::new();
    entry.insert(Value::String("schema".to_owned()), schema.clone());
    for media in media_types {
        content.insert(Value::String(media.clone()), Value::Mapping(entry.clone()));
    }
    Value::Mapping(content)
}

fn security_schemes(schemes: &Value) -> Value {
    let Some(map) = as_map(schemes) else {
        return schemes.clone();
    };
    let mut out = Map::new();
    for (name, scheme) in map.iter() {
        out.insert(name.clone(), security_scheme(scheme));
    }
    Value::Mapping(out)
}

fn security_scheme(scheme: &Value) -> Value {
    let Some(mut map) = as_map(scheme).cloned() else {
        return scheme.clone();
    };
    match map.get("type").and_then(as_str) {
        Some("basic") => {
            map.insert(Value::String("type".to_owned()), "http".into());
            map.insert(Value::String("scheme".to_owned()), "basic".into());
        }
        Some("oauth2") => {
            let flow = map.shift_remove("flow");
            let name = match flow.as_ref().and_then(as_str) {
                Some("implicit") => "implicit",
                Some("password") => "password",
                Some("application") => "clientCredentials",
                _ => "authorizationCode",
            };
            let mut inner = Map::new();
            for key in ["authorizationUrl", "tokenUrl", "scopes"] {
                if let Some(value) = map.shift_remove(key) {
                    inner.insert(Value::String(key.to_owned()), value);
                }
            }
            let mut flows = Map::new();
            flows.insert(Value::String(name.to_owned()), Value::Mapping(inner));
            map.insert(Value::String("flows".to_owned()), Value::Mapping(flows));
        }
        _ => {}
    }
    Value::Mapping(map)
}

/// `#/definitions/X` and the other 2.0 sections moved under `components`.
fn retarget(value: &mut Value) {
    match value {
        Value::Mapping(map) => {
            if let Some(Value::String(reference)) = map.get_mut("$ref") {
                for (from, to) in [
                    ("#/definitions/", "#/components/schemas/"),
                    ("#/parameters/", "#/components/parameters/"),
                    ("#/responses/", "#/components/responses/"),
                ] {
                    if let Some(rest) = reference.strip_prefix(from) {
                        *reference = format!("{to}{rest}");
                        break;
                    }
                }
            }
            for (_, item) in map.iter_mut() {
                retarget(item);
            }
        }
        Value::Sequence(items) => {
            for item in items {
                retarget(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Pointer, get, parse};

    fn converted(source: &str) -> Value {
        let mut root = parse(source.as_bytes(), "test").expect("the fixture parses");
        convert(&mut root);
        root
    }

    fn at(root: &Value, pointer: &str) -> Value {
        get(root, &Pointer::parse(pointer))
            .cloned()
            .unwrap_or(Value::Null)
    }

    #[test]
    fn host_base_path_and_schemes_become_servers() {
        let root = converted(
            "swagger: \"2.0\"\nhost: api.example.com\nbasePath: /v1\nschemes: [https, http]\n",
        );
        assert_eq!(
            at(&root, "/servers/0/url"),
            Value::from("https://api.example.com/v1")
        );
        assert_eq!(
            at(&root, "/servers/1/url"),
            Value::from("http://api.example.com/v1")
        );
        assert_eq!(at(&root, "/openapi"), Value::from("3.0.3"));
    }

    #[test]
    fn definitions_move_under_components_and_every_pointer_follows() {
        let root = converted(
            r##"
swagger: "2.0"
definitions:
  User: { type: object }
paths:
  /users:
    get:
      responses:
        "200":
          description: ok
          schema: { $ref: "#/definitions/User" }
"##,
        );
        assert_eq!(
            at(&root, "/components/schemas/User/type"),
            Value::from("object")
        );
        assert_eq!(
            at(
                &root,
                "/paths/~1users/get/responses/200/content/application~1json/schema/$ref"
            ),
            Value::from("#/components/schemas/User")
        );
    }

    #[test]
    fn a_body_parameter_becomes_a_request_body_at_the_operations_consumes() {
        let root = converted(
            r##"
swagger: "2.0"
consumes: [application/json]
paths:
  /users:
    post:
      parameters:
        - { name: body, in: body, required: true, schema: { type: object } }
      responses:
        "201": { description: made }
"##,
        );
        assert_eq!(
            at(&root, "/paths/~1users/post/requestBody/required"),
            Value::from(true)
        );
        assert_eq!(
            at(
                &root,
                "/paths/~1users/post/requestBody/content/application~1json/schema/type"
            ),
            Value::from("object")
        );
        assert_eq!(at(&root, "/paths/~1users/post/parameters"), Value::Null);
    }

    #[test]
    fn a_file_form_parameter_becomes_a_multipart_body() {
        let root = converted(
            r##"
swagger: "2.0"
paths:
  /upload:
    post:
      parameters:
        - { name: file, in: formData, type: file, required: true }
        - { name: note, in: formData, type: string }
      responses:
        "200": { description: ok }
"##,
        );
        let base = "/paths/~1upload/post/requestBody/content/multipart~1form-data/schema";
        assert_eq!(
            at(&root, &format!("{base}/properties/file/format")),
            Value::from("binary")
        );
        assert_eq!(
            at(&root, &format!("{base}/properties/note/type")),
            Value::from("string")
        );
        assert_eq!(
            at(&root, &format!("{base}/required")),
            Value::Sequence(vec![Value::from("file")])
        );
    }

    #[test]
    fn a_query_parameters_inline_type_moves_into_a_schema() {
        let root = converted(
            r##"
swagger: "2.0"
paths:
  /users:
    get:
      parameters:
        - { name: limit, in: query, type: integer, format: int32, minimum: 1 }
      responses:
        "200": { description: ok }
"##,
        );
        let schema = "/paths/~1users/get/parameters/0/schema";
        assert_eq!(at(&root, &format!("{schema}/type")), Value::from("integer"));
        assert_eq!(at(&root, &format!("{schema}/minimum")), Value::from(1));
        assert_eq!(
            at(&root, "/paths/~1users/get/parameters/0/type"),
            Value::Null
        );
    }

    #[test]
    fn basic_auth_becomes_http_basic_and_oauth_gains_a_flows_block() {
        let root = converted(
            r##"
swagger: "2.0"
securityDefinitions:
  legacy: { type: basic }
  oauth:
    type: oauth2
    flow: application
    tokenUrl: https://example.com/token
    scopes: { read: Read }
"##,
        );
        assert_eq!(
            at(&root, "/components/securitySchemes/legacy/type"),
            Value::from("http")
        );
        assert_eq!(
            at(&root, "/components/securitySchemes/legacy/scheme"),
            Value::from("basic")
        );
        assert_eq!(
            at(
                &root,
                "/components/securitySchemes/oauth/flows/clientCredentials/tokenUrl"
            ),
            Value::from("https://example.com/token")
        );
    }

    #[test]
    fn a_response_schema_becomes_content_at_the_operations_produces() {
        let root = converted(
            r##"
swagger: "2.0"
paths:
  /a:
    get:
      produces: [application/xml]
      responses:
        "200": { description: ok, schema: { type: string } }
"##,
        );
        assert_eq!(
            at(
                &root,
                "/paths/~1a/get/responses/200/content/application~1xml/schema/type"
            ),
            Value::from("string")
        );
    }
}
