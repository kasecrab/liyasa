//! Who sees what (API-51, API-52).
//!
//! `x-liyasa.hidden` removes a node from everyone; `x-liyasa.internal` and
//! `x-liyasa.groups` narrow it to the reader groups that are allowed it. The
//! same rules are applied to the model a page is built from and to the
//! document the download serves (API-50), so a reader cannot find in the spec
//! what the page does not show them.

use serde::{Deserialize, Serialize};

use crate::model::{
    Callback, Components, MediaType, OrderedMap, Parameter, PathItem, Response, Schema, Spec,
    XLiyasa, ext,
};
use crate::tree::{Value, as_map_mut, as_seq, as_str};

/// The reader a filter is run for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Audience {
    pub groups: Vec<String>,
}

impl Audience {
    /// Someone signed in to nothing, which is what a static build renders.
    pub fn public() -> Self {
        Self::default()
    }

    pub fn of(groups: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            groups: groups.into_iter().map(Into::into).collect(),
        }
    }

    fn allows(&self, hints: &XLiyasa) -> bool {
        hints.visible_to(&self.groups)
    }
}

/// What a schema says about the individual values of its `enum`
/// (`plan/rfcs/0801-enum-value-visibility.md`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumRule {
    pub value: Value,
    pub hidden: bool,
    pub groups: Vec<String>,
    pub description: Option<String>,
}

impl EnumRule {
    fn allows(&self, audience: &Audience) -> bool {
        if self.hidden {
            return false;
        }
        self.groups.is_empty()
            || self
                .groups
                .iter()
                .any(|want| audience.groups.contains(want))
    }
}

