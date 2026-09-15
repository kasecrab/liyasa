//! The document tree every stage of the loader works on.
//!
//! An OpenAPI document is read once into an order-preserving value tree and
//! stays in it through `$ref` splicing, overlays, and the processed download
//! (API-50). Key order is part of the rendered page — the properties of a body,
//! the parameters of an operation — so the tree may not sort.
//!
//! This is the only module that names the YAML crate; see
//! `plan/rfcs/0800-ordered-document-tree.md` for why it is not
//! `liyasa_core::yaml`. JSON is a subset of YAML 1.2, so one parser reads both
//! spec encodings.

// TODO(rfc-0800): pending core growing an order-preserving value type.
use liyasa_core::diagnostics::{Diagnostic, code};

use crate::SpecError;

pub type Value = serde_norway::Value;
pub type Map = serde_norway::Mapping;

/// Parses a spec's bytes. `origin` names the document in any diagnostic.
///
/// A document that fails is parsed a second time by [`lenient`], which accepts
/// an integer too large for the tree's `i64`/`u64` and keeps it as a float
/// (RFC 0804). The first pass is unchanged, so a document that parses today
/// parses by the same path it always did.
pub fn parse(bytes: &[u8], origin: &str) -> Result<Value, SpecError> {
    let strict: Result<Value, _> = serde_norway::from_slice(bytes);
    match strict {
        Ok(value) => Ok(value),
        Err(error) => lenient(bytes).ok_or_else(|| {
            Box::new(
                Diagnostic::new(code::E0501, format!("{origin}: {error}"))
                    .help("the document must be valid JSON or YAML"),
            )
        }),
    }
}

/// A second parse that survives an integer outside `i64` and `u64`.
// TODO(rfc-0804): precision beyond 2^53 is given up, which is what every
// reader of the rendered page gives up anyway.
fn lenient(bytes: &[u8]) -> Option<Value> {
    serde_norway::from_slice::<Wide>(bytes)
        .ok()
        .map(|wide| wide.0)
}

/// [`Value`], with the two integer widths its own visitor refuses.
struct Wide(Value);

impl<'de> serde::Deserialize<'de> for Wide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(WideVisitor).map(Wide)
    }
}

struct WideVisitor;

impl<'de> serde::de::Visitor<'de> for WideVisitor {
    type Value = Value;

    fn expecting(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("any YAML value")
    }

    fn visit_bool<E>(self, flag: bool) -> Result<Value, E> {
        Ok(Value::Bool(flag))
    }

    fn visit_i64<E>(self, number: i64) -> Result<Value, E> {
        Ok(Value::Number(number.into()))
    }

    fn visit_u64<E>(self, number: u64) -> Result<Value, E> {
        Ok(Value::Number(number.into()))
    }

    fn visit_i128<E>(self, number: i128) -> Result<Value, E> {
        Ok(wide(number as f64))
    }

    fn visit_u128<E>(self, number: u128) -> Result<Value, E> {
        Ok(wide(number as f64))
    }

    fn visit_f64<E>(self, number: f64) -> Result<Value, E> {
        Ok(Value::Number(number.into()))
    }

    fn visit_str<E>(self, text: &str) -> Result<Value, E> {
        Ok(Value::String(text.to_owned()))
    }

    fn visit_string<E>(self, text: String) -> Result<Value, E> {
        Ok(Value::String(text))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(WideVisitor)
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(Wide(item)) = seq.next_element()? {
            out.push(item);
        }
        Ok(Value::Sequence(out))
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut out = Map::new();
        while let Some((Wide(key), Wide(value))) = map.next_entry()? {
            out.insert(key, value);
        }
        Ok(Value::Mapping(out))
    }
}

/// An integer no `i64` or `u64` holds becomes the float nearest to it, which
/// is the only shape left that a JSON document can carry.
fn wide(number: f64) -> Value {
    if number.is_finite() {
        Value::Number(number.into())
    } else {
        Value::Null
    }
}

/// The processed spec as JSON, with the document's own key order (API-50).
pub fn to_json(value: &Value) -> Result<String, SpecError> {
    serde_json::to_string_pretty(value).map_err(|e| {
        Box::new(Diagnostic::new(
            code::E0501,
            format!("cannot write the spec as JSON: {e}"),
        ))
    })
}

/// The processed spec as YAML (API-50).
pub fn to_yaml(value: &Value) -> Result<String, SpecError> {
    serde_norway::to_string(value).map_err(|e| {
        Box::new(Diagnostic::new(
            code::E0501,
            format!("cannot write the spec as YAML: {e}"),
        ))
    })
}

