//! The `allOf` merge policy (API-02).
//!
//! `allOf` means "every member holds at once", which a reference page cannot
//! show as a list of boxes: a reader wants one table of properties. So the
//! members are folded into one schema under a stated policy —
//!
//! - object schemas merge by property union, and a property two members both
//!   define becomes an `allOf` of the two, merged in turn;
//! - `required` unions, and so do the two ends of every numeric, length, item,
//!   and property-count bound, taking the tighter one;
//! - `type` and `enum` intersect;
//! - a conflict — disjoint scalar `type`s, disjoint `enum`s, a minimum above a
//!   maximum — is [`Conflict`], not a silent guess;
//! - `oneOf`, `anyOf`, and `not` are never merged: a member carrying one stays
//!   a member, so the page shows it as the alternative it is.

use crate::model::{AdditionalProperties, Number, OrderedMap, Schema, SchemaType};

/// One reason a merge could not be made, with the path from the schema that
/// was merged to the keyword that disagreed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub path: Vec<String>,
    pub message: String,
}

impl Conflict {
    fn here(message: impl Into<String>) -> Self {
        Self {
            path: Vec::new(),
            message: message.into(),
        }
    }

    fn under(mut self, segment: &str) -> Self {
        self.path.insert(0, segment.to_owned());
        self
    }

    /// The pointer suffix a diagnostic appends to the schema's own pointer.
    pub fn pointer(&self) -> String {
        self.path
            .iter()
            .map(|segment| format!("/{}", segment.replace('~', "~0").replace('/', "~1")))
            .collect()
    }
}

/// Folds `allOf` into the schema that carries it, and into every schema below.
///
/// [`crate::read::Reader`] uses [`fold_here`] instead, because it has already
/// read — and so already folded — every schema below this one.
pub fn resolve(schema: &mut Schema) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    resolve_into(schema, &mut conflicts);
    conflicts
}

/// Folds this schema's own `allOf`, assuming every member is already folded.
pub fn fold_here(schema: &mut Schema) -> Vec<Conflict> {
    if schema.all_of.is_empty() {
        return Vec::new();
    }
    let mut conflicts = Vec::new();
    let members = std::mem::take(&mut schema.all_of);
    let mut kept = Vec::new();
    for member in members {
        if holds_an_alternative(&member) {
            kept.push(member);
            continue;
        }
        conflicts.extend(fold(schema, member));
    }
    schema.all_of = kept;
    conflicts
}

fn resolve_into(schema: &mut Schema, conflicts: &mut Vec<Conflict>) {
    for (name, property) in schema.properties.iter_mut() {
        let name = name.to_owned();
        let mut found = Vec::new();
        resolve_into(property, &mut found);
        conflicts.extend(
            found
                .into_iter()
                .map(|c| c.under(&name).under("properties")),
        );
    }
    for (index, member) in schema
        .one_of
        .iter_mut()
        .chain(schema.any_of.iter_mut())
        .enumerate()
    {
        let mut found = Vec::new();
        resolve_into(member, &mut found);
        conflicts.extend(found.into_iter().map(|c| c.under(&index.to_string())));
    }
    if let Some(items) = schema.items.as_mut() {
        let mut found = Vec::new();
        resolve_into(items, &mut found);
        conflicts.extend(found.into_iter().map(|c| c.under("items")));
    }
    if let AdditionalProperties::Schema(extra) = &mut schema.additional_properties {
        let mut found = Vec::new();
        resolve_into(extra, &mut found);
        conflicts.extend(found.into_iter().map(|c| c.under("additionalProperties")));
    }

    for member in &mut schema.all_of {
        resolve_into(member, conflicts);
    }
    conflicts.extend(fold_here(schema));
}

/// A member Liyasa will not fold, because folding it would lose the choice it
/// describes.
fn holds_an_alternative(member: &Schema) -> bool {
    !member.one_of.is_empty() || !member.any_of.is_empty() || member.not.is_some()
}

