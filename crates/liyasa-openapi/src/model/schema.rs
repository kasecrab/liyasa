//! The schema model: JSON Schema 2020-12 as OpenAPI 3.1 uses it (API-01).
//!
//! 3.0's `nullable`, boolean `exclusiveMinimum`, and single `type` are gone by
//! the time a schema reaches this type; see [`crate::normalize`].

use serde::Serialize;

use super::map::OrderedMap;
use super::{Extensions, ExternalDocs};
use crate::tree::Value;

pub type Number = serde_norway::Number;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaType {
    Null,
    Boolean,
    Object,
    Array,
    Number,
    Integer,
    String,
}

impl SchemaType {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "null" => Self::Null,
            "boolean" => Self::Boolean,
            "object" => Self::Object,
            "array" => Self::Array,
            "number" => Self::Number,
            "integer" => Self::Integer,
            "string" => Self::String,
            _ => return None,
        })
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Boolean => "boolean",
            Self::Object => "object",
            Self::Array => "array",
            Self::Number => "number",
            Self::Integer => "integer",
            Self::String => "string",
        }
    }
}

/// `additionalProperties`, whose absence and whose `true` mean the same thing
/// to a validator but not to a reference page: only the written form is shown.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AdditionalProperties {
    #[default]
    Unset,
    Allowed,
    Denied,
    Schema(Box<Schema>),
}

impl AdditionalProperties {
    pub fn schema(&self) -> Option<&Schema> {
        match self {
            Self::Schema(schema) => Some(schema),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Discriminator {
    pub property_name: String,
    /// Value to schema name or `$ref`, in the order the spec wrote it.
    #[serde(skip_serializing_if = "OrderedMap::is_empty")]
    pub mapping: OrderedMap<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Xml {
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub prefix: Option<String>,
    pub attribute: bool,
    pub wrapped: bool,
}

/// One schema. Every field that a reference page can show has a home here;
/// keywords that only a validator cares about ride along in [`Schema::rest`].
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    /// Empty means "any type", which 2020-12 spells by omitting `type`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<SchemaType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<Value>,
    pub deprecated: bool,
    pub read_only: bool,
    pub write_only: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub enumeration: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constant: Option<Value>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub all_of: Vec<Schema>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub one_of: Vec<Schema>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub any_of: Vec<Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not: Option<Box<Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<Discriminator>,

    #[serde(skip_serializing_if = "OrderedMap::is_empty")]
    pub properties: OrderedMap<Schema>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
    pub additional_properties: AdditionalProperties,
    #[serde(skip_serializing_if = "OrderedMap::is_empty")]
    pub pattern_properties: OrderedMap<Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_names: Option<Box<Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_properties: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_properties: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prefix_items: Vec<Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u64>,
    pub unique_items: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_encoding: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusive_maximum: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiple_of: Option<Number>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xml: Option<Xml>,
    /// The component this schema was written as, when it was written as one:
    /// `#/components/schemas/User`. Set when a `$ref` was replaced, so a page
    /// can link to the schema page (API-15) and a cycle can be cut (API-11).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Keywords Liyasa does not model, kept so the processed download (API-50)
    /// stays faithful.
    #[serde(skip_serializing_if = "OrderedMap::is_empty")]
    pub rest: OrderedMap<Value>,
    #[serde(skip_serializing_if = "Extensions::is_empty")]
    pub extensions: Extensions,
}

impl Schema {
    pub fn of(types: impl IntoIterator<Item = SchemaType>) -> Self {
        Self {
            types: types.into_iter().collect(),
            ..Self::default()
        }
    }

    /// True when `null` is one of the permitted types. 3.0's `nullable: true`
    /// arrives here as `types` containing `Null`.
    pub fn is_nullable(&self) -> bool {
        self.types.contains(&SchemaType::Null)
    }

    /// The types a reader is shown, which is every type except the `null` that
    /// nullability already communicates.
    pub fn shown_types(&self) -> impl Iterator<Item = SchemaType> + '_ {
        self.types
            .iter()
            .copied()
            .filter(|t| *t != SchemaType::Null)
    }

    pub fn is(&self, ty: SchemaType) -> bool {
        self.types.contains(&ty)
    }

    /// Nothing is written in it at all: a `{}` or a `true` schema.
    pub fn is_any(&self) -> bool {
        *self == Self::default()
    }

    /// A schema the reader cut a cycle at: it names the component it stands
    /// for and says nothing else, so a page shows an expand control rather
    /// than an empty object (API-11).
    pub fn is_stub(&self) -> bool {
        self.name.is_some()
            && self.types.is_empty()
            && self.properties.is_empty()
            && self.pattern_properties.is_empty()
            && self.all_of.is_empty()
            && self.one_of.is_empty()
            && self.any_of.is_empty()
            && self.items.is_none()
            && self.enumeration.is_empty()
            && self.constant.is_none()
            && self.title.is_none()
            && self.description.is_none()
            && self.additional_properties == AdditionalProperties::Unset
    }

    pub fn variants(&self) -> &[Schema] {
        if !self.one_of.is_empty() {
            &self.one_of
        } else {
            &self.any_of
        }
    }

    pub fn is_required(&self, property: &str) -> bool {
        self.required.iter().any(|name| name == property)
    }
}