pub fn as_map(value: &Value) -> Option<&Map> {
    match value {
        Value::Mapping(map) => Some(map),
        _ => None,
    }
}

pub fn as_map_mut(value: &mut Value) -> Option<&mut Map> {
    match value {
        Value::Mapping(map) => Some(map),
        _ => None,
    }
}

pub fn as_seq(value: &Value) -> Option<&[Value]> {
    match value {
        Value::Sequence(items) => Some(items),
        _ => None,
    }
}

pub fn as_str(value: &Value) -> Option<&str> {
    match value {
        Value::String(text) => Some(text),
        _ => None,
    }
}

/// A mapping key as the string OpenAPI meant it to be.
///
/// YAML resolves an unquoted `200:` to an integer, so a spec that writes its
/// status codes without quotes has keys the model would otherwise reject. A
/// scalar is read as its text; a mapping or a sequence used as a key has no
/// reading and is left to the caller to complain about.
// TODO(rfc-0803): coercion is silent; `liyasa validate --openapi` is where a
// spec author hears that their document is not conformant.
pub fn key_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

pub fn as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(flag) => Some(*flag),
        _ => None,
    }
}

pub fn field<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    as_map(value)?.get(key)
}

pub fn field_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    as_str(field(value, key)?)
}

/// Every `key: value` of a mapping in document order, skipping non-string keys.
pub fn entries(value: &Value) -> impl Iterator<Item = (&str, &Value)> {
    as_map(value)
        .into_iter()
        .flat_map(|map| map.iter())
        .filter_map(|(k, v)| as_str(k).map(|k| (k, v)))
}

/// An RFC 6901 JSON Pointer, built segment by segment so the escaping is done
/// once and every diagnostic quotes the same shape.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pointer(String);

impl Pointer {
    pub const fn root() -> Self {
        Self(String::new())
    }

    pub fn parse(text: &str) -> Self {
        Self(text.to_owned())
    }

    #[must_use]
    pub fn push(&self, segment: &str) -> Self {
        Self(format!("{}/{}", self.0, escape(segment)))
    }

    #[must_use]
    pub fn index(&self, at: usize) -> Self {
        Self(format!("{}/{at}", self.0))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// The pointer's segments, unescaped.
    pub fn segments(&self) -> impl Iterator<Item = String> + '_ {
        self.0
            .split('/')
            .skip(usize::from(self.0.starts_with('/')))
            .filter(|_| !self.0.is_empty())
            .map(unescape)
    }
}

impl std::fmt::Display for Pointer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_empty() { "#" } else { &self.0 })
    }
}

fn escape(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

fn unescape(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

/// Resolves a pointer against a document. An array index must be a decimal
/// number; `-` (RFC 6901's "past the end") never resolves for a read.
pub fn get<'a>(root: &'a Value, pointer: &Pointer) -> Option<&'a Value> {
    let mut at = root;
    for segment in pointer.segments() {
        at = step(at, &segment)?;
    }
    Some(at)
}

pub fn get_mut<'a>(root: &'a mut Value, pointer: &Pointer) -> Option<&'a mut Value> {
    let mut at = root;
    for segment in pointer.segments() {
        at = step_mut(at, &segment)?;
    }
    Some(at)
}

fn step<'a>(value: &'a Value, segment: &str) -> Option<&'a Value> {
    match value {
        Value::Mapping(map) => map.get(segment),
        Value::Sequence(items) => items.get(segment.parse::<usize>().ok()?),
        _ => None,
    }
}

fn step_mut<'a>(value: &'a mut Value, segment: &str) -> Option<&'a mut Value> {
    match value {
        Value::Mapping(map) => map.get_mut(segment),
        Value::Sequence(items) => items.get_mut(segment.parse::<usize>().ok()?),
        _ => None,
    }
}

/// Writes `value` at `pointer`, creating nothing: the parent must exist. A new
/// mapping key is appended, which keeps overlay additions in a stable place.
pub fn set(root: &mut Value, pointer: &Pointer, value: Value) -> bool {
    let segments: Vec<String> = pointer.segments().collect();
    let Some((last, parents)) = segments.split_last() else {
        *root = value;
        return true;
    };
    let mut at = root;
    for segment in parents {
        match step_mut(at, segment) {
            Some(next) => at = next,
            None => return false,
        }
    }
    match at {
        Value::Mapping(map) => {
            map.insert(Value::String(last.clone()), value);
            true
        }
        Value::Sequence(items) => match last.parse::<usize>() {
            Ok(index) if index < items.len() => {
                items[index] = value;
                true
            }
            _ => false,
        },
        _ => false,
    }
}

