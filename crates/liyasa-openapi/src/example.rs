//! Example values derived from a schema (API-31).
//!
//! A sample is only useful if it is filled in, so every value a page shows —
//! the request body, a parameter's placeholder, a response body with no
//! example of its own — comes from here. What the spec wrote always wins; the
//! synthesized value is the fallback, and it is deterministic, because a
//! sample that changes between builds is a diff on every deploy.

use crate::model::{Number, Schema, SchemaType};
use crate::tree::{Map, Value};

/// How deep a synthesized value goes before it stops. A recursive schema is
/// cut with `null` rather than expanded (API-11).
const DEPTH: usize = 6;

/// Which of a schema's two sides a value is for: a `readOnly` property has no
/// place in a request and a `writeOnly` one has none in a response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Request,
    Response,
}

impl Side {
    fn keeps(self, schema: &Schema) -> bool {
        match self {
            Self::Request => !schema.read_only,
            Self::Response => !schema.write_only,
        }
    }
}

/// What a generated value includes when a schema's properties are optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    /// Only what the schema marks `required` (API-31's required-only toggle).
    Required,
    All,
}

/// Builds one example value for `schema`.
pub fn of(schema: &Schema, side: Side, fill: Fill) -> Value {
    build(schema, side, fill, DEPTH)
}

/// The value a spec wrote for this schema, if it wrote one.
pub fn written(schema: &Schema) -> Option<Value> {
    schema
        .examples
        .first()
        .or(schema.constant.as_ref())
        .or(schema.default.as_ref())
        .or_else(|| schema.enumeration.first())
        .cloned()
}

fn build(schema: &Schema, side: Side, fill: Fill, depth: usize) -> Value {
    if let Some(written) = written(schema) {
        return written;
    }
    if depth == 0 {
        return Value::Null;
    }
    // A cycle the reader cut: there is nothing left to build from, and an
    // empty object is closer to the truth than `null`.
    if schema.is_stub() {
        return Value::Mapping(Map::new());
    }
    // A choice is shown by its first alternative; the page's variant selector
    // is what offers the rest (API-11).
    if let Some(first) = schema.variants().first() {
        return build(first, side, fill, depth - 1);
    }
    match chosen_type(schema) {
        Some(SchemaType::Object) => object(schema, side, fill, depth),
        Some(SchemaType::Array) => array(schema, side, fill, depth),
        Some(SchemaType::String) => Value::String(string(schema)),
        Some(SchemaType::Integer) => Value::Number(integer(schema)),
        Some(SchemaType::Number) => Value::Number(fractional(schema)),
        Some(SchemaType::Boolean) => Value::Bool(true),
        Some(SchemaType::Null) | None => Value::Null,
    }
}

/// The type a value is built for: the first the schema names that is not
/// `null`, because nullability is not a shape.
fn chosen_type(schema: &Schema) -> Option<SchemaType> {
    if let Some(found) = schema.shown_types().next() {
        return Some(found);
    }
    if schema.types.contains(&SchemaType::Null) {
        return Some(SchemaType::Null);
    }
    // No `type`, but shaped like one of them anyway.
    if !schema.properties.is_empty() {
        return Some(SchemaType::Object);
    }
    if schema.items.is_some() {
        return Some(SchemaType::Array);
    }
    None
}

fn object(schema: &Schema, side: Side, fill: Fill, depth: usize) -> Value {
    let mut map = Map::new();
    for (name, property) in schema.properties.iter() {
        if !side.keeps(property) {
            continue;
        }
        if fill == Fill::Required && !schema.is_required(name) {
            continue;
        }
        map.insert(
            Value::String(name.to_owned()),
            build(property, side, fill, depth - 1),
        );
    }
    if map.is_empty()
        && let Some(extra) = schema.additional_properties.schema()
    {
        map.insert(
            Value::String("key".to_owned()),
            build(extra, side, fill, depth - 1),
        );
    }
    Value::Mapping(map)
}

fn array(schema: &Schema, side: Side, fill: Fill, depth: usize) -> Value {
    let Some(items) = schema.items.as_deref() else {
        return Value::Sequence(Vec::new());
    };
    if items.is_stub() {
        return Value::Sequence(Vec::new());
    }
    let count = schema.min_items.unwrap_or(1).clamp(1, 2) as usize;
    Value::Sequence(
        (0..count)
            .map(|_| build(items, side, fill, depth - 1))
            .collect(),
    )
}

/// A placeholder that looks like what the format promises, so a reader
/// pasting the sample sees the right shape rather than the word "string".
fn string(schema: &Schema) -> String {
    let placeholder = match schema.format.as_deref() {
        Some("date-time") => "2026-01-01T00:00:00Z",
        Some("date") => "2026-01-01",
        Some("time") => "00:00:00",
        Some("duration") => "P1D",
        Some("email" | "idn-email") => "user@example.com",
        Some("hostname" | "idn-hostname") => "example.com",
        Some("ipv4") => "192.0.2.1",
        Some("ipv6") => "2001:db8::1",
        Some("uri" | "iri" | "uri-reference") => "https://example.com",
        Some("uuid") => "00000000-0000-0000-0000-000000000000",
        Some("byte") => "ZXhhbXBsZQ==",
        Some("binary") => "file.bin",
        Some("password") => "password",
        Some("regex") => ".*",
        _ => "",
    };
    if !placeholder.is_empty() {
        return placeholder.to_owned();
    }
    if let Some(pattern) = &schema.pattern {
        return format!("<{pattern}>");
    }
    let base = schema.title.clone().unwrap_or_else(|| "string".to_owned());
    match schema.min_length {
        // Pad rather than return something the schema itself rejects.
        Some(min) if (base.len() as u64) < min => {
            let mut out = base;
            while (out.len() as u64) < min {
                out.push('x');
            }
            out
        }
        _ => base,
    }
}

