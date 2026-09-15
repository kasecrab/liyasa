//! The controls above a parameter or schema table (API-13).
//!
//! A table of two hundred rows is not a reference, it is a haystack. Three
//! controls make it one again: show only what a request must carry, search the
//! field names, and take an object's schema away to paste somewhere else.
//!
//! Filtering happens here rather than in the reader runtime so that the HTML
//! page, the Markdown (API-14), and the search index agree about what a filter
//! leaves behind.

use crate::field::{Field, Variant};
use crate::model::{AdditionalProperties, Discriminator, Schema, SchemaType};
use crate::tree::{Map, Value};

/// Past this many rows a table is long enough to want a search box.
pub const SEARCH_THRESHOLD: usize = 12;

/// The JSON Schema dialect the copied text declares.
pub const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Whether a table is worth offering a field search over.
pub fn wants_search(fields: &[Field]) -> bool {
    rows(fields) > SEARCH_THRESHOLD
}

fn rows(fields: &[Field]) -> usize {
    fields
        .iter()
        .map(|field| 1 + rows(&field.children) + rows(&variant_fields(field)))
        .sum()
}

fn variant_fields(field: &Field) -> Vec<Field> {
    field
        .variants
        .iter()
        .map(|variant| (*variant.field).clone())
        .collect()
}

/// The rows a reader sees with the required-only pill on.
///
/// An optional object whose children include a required field stays: hiding it
/// would hide the required field with it, and a filter that loses what it is
/// filtering for is worse than no filter.
pub fn required_only(fields: &[Field]) -> Vec<Field> {
    keep(fields, &|field| field.required, Descendants::Filtered)
}

/// The rows a reader sees with `query` typed into the field search.
///
/// A row matches on its name, its description, its type, or one of its enum
/// values. A row whose descendant matches is kept too, so a hit is shown where
/// it lives rather than torn out of its object.
pub fn search(fields: &[Field], query: &str) -> Vec<Field> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return fields.to_vec();
    }
    keep(
        fields,
        &|field| haystack(field).contains(&needle),
        Descendants::Whole,
    )
}

/// What a row that matches keeps underneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Descendants {
    /// Everything: a reader who searched for an object wants to see inside it.
    Whole,
    /// Only what matches in turn: a required object may hold optional fields,
    /// and the required-only pill means to hide those too.
    Filtered,
}

/// The text a field search looks in.
pub fn haystack(field: &Field) -> String {
    let mut text = field.name.to_lowercase();
    text.push(' ');
    text.push_str(&field.type_label.to_lowercase());
    if let Some(format) = &field.format {
        text.push(' ');
        text.push_str(&format.to_lowercase());
    }
    if let Some(description) = &field.description {
        text.push(' ');
        text.push_str(&description.to_lowercase());
    }
    for value in &field.enumeration {
        text.push(' ');
        text.push_str(&crate::example::as_text(value).to_lowercase());
    }
    text
}

/// Keeps every row that matches, plus every ancestor of one.
fn keep(fields: &[Field], wanted: &dyn Fn(&Field) -> bool, under: Descendants) -> Vec<Field> {
    let mut out = Vec::new();
    for field in fields {
        let children = keep(&field.children, wanted, under);
        let variants: Vec<Variant> = field
            .variants
            .iter()
            .filter_map(|variant| {
                let kept = keep(std::slice::from_ref(&*variant.field), wanted, under);
                kept.into_iter().next().map(|field| Variant {
                    label: variant.label.clone(),
                    field: Box::new(field),
                })
            })
            .collect();
        let hit = wanted(field);
        if !hit && children.is_empty() && variants.is_empty() {
            continue;
        }
        let mut field = field.clone();
        // A row kept only because something under it matched shows the way
        // down to that and nothing else, whichever control is on.
        if !hit || under == Descendants::Filtered {
            field.children = children;
            field.variants = variants;
        }
        out.push(field);
    }
    out
}

/// One schema as JSON Schema 2020-12, ready to be copied.
///
/// The model's own `Serialize` is not this: it spells `type` as `types` and
/// `enum` as `enumeration` because those are Rust keywords, and it carries
/// `name`, which is Liyasa's bookkeeping rather than anything a validator
/// reads. Everything the spec wrote that the model does not have a field for
/// rides in [`Schema::rest`] and is written back out here.
pub fn json_schema(schema: &Schema) -> String {
    let written = write(schema);
    let mut root = Map::new();
    root.insert(Value::from("$schema"), Value::from(DIALECT));
    if let Some(map) = crate::tree::as_map(&written) {
        for (key, value) in map {
            root.insert(key.clone(), value.clone());
        }
    }
    crate::tree::to_json(&Value::Mapping(root)).unwrap_or_else(|_| "{}".to_owned())
}

/// Whether a row is one the copy control is offered on.
pub fn is_copyable(schema: &Schema) -> bool {
    schema.is(SchemaType::Object) || !schema.properties.is_empty()
}

