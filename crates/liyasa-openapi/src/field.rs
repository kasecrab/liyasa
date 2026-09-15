//! One row of a parameter or schema table (API-10, API-11).
//!
//! A reference page is mostly this type repeated: a name, what it accepts,
//! whether it has to be there, and — when it is an object or a choice — the
//! rows underneath it. Everything a renderer needs is decided here so that the
//! HTML, the Markdown (API-14), and the search text agree.

use serde::Serialize;

use crate::example;
use crate::model::{AdditionalProperties, Number, Parameter, Schema, SchemaType, slug};
use crate::tree::Value;

/// How far a nested object is expanded before the row is marked
/// [`Field::truncated`] and the reader is offered an expand control (API-11).
pub const DEPTH: usize = 5;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    /// The anchor a link to this row uses (API-10).
    pub anchor: String,
    /// `string`, `integer`, `array of string`, `object`.
    pub type_label: String,
    /// `int64`, `date-time`: the format, when the spec names one.
    pub format: Option<String>,
    pub required: bool,
    pub deprecated: bool,
    /// `null` is among the permitted types (API-11).
    pub nullable: bool,
    pub read_only: bool,
    pub write_only: bool,
    pub description: Option<String>,
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub enumeration: Vec<Value>,
    /// `at least 1`, `up to 20 characters`, and the rest, already in words.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    pub example: Option<Value>,
    /// The rows of an object, or of an array's items.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Field>,
    /// The alternatives of a `oneOf` or `anyOf` (API-11).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<Variant>,
    /// The component this row's schema was written as, which is what a schema
    /// page links to and what an expand control names (API-15).
    pub schema_name: Option<String>,
    /// The depth limit stopped the expansion here.
    pub truncated: bool,
}

/// One alternative of a choice, labelled by its discriminator value when the
/// spec supplies one.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    pub label: String,
    pub field: Box<Field>,
}

impl Field {
    /// The row for one parameter.
    pub fn of_parameter(parameter: &Parameter) -> Self {
        let schema = parameter.schema.clone().unwrap_or_else(|| {
            parameter
                .content
                .values()
                .next()
                .and_then(|media| media.schema.clone())
                .unwrap_or_default()
        });
        let mut field = Self::of_schema(&parameter.name, &schema, parameter.required, DEPTH);
        field.anchor = parameter.anchor();
        field.deprecated |= parameter.deprecated;
        if parameter.description.is_some() {
            field.description = parameter.description.clone();
        }
        if let Some(hint) = &parameter.liyasa.description {
            field.description = Some(hint.clone());
        }
        if let Some(title) = &parameter.liyasa.title {
            field.name = title.clone();
        }
        if field.example.is_none() {
            field.example = parameter
                .example
                .clone()
                .or_else(|| parameter.examples.values().find_map(|e| e.value.clone()));
        }
        field
    }

    /// The row for one schema, named.
    pub fn of_schema(name: &str, schema: &Schema, required: bool, depth: usize) -> Self {
        let mut field = Self {
            name: name.to_owned(),
            anchor: slug(name),
            type_label: type_label(schema),
            format: schema.format.clone(),
            required,
            deprecated: schema.deprecated,
            nullable: schema.is_nullable(),
            read_only: schema.read_only,
            write_only: schema.write_only,
            description: schema.description.clone(),
            default: schema.default.clone(),
            enumeration: schema
                .enumeration
                .iter()
                .filter(|value| **value != Value::Null)
                .cloned()
                .collect(),
            constraints: constraints(schema),
            example: example::written(schema),
            children: Vec::new(),
            variants: Vec::new(),
            schema_name: schema.name.clone(),
            truncated: false,
        };

        if schema.is_stub() {
            // The reader cut a cycle here; the name is the expand control.
            field.truncated = true;
            return field;
        }
        if depth == 0 {
            // Stop, but say what would have been here, so the reader interface
            // can offer to fetch it.
            field.truncated = has_children(schema);
            return field;
        }

        let variants = schema.variants();
        if !variants.is_empty() {
            field.variants = variants
                .iter()
                .enumerate()
                .map(|(index, variant)| Variant {
                    label: variant_label(schema, variant, index),
                    field: Box::new(Self::of_schema(name, variant, required, depth - 1)),
                })
                .collect();
            return field;
        }

        if schema.is(SchemaType::Array)
            && let Some(items) = schema.items.as_deref()
        {
            field.children = object_rows(items, depth - 1);
            if field.children.is_empty() && (has_children(items) || items.is_stub()) {
                field.truncated = true;
                field.schema_name = field.schema_name.take().or_else(|| items.name.clone());
            }
            return field;
        }
        field.children = object_rows(schema, depth - 1);
        field
    }