/// Removes what `pointer` names, keeping the order of what is left.
pub fn remove(root: &mut Value, pointer: &Pointer) -> Option<Value> {
    let segments: Vec<String> = pointer.segments().collect();
    let (last, parents) = segments.split_last()?;
    let mut at = root;
    for segment in parents {
        at = step_mut(at, segment)?;
    }
    match at {
        Value::Mapping(map) => map.shift_remove(last.as_str()),
        Value::Sequence(items) => {
            let index = last.parse::<usize>().ok()?;
            (index < items.len()).then(|| items.remove(index))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "openapi: 3.1.0\npaths:\n  /a/b:\n    get:\n      tags: [x, y]\n";

    fn doc() -> Value {
        parse(DOC.as_bytes(), "test").expect("the fixture parses")
    }

    #[test]
    fn json_and_yaml_encodings_parse_to_the_same_tree() {
        let json = br#"{"openapi":"3.1.0","paths":{"/a/b":{"get":{"tags":["x","y"]}}}}"#;
        assert_eq!(
            parse(json, "test").expect("json parses"),
            doc(),
            "JSON is YAML; one parser reads both"
        );
    }

    #[test]
    fn key_order_survives_the_round_trip() {
        let source = b"b: 1\na: 2\nc: 3\n";
        let value = parse(source, "test").expect("the fixture parses");
        let json = to_json(&value).expect("the tree writes as JSON");
        assert!(
            json.find("\"b\"") < json.find("\"a\""),
            "a sorted tree would put `a` first: {json}"
        );
        assert_eq!(
            to_yaml(&value).expect("writes as YAML"),
            "b: 1\na: 2\nc: 3\n"
        );
    }

    #[test]
    fn a_path_with_slashes_escapes_into_one_segment() {
        let pointer = Pointer::root().push("paths").push("/a/b").push("get");
        assert_eq!(pointer.as_str(), "/paths/~1a~1b/get");
        assert!(
            get(&doc(), &pointer).is_some(),
            "the escaped pointer resolves"
        );
    }

    #[test]
    fn a_tilde_in_a_key_escapes_before_the_slash_does() {
        assert_eq!(Pointer::root().push("~/x").as_str(), "/~0~1x");
        assert_eq!(
            Pointer::parse("/~0~1x").segments().collect::<Vec<_>>(),
            vec!["~/x"]
        );
    }

    #[test]
    fn a_sequence_index_resolves_and_past_the_end_does_not() {
        let tags = Pointer::parse("/paths/~1a~1b/get/tags");
        assert_eq!(
            get(&doc(), &tags.index(1)),
            Some(&Value::String("y".into()))
        );
        assert_eq!(get(&doc(), &tags.push("-")), None);
        assert_eq!(get(&doc(), &tags.index(9)), None);
    }

    #[test]
    fn set_appends_a_new_key_and_replaces_an_existing_one() {
        let mut value = doc();
        let get_op = Pointer::parse("/paths/~1a~1b/get");
        assert!(set(&mut value, &get_op.push("summary"), "List".into()));
        assert!(set(
            &mut value,
            &get_op.push("tags"),
            Value::Sequence(vec![])
        ));

        let op = get(&value, &get_op).expect("the operation is still there");
        let keys: Vec<&str> = entries(op).map(|(k, _)| k).collect();
        assert_eq!(
            keys,
            vec!["tags", "summary"],
            "a replaced key keeps its place"
        );
    }

    #[test]
    fn set_refuses_a_parent_that_does_not_exist() {
        let mut value = doc();
        let missing = Pointer::parse("/paths/~1nope/get/summary");
        assert!(!set(&mut value, &missing, "x".into()));
    }

    #[test]
    fn remove_keeps_the_order_of_what_is_left() {
        let mut value = parse(b"a: 1\nb: 2\nc: 3\n", "test").expect("parses");
        assert_eq!(
            remove(&mut value, &Pointer::root().push("b")),
            Some(2.into())
        );
        assert_eq!(
            entries(&value).map(|(k, _)| k).collect::<Vec<_>>(),
            vec!["a", "c"]
        );
    }

    #[test]
    fn a_malformed_document_is_a_diagnostic_not_a_panic() {
        let error = parse(b"openapi: [unclosed", "api.yaml").expect_err("malformed YAML fails");
        assert_eq!(error.code, code::E0501);
        assert!(error.message.starts_with("api.yaml:"), "{}", error.message);
    }
}