fn write(schema: &Schema) -> Value {
    // A cycle the reader cut stands for the component it named, and a `$ref`
    // is what that means to anything reading the copied text (API-11).
    if schema.is_stub()
        && let Some(name) = &schema.name
    {
        let mut map = Map::new();
        map.insert(Value::from("$ref"), Value::from(reference(name)));
        return Value::Mapping(map);
    }

    let mut map = Map::new();
    let mut put = |key: &str, value: Value| {
        map.insert(Value::from(key), value);
    };

    match schema.types.len() {
        0 => {}
        1 => put("type", Value::from(schema.types[0].as_str())),
        _ => put(
            "type",
            Value::Sequence(
                schema
                    .types
                    .iter()
                    .map(|ty| Value::from(ty.as_str()))
                    .collect(),
            ),
        ),
    }
    for (key, text) in [
        ("format", &schema.format),
        ("title", &schema.title),
        ("description", &schema.description),
        ("pattern", &schema.pattern),
        ("contentMediaType", &schema.content_media_type),
        ("contentEncoding", &schema.content_encoding),
    ] {
        if let Some(text) = text {
            put(key, Value::from(text.as_str()));
        }
    }
    if let Some(default) = &schema.default {
        put("default", default.clone());
    }
    if !schema.examples.is_empty() {
        put("examples", Value::Sequence(schema.examples.clone()));
    }
    if !schema.enumeration.is_empty() {
        put("enum", Value::Sequence(schema.enumeration.clone()));
    }
    if let Some(constant) = &schema.constant {
        put("const", constant.clone());
    }
    for (key, flag) in [
        ("deprecated", schema.deprecated),
        ("readOnly", schema.read_only),
        ("writeOnly", schema.write_only),
        ("uniqueItems", schema.unique_items),
    ] {
        if flag {
            put(key, Value::from(true));
        }
    }

    for (key, members) in [
        ("allOf", &schema.all_of),
        ("oneOf", &schema.one_of),
        ("anyOf", &schema.any_of),
        ("prefixItems", &schema.prefix_items),
    ] {
        if !members.is_empty() {
            put(key, Value::Sequence(members.iter().map(write).collect()));
        }
    }
    if let Some(not) = &schema.not {
        put("not", write(not));
    }
    if let Some(discriminator) = &schema.discriminator {
        put("discriminator", write_discriminator(discriminator));
    }

    if !schema.properties.is_empty() {
        put("properties", write_map(schema.properties.iter()));
    }
    if !schema.required.is_empty() {
        put(
            "required",
            Value::Sequence(
                schema
                    .required
                    .iter()
                    .map(|name| Value::from(name.as_str()))
                    .collect(),
            ),
        );
    }
    match &schema.additional_properties {
        AdditionalProperties::Unset => {}
        AdditionalProperties::Allowed => put("additionalProperties", Value::from(true)),
        AdditionalProperties::Denied => put("additionalProperties", Value::from(false)),
        AdditionalProperties::Schema(inner) => put("additionalProperties", write(inner)),
    }
    if !schema.pattern_properties.is_empty() {
        put(
            "patternProperties",
            write_map(schema.pattern_properties.iter()),
        );
    }
    if let Some(names) = &schema.property_names {
        put("propertyNames", write(names));
    }
    if let Some(items) = &schema.items {
        put("items", write(items));
    }

    for (key, bound) in [
        ("minProperties", schema.min_properties),
        ("maxProperties", schema.max_properties),
        ("minItems", schema.min_items),
        ("maxItems", schema.max_items),
        ("minLength", schema.min_length),
        ("maxLength", schema.max_length),
    ] {
        if let Some(bound) = bound {
            put(key, Value::from(bound));
        }
    }
    for (key, bound) in [
        ("minimum", &schema.minimum),
        ("maximum", &schema.maximum),
        ("exclusiveMinimum", &schema.exclusive_minimum),
        ("exclusiveMaximum", &schema.exclusive_maximum),
        ("multipleOf", &schema.multiple_of),
    ] {
        if let Some(bound) = bound {
            put(key, Value::Number(bound.clone()));
        }
    }

    for (key, value) in schema.rest.iter() {
        put(key, value.clone());
    }
    for (key, value) in schema.extensions.iter() {
        put(key, value.clone());
    }
    Value::Mapping(map)
}

fn write_map<'a>(entries: impl Iterator<Item = (&'a str, &'a Schema)>) -> Value {
    let mut map = Map::new();
    for (name, schema) in entries {
        map.insert(Value::from(name), write(schema));
    }
    Value::Mapping(map)
}

fn write_discriminator(discriminator: &Discriminator) -> Value {
    let mut map = Map::new();
    map.insert(
        Value::from("propertyName"),
        Value::from(discriminator.property_name.as_str()),
    );
    if !discriminator.mapping.is_empty() {
        let mut mapping = Map::new();
        for (key, target) in discriminator.mapping.iter() {
            mapping.insert(Value::from(key), Value::from(target.as_str()));
        }
        map.insert(Value::from("mapping"), Value::Mapping(mapping));
    }
    Value::Mapping(map)
}

fn reference(name: &str) -> String {
    if name.starts_with("#/") {
        name.to_owned()
    } else {
        format!("#/components/schemas/{name}")
    }
}