/// Folds one member into `base`.
fn fold(base: &mut Schema, member: Schema) -> Vec<Conflict> {
    let mut conflicts = Vec::new();

    match intersect_types(&base.types, &member.types) {
        Ok(types) => base.types = types,
        Err(message) => conflicts.push(Conflict::here(message)),
    }
    match intersect_enums(&base.enumeration, &member.enumeration) {
        Ok(values) => base.enumeration = values,
        Err(message) => conflicts.push(Conflict::here(message)),
    }
    match (&base.constant, &member.constant) {
        (Some(ours), Some(theirs)) if ours != theirs => conflicts.push(Conflict::here(
            "`allOf` members fix `const` to two different values",
        )),
        (None, Some(theirs)) => base.constant = Some(theirs.clone()),
        _ => {}
    }

    for name in member.required {
        if !base.required.contains(&name) {
            base.required.push(name);
        }
    }
    conflicts.extend(fold_properties(base, member.properties));

    base.minimum = tighter(base.minimum.take(), member.minimum, Bound::Low);
    base.exclusive_minimum = tighter(
        base.exclusive_minimum.take(),
        member.exclusive_minimum,
        Bound::Low,
    );
    base.maximum = tighter(base.maximum.take(), member.maximum, Bound::High);
    base.exclusive_maximum = tighter(
        base.exclusive_maximum.take(),
        member.exclusive_maximum,
        Bound::High,
    );
    base.min_length = base.min_length.max(member.min_length);
    base.max_length = smaller(base.max_length, member.max_length);
    base.min_items = base.min_items.max(member.min_items);
    base.max_items = smaller(base.max_items, member.max_items);
    base.min_properties = base.min_properties.max(member.min_properties);
    base.max_properties = smaller(base.max_properties, member.max_properties);
    base.unique_items |= member.unique_items;
    base.deprecated |= member.deprecated;

    if let Some(message) = contradiction(base) {
        conflicts.push(Conflict::here(message));
    }

    base.format = base.format.take().or(member.format);
    base.pattern = base.pattern.take().or(member.pattern);
    base.title = base.title.take().or(member.title);
    base.description = base.description.take().or(member.description);
    base.default = base.default.take().or(member.default);
    base.discriminator = base.discriminator.take().or(member.discriminator);
    base.external_docs = base.external_docs.take().or(member.external_docs);
    base.content_media_type = base.content_media_type.take().or(member.content_media_type);
    base.content_encoding = base.content_encoding.take().or(member.content_encoding);
    base.multiple_of = base.multiple_of.take().or(member.multiple_of);
    if base.items.is_none() {
        base.items = member.items;
    }
    for example in member.examples {
        if !base.examples.contains(&example) {
            base.examples.push(example);
        }
    }
    base.additional_properties = fold_additional(
        std::mem::take(&mut base.additional_properties),
        member.additional_properties,
    );
    for (key, value) in member.rest {
        if !base.rest.contains_key(&key) {
            base.rest.insert(key, value);
        }
    }
    conflicts
}

fn fold_properties(base: &mut Schema, members: OrderedMap<Schema>) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    for (name, theirs) in members {
        match base.properties.get(name.as_str()) {
            None => {
                base.properties.insert(name, theirs);
            }
            Some(ours) if *ours == theirs => {}
            Some(ours) => {
                // The policy's "per-property `allOf` on conflicts": the two
                // definitions become one, by the same rules, one level down.
                let mut combined = Schema {
                    all_of: vec![ours.clone(), theirs],
                    ..Schema::default()
                };
                let found = fold_here(&mut combined);
                conflicts.extend(
                    found
                        .into_iter()
                        .map(|c| c.under(&name).under("properties")),
                );
                base.properties.insert(name, combined);
            }
        }
    }
    conflicts
}

fn fold_additional(
    ours: AdditionalProperties,
    theirs: AdditionalProperties,
) -> AdditionalProperties {
    use AdditionalProperties::{Allowed, Denied, Schema as WithSchema, Unset};
    match (ours, theirs) {
        // The narrower reading wins: one member forbidding extra properties
        // forbids them for the whole.
        (Denied, _) | (_, Denied) => Denied,
        (WithSchema(ours), WithSchema(theirs)) => {
            let mut combined = crate::model::Schema {
                all_of: vec![*ours, *theirs],
                ..crate::model::Schema::default()
            };
            let _ = fold_here(&mut combined);
            WithSchema(Box::new(combined))
        }
        (WithSchema(schema), _) | (_, WithSchema(schema)) => WithSchema(schema),
        (Allowed, _) | (_, Allowed) => Allowed,
        (Unset, Unset) => Unset,
    }
}

/// `integer` is a narrowing of `number`, so the two intersect rather than
/// conflict; every other pair of scalar types is disjoint.
fn intersect_types(ours: &[SchemaType], theirs: &[SchemaType]) -> Result<Vec<SchemaType>, String> {
    if ours.is_empty() {
        return Ok(theirs.to_vec());
    }
    if theirs.is_empty() {
        return Ok(ours.to_vec());
    }
    let mut out = Vec::new();
    for ty in ours {
        let narrowed = matches!(
            (
                ty,
                theirs.contains(&SchemaType::Integer),
                theirs.contains(&SchemaType::Number)
            ),
            (SchemaType::Number, true, _) | (SchemaType::Integer, _, true)
        );
        if theirs.contains(ty) {
            out.push(*ty);
        } else if narrowed {
            out.push(SchemaType::Integer);
        }
    }
    if out.is_empty() {
        return Err(format!(
            "`allOf` members require {} and {}, which no value is at once",
            names(ours),
            names(theirs)
        ));
    }
    Ok(out)
}