/// Reads `x-liyasa.enum` off a schema.
pub fn enum_rules(schema: &Schema) -> Vec<EnumRule> {
    // TODO(rfc-0801): the shape is this crate's proposal, not the spec's.
    let Some(hints) = schema.extensions.get(ext::NAMESPACE) else {
        return Vec::new();
    };
    let Some(entries) = crate::tree::field(hints, "enum").and_then(as_seq) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            Some(EnumRule {
                value: crate::tree::field(entry, "value")?.clone(),
                hidden: crate::tree::field(entry, "hidden")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                groups: crate::tree::field(entry, "groups")
                    .and_then(as_seq)
                    .map(|groups| {
                        groups
                            .iter()
                            .filter_map(as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
                description: crate::tree::field(entry, "description")
                    .and_then(as_str)
                    .map(str::to_owned),
            })
        })
        .collect()
}

/// Removes from `spec` everything `audience` may not see.
pub fn filter(spec: &mut Spec, audience: &Audience) {
    spec.paths.retain(|_, item| {
        path_item(item, audience);
        !item.operations.is_empty()
    });
    spec.webhooks.retain(|_, item| {
        path_item(item, audience);
        !item.operations.is_empty()
    });
    spec.tags
        .retain(|tag| audience.allows(&XLiyasa::read(&tag.extensions)));
    components(&mut spec.components, audience);
}

fn path_item(item: &mut PathItem, audience: &Audience) {
    item.operations.retain(|_, operation| {
        if !audience.allows(&operation.liyasa) {
            return false;
        }
        operation.parameters.retain(|p| audience.allows(&p.liyasa));
        for parameter in &mut operation.parameters {
            parameter_schema(parameter, audience);
        }
        if let Some(body) = &mut operation.request_body {
            content(&mut body.content, audience);
        }
        for (_, response) in operation.responses.iter_mut() {
            response_of(response, audience);
        }
        for (_, Callback(paths)) in operation.callbacks.iter_mut() {
            for (_, item) in paths.iter_mut() {
                path_item(item, audience);
            }
        }
        true
    });
    item.parameters.retain(|p| audience.allows(&p.liyasa));
    for parameter in &mut item.parameters {
        parameter_schema(parameter, audience);
    }
}

fn parameter_schema(parameter: &mut Parameter, audience: &Audience) {
    if let Some(schema) = &mut parameter.schema {
        schema_of(schema, audience);
    }
    content(&mut parameter.content, audience);
}

fn response_of(response: &mut Response, audience: &Audience) {
    content(&mut response.content, audience);
    for (_, header) in response.headers.iter_mut() {
        if let Some(schema) = &mut header.schema {
            schema_of(schema, audience);
        }
    }
}

fn content(content: &mut OrderedMap<MediaType>, audience: &Audience) {
    for (_, media) in content.iter_mut() {
        if let Some(schema) = &mut media.schema {
            schema_of(schema, audience);
        }
    }
}

fn components(components: &mut Components, audience: &Audience) {
    components.schemas.retain(|_, schema| {
        schema_of(schema, audience);
        audience.allows(&XLiyasa::read(&schema.extensions))
    });
    for (_, response) in components.responses.iter_mut() {
        response_of(response, audience);
    }
    components.parameters.retain(|_, parameter| {
        parameter_schema(parameter, audience);
        audience.allows(&parameter.liyasa)
    });
    for (_, body) in components.request_bodies.iter_mut() {
        content(&mut body.content, audience);
    }
}

/// Filters a schema's properties, variants, and enum values.
pub fn schema_of(schema: &mut Schema, audience: &Audience) {
    let rules = enum_rules(schema);
    if !rules.is_empty() {
        schema.enumeration.retain(|value| {
            rules
                .iter()
                .find(|rule| rule.value == *value)
                .is_none_or(|rule| rule.allows(audience))
        });
    }
    schema.properties.retain(|_, property| {
        schema_of(property, audience);
        audience.allows(&XLiyasa::read(&property.extensions))
    });
    schema
        .required
        .retain(|name| schema.properties.contains_key(name));
    for list in [&mut schema.one_of, &mut schema.any_of, &mut schema.all_of] {
        list.retain(|variant| audience.allows(&XLiyasa::read(&variant.extensions)));
        for variant in list.iter_mut() {
            schema_of(variant, audience);
        }
    }
    if let Some(items) = schema.items.as_deref_mut() {
        schema_of(items, audience);
    }
    if let Some(extra) = match &mut schema.additional_properties {
        crate::model::AdditionalProperties::Schema(extra) => Some(extra.as_mut()),
        _ => None,
    } {
        schema_of(extra, audience);
    }
}

/// The same rules on the document tree, for the processed download (API-50).
///
/// The walk is structural, like the normalizer's: a property called `hidden`
/// is not a hidden property.
pub fn filter_tree(root: &mut Value, audience: &Audience) {
    for key in ["paths", "webhooks"] {
        if let Some(paths) = child_mut(root, key) {
            retain(paths, |item| {
                tree_path_item(item, audience);
                child(item, "get").is_some()
                    || crate::model::Method::ALL
                        .into_iter()
                        .any(|method| child(item, method.lowercase()).is_some())
            });
        }
    }
    if let Some(tags) = child_mut(root, "tags")
        && let Value::Sequence(items) = tags
    {
        items.retain(|tag| visible(tag, audience));
    }
    if let Some(components) = child_mut(root, "components") {
        for section in [
            "schemas",
            "parameters",
            "responses",
            "requestBodies",
            "headers",
        ] {
            if let Some(map) = child_mut(components, section) {
                retain(map, |item| {
                    tree_schema(item, audience);
                    visible(item, audience)
                });
            }
        }
    }
}

fn tree_path_item(item: &mut Value, audience: &Audience) {
    retain(item, |node| visible(node, audience));
    for method in crate::model::Method::ALL {
        let Some(operation) = child_mut(item, method.lowercase()) else {
            continue;
        };
        if let Some(Value::Sequence(parameters)) = child_mut(operation, "parameters") {
            parameters.retain(|parameter| visible(parameter, audience));
            for parameter in parameters.iter_mut() {
                if let Some(schema) = child_mut(parameter, "schema") {
                    tree_schema(schema, audience);
                }
            }
        }
        for key in ["requestBody", "responses"] {
            if let Some(node) = child_mut(operation, key) {
                tree_content(node, audience);
            }
        }
    }
}

/// Walks whatever `content` maps hang below a node.
fn tree_content(node: &mut Value, audience: &Audience) {
    if let Some(content) = child_mut(node, "content") {
        for_each(content, |media| {
            if let Some(schema) = child_mut(media, "schema") {
                tree_schema(schema, audience);
            }
        });
        return;
    }
    for_each(node, |item| tree_content(item, audience));
}

fn tree_schema(schema: &mut Value, audience: &Audience) {
    if let Some(properties) = child_mut(schema, "properties") {
        retain(properties, |property| {
            tree_schema(property, audience);
            visible(property, audience)
        });
    }
    if let Some(Value::Sequence(required)) = child_mut(schema, "required") {
        let kept: Vec<Value> = required.clone();
        let names: Vec<String> = child(schema, "properties")
            .map(|properties| {
                crate::tree::entries(properties)
                    .map(|(k, _)| k.to_owned())
                    .collect()
            })
            .unwrap_or_default();
        if !names.is_empty()
            && let Some(Value::Sequence(required)) = child_mut(schema, "required")
        {
            *required = kept
                .into_iter()
                .filter(|name| as_str(name).is_some_and(|name| names.iter().any(|k| k == name)))
                .collect();
        }
    }
    for key in ["items", "additionalProperties", "not"] {
        if let Some(item) = child_mut(schema, key) {
            tree_schema(item, audience);
        }
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(Value::Sequence(items)) = child_mut(schema, key) {
            items.retain(|item| visible(item, audience));
            for item in items.iter_mut() {
                tree_schema(item, audience);
            }
        }
    }
}

/// Whether one node's `x-liyasa` lets `audience` see it.
fn visible(node: &Value, audience: &Audience) -> bool {
    let Some(hints) = crate::tree::field(node, ext::NAMESPACE) else {
        return true;
    };
    let flag = |key: &str| {
        crate::tree::field(hints, key)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    if flag("hidden") {
        return false;
    }
    let groups: Vec<&str> = crate::tree::field(hints, "groups")
        .and_then(as_seq)
        .map(|groups| groups.iter().filter_map(as_str).collect())
        .unwrap_or_default();
    if groups.is_empty() {
        return !flag("internal");
    }
    groups
        .iter()
        .any(|want| audience.groups.iter().any(|has| has == want))
}

fn child<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    crate::tree::field(value, key)
}

fn child_mut<'a>(value: &'a mut Value, key: &str) -> Option<&'a mut Value> {
    as_map_mut(value)?.get_mut(key)
}