    /// Every row underneath this one, flattened, which is what search indexes
    /// and what the Markdown table lists.
    pub fn flatten(&self) -> Vec<&Field> {
        let mut out = vec![self];
        for child in &self.children {
            out.extend(child.flatten());
        }
        for variant in &self.variants {
            out.extend(variant.field.flatten());
        }
        out
    }
}

/// The rows of an object schema, in the order its properties are written.
fn object_rows(schema: &Schema, depth: usize) -> Vec<Field> {
    let mut rows: Vec<Field> = schema
        .properties
        .iter()
        .map(|(name, property)| Field::of_schema(name, property, schema.is_required(name), depth))
        .collect();
    for (pattern, property) in schema.pattern_properties.iter() {
        rows.push(Field::of_schema(
            &format!("/{pattern}/"),
            property,
            false,
            depth,
        ));
    }
    if let AdditionalProperties::Schema(extra) = &schema.additional_properties {
        rows.push(Field::of_schema("*", extra, false, depth));
    }
    rows
}

fn has_children(schema: &Schema) -> bool {
    !schema.properties.is_empty()
        || !schema.pattern_properties.is_empty()
        || !schema.variants().is_empty()
        || schema.additional_properties.schema().is_some()
        || schema
            .items
            .as_deref()
            .is_some_and(|items| !items.properties.is_empty() || !items.variants().is_empty())
}

/// `oneOf` variants are labelled by the discriminator's mapping when there is
/// one, then by the variant's own name or title, and only then by position.
fn variant_label(parent: &Schema, variant: &Schema, index: usize) -> String {
    if let Some(discriminator) = &parent.discriminator {
        let by_mapping = discriminator.mapping.iter().find_map(|(value, target)| {
            let named = target.rsplit('/').next().unwrap_or(target);
            (Some(named) == variant.name.as_deref()).then(|| value.to_owned())
        });
        if let Some(label) = by_mapping {
            return label;
        }
    }
    variant
        .name
        .clone()
        .or_else(|| variant.title.clone())
        .unwrap_or_else(|| {
            let ty = type_label(variant);
            if ty == "any" {
                format!("Option {}", index + 1)
            } else {
                ty
            }
        })
}

/// What a row says the value is. `null` is left out: nullability is its own
/// column, not a type a reader has to parse out of a union.
pub fn type_label(schema: &Schema) -> String {
    if schema.is_stub()
        && let Some(name) = &schema.name
    {
        return name.clone();
    }
    let shown: Vec<SchemaType> = schema.shown_types().collect();
    match shown.as_slice() {
        [] => {
            if !schema.variants().is_empty() {
                "one of".to_owned()
            } else if !schema.properties.is_empty() {
                "object".to_owned()
            } else if schema.items.is_some() {
                array_label(schema)
            } else if schema.constant.is_some() {
                "const".to_owned()
            } else if !schema.enumeration.is_empty() {
                "enum".to_owned()
            } else {
                "any".to_owned()
            }
        }
        [SchemaType::Array] => array_label(schema),
        [one] => one.as_str().to_owned(),
        many => many
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(" or "),
    }
}

fn array_label(schema: &Schema) -> String {
    match schema.items.as_deref() {
        Some(items) => {
            let inner = type_label(items);
            if inner == "any" {
                "array".to_owned()
            } else {
                format!("array of {inner}")
            }
        }
        None => "array".to_owned(),
    }
}