fn names(types: &[SchemaType]) -> String {
    types
        .iter()
        .map(|t| format!("`{}`", t.as_str()))
        .collect::<Vec<_>>()
        .join(" or ")
}

fn intersect_enums(
    ours: &[crate::tree::Value],
    theirs: &[crate::tree::Value],
) -> Result<Vec<crate::tree::Value>, String> {
    if ours.is_empty() {
        return Ok(theirs.to_vec());
    }
    if theirs.is_empty() {
        return Ok(ours.to_vec());
    }
    let out: Vec<_> = ours
        .iter()
        .filter(|value| theirs.contains(value))
        .cloned()
        .collect();
    if out.is_empty() {
        return Err("`allOf` members list `enum` values with nothing in common".to_owned());
    }
    Ok(out)
}

#[derive(Clone, Copy)]
enum Bound {
    Low,
    High,
}

/// Keeps whichever bound is the stricter of the two.
fn tighter(ours: Option<Number>, theirs: Option<Number>, end: Bound) -> Option<Number> {
    match (ours, theirs) {
        (Some(ours), Some(theirs)) => {
            let (a, b) = (ours.as_f64().unwrap_or(0.0), theirs.as_f64().unwrap_or(0.0));
            let keep_ours = match end {
                Bound::Low => a >= b,
                Bound::High => a <= b,
            };
            Some(if keep_ours { ours } else { theirs })
        }
        (found, None) | (None, found) => found,
    }
}

fn smaller(ours: Option<u64>, theirs: Option<u64>) -> Option<u64> {
    match (ours, theirs) {
        (Some(ours), Some(theirs)) => Some(ours.min(theirs)),
        (found, None) | (None, found) => found,
    }
}