fn integer(schema: &Schema) -> Number {
    let low = bound(schema.minimum.as_ref())
        .or_else(|| bound(schema.exclusive_minimum.as_ref()).map(|n| n + 1.0));
    let high = bound(schema.maximum.as_ref())
        .or_else(|| bound(schema.exclusive_maximum.as_ref()).map(|n| n - 1.0));
    let value = match (low, high) {
        (Some(low), _) => low,
        (None, Some(high)) if high < 1.0 => high,
        _ => 1.0,
    };
    Number::from(value as i64)
}

fn fractional(schema: &Schema) -> Number {
    let low = bound(schema.minimum.as_ref()).or_else(|| bound(schema.exclusive_minimum.as_ref()));
    match low {
        Some(low) => Number::from(low),
        None => Number::from(1.0),
    }
}

fn bound(number: Option<&Number>) -> Option<f64> {
    number.and_then(Number::as_f64)
}

/// The value written into a URL or a header, which is the scalar itself rather
/// than its JSON spelling: a string parameter is `abc`, not `"abc"`.
pub fn as_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Null => String::new(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OrderedMap;

    fn object_of(properties: &[(&str, Schema)], required: &[&str]) -> Schema {
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
    fn what_the_spec_wrote_beats_anything_synthesized() {
        let schema = Schema {
            examples: vec![Value::from("written")],
            ..Schema::of([SchemaType::String])
        };
        assert_eq!(
            of(&schema, Side::Request, Fill::All),
            Value::from("written")
        );
    }

    #[test]
    fn the_first_enum_value_stands_in_for_the_type() {
        let schema = Schema {
            enumeration: vec![Value::from("active"), Value::from("archived")],
            ..Schema::of([SchemaType::String])
        };
        assert_eq!(of(&schema, Side::Request, Fill::All), Value::from("active"));
    }

    #[test]
    fn a_format_produces_a_placeholder_of_that_shape() {
        let dated = Schema {
            format: Some("date-time".to_owned()),
            ..Schema::of([SchemaType::String])
        };
        assert_eq!(
            of(&dated, Side::Request, Fill::All),
            Value::from("2026-01-01T00:00:00Z")
        );
    }

    #[test]
    fn required_only_leaves_the_optional_properties_out() {
        let schema = object_of(
            &[
                ("id", Schema::of([SchemaType::String])),
                ("note", Schema::of([SchemaType::String])),
            ],
            &["id"],
        );
        let required = of(&schema, Side::Request, Fill::Required);
        assert!(crate::tree::field(&required, "id").is_some());
        assert!(crate::tree::field(&required, "note").is_none());

        let all = of(&schema, Side::Request, Fill::All);
        assert!(crate::tree::field(&all, "note").is_some());
    }

    #[test]
    fn a_read_only_property_is_left_out_of_a_request_and_kept_in_a_response() {
        let schema = object_of(
            &[
                (
                    "id",
                    Schema {
                        read_only: true,
                        ..Schema::of([SchemaType::String])
                    },
                ),
                ("name", Schema::of([SchemaType::String])),
            ],
            &["id", "name"],
        );
        assert!(crate::tree::field(&of(&schema, Side::Request, Fill::All), "id").is_none());
        assert!(crate::tree::field(&of(&schema, Side::Response, Fill::All), "id").is_some());
    }

    #[test]
    fn a_recursive_schema_stops_rather_than_running_away() {
        let mut node = object_of(&[("name", Schema::of([SchemaType::String]))], &["name"]);
        let mut nested = node.clone();
        for _ in 0..40 {
            let mut outer = node.clone();
            outer.properties.insert("child", nested.clone());
            nested = outer.clone();
            node = outer;
        }
        let value = of(&node, Side::Request, Fill::All);
        assert!(
            matches!(value, Value::Mapping(_)),
            "it produced a value at all"
        );
    }

    #[test]
    fn a_minimum_is_the_integer_a_sample_uses() {
        let schema = Schema {
            minimum: Some(10.into()),
            ..Schema::of([SchemaType::Integer])
        };
        assert_eq!(of(&schema, Side::Request, Fill::All), Value::from(10));
    }

    #[test]
    fn an_exclusive_minimum_steps_past_itself() {
        let schema = Schema {
            exclusive_minimum: Some(0.into()),
            ..Schema::of([SchemaType::Integer])
        };
        assert_eq!(of(&schema, Side::Request, Fill::All), Value::from(1));
    }

    #[test]
    fn a_one_of_is_shown_as_its_first_alternative() {
        let schema = Schema {
            one_of: vec![
                Schema {
                    examples: vec![Value::from("first")],
                    ..Schema::of([SchemaType::String])
                },
                Schema::of([SchemaType::Integer]),
            ],
            ..Schema::default()
        };
        assert_eq!(of(&schema, Side::Request, Fill::All), Value::from("first"));
    }

    #[test]
    fn an_array_holds_one_item_unless_the_schema_asks_for_more() {
        let schema = Schema {
            items: Some(Box::new(Schema::of([SchemaType::String]))),
            min_items: Some(2),
            ..Schema::of([SchemaType::Array])
        };
        let Value::Sequence(items) = of(&schema, Side::Request, Fill::All) else {
            panic!("an array schema makes an array");
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn a_scalar_in_a_url_is_written_without_json_quoting() {
        assert_eq!(as_text(&Value::from("abc")), "abc");
        assert_eq!(as_text(&Value::from(3)), "3");
        assert_eq!(as_text(&Value::from(true)), "true");
    }
}