/// The schema's bounds, in the words a reader reads rather than the keywords a
/// validator does.
pub fn constraints(schema: &Schema) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(min) = &schema.minimum {
        out.push(format!("at least {}", number(min)));
    }
    if let Some(min) = &schema.exclusive_minimum {
        out.push(format!("greater than {}", number(min)));
    }
    if let Some(max) = &schema.maximum {
        out.push(format!("at most {}", number(max)));
    }
    if let Some(max) = &schema.exclusive_maximum {
        out.push(format!("less than {}", number(max)));
    }
    if let Some(step) = &schema.multiple_of {
        out.push(format!("a multiple of {}", number(step)));
    }
    match (schema.min_length, schema.max_length) {
        (Some(min), Some(max)) if min == max => out.push(format!("exactly {min} characters")),
        (Some(min), Some(max)) => out.push(format!("{min} to {max} characters")),
        (Some(min), None) => out.push(format!("at least {min} characters")),
        (None, Some(max)) => out.push(format!("up to {max} characters")),
        (None, None) => {}
    }
    match (schema.min_items, schema.max_items) {
        (Some(min), Some(max)) if min == max => out.push(format!("exactly {min} items")),
        (Some(min), Some(max)) => out.push(format!("{min} to {max} items")),
        (Some(min), None) => out.push(format!("at least {min} items")),
        (None, Some(max)) => out.push(format!("up to {max} items")),
        (None, None) => {}
    }
    if schema.unique_items {
        out.push("items are unique".to_owned());
    }
    if let Some(pattern) = &schema.pattern {
        out.push(format!("matches `{pattern}`"));
    }
    if schema.additional_properties == AdditionalProperties::Denied {
        out.push("no other properties".to_owned());
    }
    out
}