/// A range the merge closed to nothing.
fn contradiction(schema: &Schema) -> Option<String> {
    let low = schema
        .minimum
        .as_ref()
        .or(schema.exclusive_minimum.as_ref())
        .and_then(Number::as_f64);
    let high = schema
        .maximum
        .as_ref()
        .or(schema.exclusive_maximum.as_ref())
        .and_then(Number::as_f64);
    if let (Some(low), Some(high)) = (low, high)
        && low > high
    {
        return Some(format!(
            "`allOf` members leave no number: the lowest allowed is {low} and the highest is {high}"
        ));
    }
    if let (Some(low), Some(high)) = (schema.min_length, schema.max_length)
        && low > high
    {
        return Some(format!(
            "`allOf` members leave no string: `minLength` is {low} and `maxLength` is {high}"
        ));
    }
    if let (Some(low), Some(high)) = (schema.min_items, schema.max_items)
        && low > high
    {
        return Some(format!(
            "`allOf` members leave no array: `minItems` is {low} and `maxItems` is {high}"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Value;

    fn object(properties: &[(&str, Schema)], required: &[&str]) -> Schema {
        Schema {
            types: vec![SchemaType::Object],
            properties: properties
                .iter()
                .map(|(name, schema)| ((*name).to_owned(), schema.clone()))
                .collect(),
            required: required.iter().map(|name| (*name).to_owned()).collect(),
            ..Schema::default()
        }
    }

    fn string() -> Schema {
        Schema::of([SchemaType::String])
    }

    #[test]
    fn properties_union_and_required_unions_with_them() {
        let mut schema = Schema {
            all_of: vec![
                object(&[("a", string())], &["a"]),
                object(&[("b", string())], &["b"]),
            ],
            ..Schema::default()
        };
        assert_eq!(resolve(&mut schema), vec![]);
        assert_eq!(
            schema.properties.keys().collect::<Vec<_>>(),
            vec!["a", "b"],
            "the members keep their order"
        );
        assert_eq!(schema.required, vec!["a".to_owned(), "b".to_owned()]);
        assert_eq!(schema.types, vec![SchemaType::Object]);
    }

    #[test]
    fn a_property_two_members_define_becomes_a_merge_of_the_two() {
        let narrow = Schema {
            max_length: Some(10),
            ..string()
        };
        let mut schema = Schema {
            all_of: vec![
                object(&[("a", string())], &[]),
                object(&[("a", narrow)], &[]),
            ],
            ..Schema::default()
        };
        assert_eq!(resolve(&mut schema), vec![]);
        let merged = schema.properties.get("a").expect("the property survives");
        assert_eq!(merged.max_length, Some(10));
        assert_eq!(merged.types, vec![SchemaType::String]);
    }

    #[test]
    fn two_scalar_types_that_no_value_satisfies_are_a_conflict() {
        let mut schema = Schema {
            all_of: vec![
                Schema::of([SchemaType::String]),
                Schema::of([SchemaType::Integer]),
            ],
            ..Schema::default()
        };
        let conflicts = resolve(&mut schema);
        assert_eq!(conflicts.len(), 1);
        assert!(
            conflicts[0].message.contains("`string`"),
            "{:?}",
            conflicts[0]
        );
        assert_eq!(conflicts[0].pointer(), "");
    }

    #[test]
    fn integer_narrows_number_rather_than_conflicting_with_it() {
        let mut schema = Schema {
            all_of: vec![
                Schema::of([SchemaType::Number]),
                Schema::of([SchemaType::Integer]),
            ],
            ..Schema::default()
        };
        assert_eq!(resolve(&mut schema), vec![]);
        assert_eq!(schema.types, vec![SchemaType::Integer]);
    }

    #[test]
    fn a_conflict_inside_a_property_carries_the_path_to_it() {
        let mut schema = Schema {
            all_of: vec![
                object(&[("a", Schema::of([SchemaType::String]))], &[]),
                object(&[("a", Schema::of([SchemaType::Boolean]))], &[]),
            ],
            ..Schema::default()
        };
        let conflicts = resolve(&mut schema);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].pointer(), "/properties/a");
    }

    #[test]
    fn enums_intersect_and_disjoint_ones_are_a_conflict() {
        let listing = |values: &[&str]| Schema {
            enumeration: values.iter().map(|v| Value::from(*v)).collect(),
            ..string()
        };
        let mut shared = Schema {
            all_of: vec![listing(&["a", "b"]), listing(&["b", "c"])],
            ..Schema::default()
        };
        assert_eq!(resolve(&mut shared), vec![]);
        assert_eq!(shared.enumeration, vec![Value::from("b")]);

        let mut disjoint = Schema {
            all_of: vec![listing(&["a"]), listing(&["b"])],
            ..Schema::default()
        };
        let conflicts = resolve(&mut disjoint);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].message.contains("enum"), "{:?}", conflicts[0]);
    }

    #[test]
    fn bounds_take_the_tighter_end_and_an_empty_range_is_a_conflict() {
        let bounded = |min: i64, max: i64| Schema {
            minimum: Some(min.into()),
            maximum: Some(max.into()),
            ..Schema::of([SchemaType::Integer])
        };
        let mut ok = Schema {
            all_of: vec![bounded(0, 100), bounded(10, 50)],
            ..Schema::default()
        };
        assert_eq!(resolve(&mut ok), vec![]);
        assert_eq!(ok.minimum.as_ref().and_then(Number::as_i64), Some(10));
        assert_eq!(ok.maximum.as_ref().and_then(Number::as_i64), Some(50));

        let mut empty = Schema {
            all_of: vec![bounded(60, 100), bounded(0, 50)],
            ..Schema::default()
        };
        let conflicts = resolve(&mut empty);
        assert_eq!(conflicts.len(), 1);
        assert!(
            conflicts[0].message.contains("no number"),
            "{:?}",
            conflicts[0]
        );
    }

    #[test]
    fn a_member_carrying_one_of_is_left_as_a_member() {
        let alternative = Schema {
            one_of: vec![string(), Schema::of([SchemaType::Integer])],
            ..Schema::default()
        };
        let mut schema = Schema {
            all_of: vec![object(&[("a", string())], &[]), alternative],
            ..Schema::default()
        };
        assert_eq!(resolve(&mut schema), vec![]);
        assert_eq!(schema.all_of.len(), 1, "the alternative stayed a member");
        assert_eq!(schema.all_of[0].one_of.len(), 2);
        assert!(
            schema.properties.get("a").is_some(),
            "the object still merged"
        );
    }

    #[test]
    fn one_member_forbidding_extra_properties_forbids_them_for_the_whole() {
        let mut schema = Schema {
            all_of: vec![
                Schema {
                    additional_properties: AdditionalProperties::Allowed,
                    ..object(&[], &[])
                },
                Schema {
                    additional_properties: AdditionalProperties::Denied,
                    ..object(&[], &[])
                },
            ],
            ..Schema::default()
        };
        assert_eq!(resolve(&mut schema), vec![]);
        assert_eq!(schema.additional_properties, AdditionalProperties::Denied);
    }

    #[test]
    fn a_nested_all_of_inside_a_property_is_merged_too() {
        let mut schema = object(
            &[(
                "a",
                Schema {
                    all_of: vec![
                        string(),
                        Schema {
                            max_length: Some(4),
                            ..Schema::default()
                        },
                    ],
                    ..Schema::default()
                },
            )],
            &[],
        );
        assert_eq!(resolve(&mut schema), vec![]);
        let property = schema.properties.get("a").expect("the property is there");
        assert!(property.all_of.is_empty(), "the nested allOf was folded");
        assert_eq!(property.max_length, Some(4));
        assert_eq!(property.types, vec![SchemaType::String]);
    }
}