fn retain(value: &mut Value, mut keep: impl FnMut(&mut Value) -> bool) {
    if let Some(map) = as_map_mut(value) {
        map.retain(|_, item| keep(item));
    }
}

fn for_each(value: &mut Value, mut each: impl FnMut(&mut Value)) {
    if let Some(map) = as_map_mut(value) {
        for (_, item) in map.iter_mut() {
            each(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load;
    use crate::tree::{Pointer, get, parse};

    const SPEC: &str = r##"
openapi: 3.1.0
info: { title: T, version: "1" }
tags:
  - { name: Public }
  - { name: Staff, x-liyasa: { internal: true, groups: [staff] } }
paths:
  /open:
    get:
      operationId: open
      responses: { "200": { description: ok } }
  /secret:
    get:
      operationId: secret
      x-liyasa: { hidden: true }
      responses: { "200": { description: ok } }
  /staff:
    get:
      operationId: staffOnly
      x-liyasa: { internal: true, groups: [staff] }
      responses: { "200": { description: ok } }
  /widgets:
    post:
      operationId: makeWidget
      requestBody:
        content:
          application/json:
            schema:
              type: object
              required: [name, cost]
              properties:
                name: { type: string }
                cost: { type: integer, x-liyasa: { internal: true, groups: [staff] } }
                state:
                  type: string
                  enum: [active, archived, quarantined]
                  x-liyasa:
                    enum:
                      - { value: quarantined, groups: [staff] }
                      - { value: archived, hidden: true }
      responses: { "201": { description: made } }
"##;

    fn filtered(audience: &Audience) -> Spec {
        let mut spec = load::from_bytes("api", "api.yaml", SPEC.as_bytes())
            .expect("the spec loads")
            .spec;
        filter(&mut spec, audience);
        spec
    }

    #[test]
    fn hidden_removes_an_operation_from_everyone() {
        for audience in [Audience::public(), Audience::of(["staff"])] {
            let spec = filtered(&audience);
            assert!(spec.by_operation_id("secret").is_none(), "{audience:?}");
        }
    }

    #[test]
    fn internal_narrows_an_operation_to_its_groups() {
        assert!(
            filtered(&Audience::public())
                .by_operation_id("staffOnly")
                .is_none()
        );
        assert!(
            filtered(&Audience::of(["staff"]))
                .by_operation_id("staffOnly")
                .is_some()
        );
        assert!(
            filtered(&Audience::of(["customer"]))
                .by_operation_id("staffOnly")
                .is_none()
        );
    }

    #[test]
    fn a_tag_nobody_can_see_is_dropped_with_its_operations() {
        assert_eq!(
            filtered(&Audience::public())
                .tags
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Public"]
        );
        assert_eq!(filtered(&Audience::of(["staff"])).tags.len(), 2);
    }

    fn body_properties(spec: &Spec) -> Vec<String> {
        let operation = spec
            .by_operation_id("makeWidget")
            .expect("the operation is there");
        let body = operation.operation.request_body.as_ref().expect("a body");
        let (_, media) = body.preferred().expect("content");
        media
            .schema
            .as_ref()
            .expect("a schema")
            .properties
            .keys()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn an_internal_property_is_dropped_along_with_its_required_entry() {
        let public = filtered(&Audience::public());
        assert_eq!(body_properties(&public), vec!["name", "state"]);

        let operation = public.by_operation_id("makeWidget").expect("the operation");
        let body = operation.operation.request_body.as_ref().expect("a body");
        let (_, media) = body.preferred().expect("content");
        assert_eq!(
            media.schema.as_ref().expect("a schema").required,
            vec!["name".to_owned()],
            "`cost` is gone, so requiring it would be a schema nobody can satisfy"
        );

        assert!(body_properties(&filtered(&Audience::of(["staff"]))).contains(&"cost".to_owned()));
    }

    fn states(spec: &Spec) -> Vec<String> {
        let operation = spec.by_operation_id("makeWidget").expect("the operation");
        let body = operation.operation.request_body.as_ref().expect("a body");
        let (_, media) = body.preferred().expect("content");
        media
            .schema
            .as_ref()
            .and_then(|schema| schema.properties.get("state"))
            .map(|state| {
                state
                    .enumeration
                    .iter()
                    .filter_map(as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn an_enum_value_is_filtered_by_its_own_rule() {
        assert_eq!(states(&filtered(&Audience::public())), vec!["active"]);
        assert_eq!(
            states(&filtered(&Audience::of(["staff"]))),
            vec!["active", "quarantined"],
            "hidden removes a value from everyone; groups narrows one"
        );
    }

    fn filtered_tree(audience: &Audience) -> Value {
        let mut tree = parse(SPEC.as_bytes(), "api.yaml").expect("the fixture parses");
        filter_tree(&mut tree, audience);
        tree
    }

    fn at(tree: &Value, pointer: &str) -> Value {
        get(tree, &Pointer::parse(pointer))
            .cloned()
            .unwrap_or(Value::Null)
    }

    #[test]
    fn the_downloadable_document_loses_what_the_page_does() {
        let tree = filtered_tree(&Audience::public());
        assert_eq!(at(&tree, "/paths/~1secret"), Value::Null);
        assert_eq!(at(&tree, "/paths/~1staff"), Value::Null);
        assert!(at(&tree, "/paths/~1open/get").is_mapping());
        assert_eq!(
            at(
                &tree,
                "/paths/~1widgets/post/requestBody/content/application~1json/schema/properties/cost"
            ),
            Value::Null,
            "an internal property is not in the download either"
        );
    }

    #[test]
    fn a_reader_in_the_group_gets_the_whole_document_except_the_hidden_parts() {
        let tree = filtered_tree(&Audience::of(["staff"]));
        assert!(at(&tree, "/paths/~1staff/get").is_mapping());
        assert_eq!(at(&tree, "/paths/~1secret"), Value::Null);
        assert!(
            at(
                &tree,
                "/paths/~1widgets/post/requestBody/content/application~1json/schema/properties/cost"
            )
            .is_mapping()
        );
    }

    #[test]
    fn an_enum_rule_reads_off_the_schema() {
        let spec = load::from_bytes("api", "api.yaml", SPEC.as_bytes())
            .expect("the spec loads")
            .spec;
        let operation = spec.by_operation_id("makeWidget").expect("the operation");
        let body = operation.operation.request_body.as_ref().expect("a body");
        let (_, media) = body.preferred().expect("content");
        let state = media
            .schema
            .as_ref()
            .and_then(|schema| schema.properties.get("state"))
            .expect("the property is there");
        let rules = enum_rules(state);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].value, Value::from("quarantined"));
        assert_eq!(rules[0].groups, vec!["staff".to_owned()]);
        assert!(rules[1].hidden);
    }
}