/// A number without the trailing `.0` an integer written as a float picks up.
fn number(value: &Number) -> String {
    match value.as_i64() {
        Some(whole) => whole.to_string(),
        None => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Discriminator, OrderedMap, ParameterIn};

    fn object(properties: &[(&str, Schema)], required: &[&str]) -> Schema {
        Schema {
            types: vec![SchemaType::Object],
            properties: properties
                .iter()
                .map(|(name, schema)| ((*name).to_owned(), schema.clone()))
                .collect::<OrderedMap<_>>(),
            required: required.iter().map(|n| (*n).to_owned()).collect(),
            ..Schema::default()
        }
    }

    #[test]
    fn a_type_array_with_null_shows_the_type_and_the_nullability_apart() {
        let schema = Schema::of([SchemaType::String, SchemaType::Null]);
        let field = Field::of_schema("a", &schema, false, DEPTH);
        assert_eq!(field.type_label, "string");
        assert!(field.nullable);
    }

    #[test]
    fn a_union_of_two_real_types_is_spelled_out() {
        let schema = Schema::of([SchemaType::String, SchemaType::Integer, SchemaType::Null]);
        assert_eq!(type_label(&schema), "string or integer");
    }

    #[test]
    fn an_array_says_what_it_holds() {
        let schema = Schema {
            items: Some(Box::new(Schema::of([SchemaType::String]))),
            ..Schema::of([SchemaType::Array])
        };
        assert_eq!(type_label(&schema), "array of string");
    }

    #[test]
    fn a_nested_object_becomes_child_rows_in_the_order_it_declares() {
        let schema = object(
            &[
                ("id", Schema::of([SchemaType::String])),
                (
                    "owner",
                    object(&[("name", Schema::of([SchemaType::String]))], &["name"]),
                ),
            ],
            &["id"],
        );
        let field = Field::of_schema("widget", &schema, true, DEPTH);
        assert_eq!(
            field
                .children
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["id", "owner"]
        );
        assert!(field.children[0].required);
        assert!(!field.children[1].required);
        assert_eq!(field.children[1].children[0].name, "name");
    }

    #[test]
    fn recursion_stops_at_the_depth_limit_and_says_so() {
        let mut schema = object(&[("leaf", Schema::of([SchemaType::String]))], &[]);
        for _ in 0..DEPTH + 3 {
            schema = object(&[("child", schema)], &[]);
        }
        let field = Field::of_schema("root", &schema, false, DEPTH);
        let deepest = field
            .flatten()
            .into_iter()
            .find(|f| f.truncated)
            .expect("something was truncated");
        assert!(deepest.children.is_empty());
    }

    #[test]
    fn a_one_of_becomes_labelled_variants() {
        let cat = Schema {
            name: Some("Cat".to_owned()),
            ..object(&[("meow", Schema::of([SchemaType::Boolean]))], &[])
        };
        let dog = Schema {
            name: Some("Dog".to_owned()),
            ..object(&[("bark", Schema::of([SchemaType::Boolean]))], &[])
        };
        let schema = Schema {
            one_of: vec![cat, dog],
            discriminator: Some(Discriminator {
                property_name: "kind".to_owned(),
                mapping: [
                    ("cat".to_owned(), "#/components/schemas/Cat".to_owned()),
                    ("dog".to_owned(), "#/components/schemas/Dog".to_owned()),
                ]
                .into_iter()
                .collect(),
            }),
            ..Schema::default()
        };
        let field = Field::of_schema("pet", &schema, true, DEPTH);
        assert_eq!(
            field
                .variants
                .iter()
                .map(|v| v.label.as_str())
                .collect::<Vec<_>>(),
            vec!["cat", "dog"],
            "the discriminator's own words label the variants"
        );
        assert_eq!(field.type_label, "one of");
    }

    #[test]
    fn a_variant_with_no_discriminator_is_labelled_by_its_component_name() {
        let schema = Schema {
            one_of: vec![
                Schema {
                    name: Some("Card".to_owned()),
                    ..Schema::of([SchemaType::Object])
                },
                Schema::of([SchemaType::String]),
            ],
            ..Schema::default()
        };
        let field = Field::of_schema("payment", &schema, false, DEPTH);
        assert_eq!(
            field
                .variants
                .iter()
                .map(|v| v.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Card", "string"]
        );
    }

    #[test]
    fn constraints_read_as_sentences() {
        let schema = Schema {
            minimum: Some(1.into()),
            maximum: Some(10.into()),
            min_length: Some(2),
            max_length: Some(2),
            pattern: Some("^a".to_owned()),
            unique_items: true,
            ..Schema::of([SchemaType::String])
        };
        assert_eq!(
            constraints(&schema),
            vec![
                "at least 1".to_owned(),
                "at most 10".to_owned(),
                "exactly 2 characters".to_owned(),
                "items are unique".to_owned(),
                "matches `^a`".to_owned(),
            ]
        );
    }

    #[test]
    fn a_parameters_anchor_names_its_location_so_two_places_can_share_a_name() {
        let query = Parameter {
            name: "id".to_owned(),
            location: ParameterIn::Query,
            ..Parameter::default()
        };
        let path = Parameter {
            name: "id".to_owned(),
            location: ParameterIn::Path,
            required: true,
            ..Parameter::default()
        };
        assert_eq!(Field::of_parameter(&query).anchor, "query-id");
        assert_eq!(Field::of_parameter(&path).anchor, "path-id");
    }

    #[test]
    fn additional_properties_become_a_row_of_their_own() {
        let schema = Schema {
            additional_properties: AdditionalProperties::Schema(Box::new(Schema::of([
                SchemaType::Integer,
            ]))),
            ..object(&[("a", Schema::of([SchemaType::String]))], &[])
        };
        let field = Field::of_schema("counts", &schema, false, DEPTH);
        assert_eq!(
            field
                .children
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "*"]
        );
        assert_eq!(field.children[1].type_label, "integer");
    }

    #[test]
    fn a_null_in_an_enum_is_shown_as_nullability_rather_than_as_a_value() {
        let schema = Schema {
            types: vec![SchemaType::String, SchemaType::Null],
            enumeration: vec![Value::from("a"), Value::Null],
            ..Schema::default()
        };
        let field = Field::of_schema("state", &schema, false, DEPTH);
        assert_eq!(field.enumeration, vec![Value::from("a")]);
        assert!(field.nullable);
    }
}
