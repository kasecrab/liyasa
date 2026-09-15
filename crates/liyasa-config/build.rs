//! Generates `SiteConfig` and its subtypes from `schemas/liyasa.schema.json`.
//!
//! The schema is the single source of truth (CFG-94); nothing here may change
//! what it means. typify 0.8 reads schemars 0.8's draft-07 model, so the copy it
//! is handed is normalized first — see `plan/rfcs/0100-typify-schema-input.md`
//! for why `$defs` moves, why `pattern` is dropped, and why `oneOf` branches are
//! given titles.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value};
use typify::{TypeSpace, TypeSpacePatch, TypeSpaceSettings};

const SCHEMA: &str = "../../schemas/liyasa.schema.json";
/// The type typify names after the schema's `title`.
const ROOT: &str = "LiyasaSiteConfiguration";

fn main() {
    println!("cargo::rerun-if-changed={SCHEMA}");
    let text = std::fs::read_to_string(SCHEMA).expect("the config schema is readable");
    let normalized = normalize(&text);

    // First pass: learn the names typify derives, so they can be shortened
    // through its own rename mechanism rather than by editing its output.
    let renames = shorter_names(&generate(&normalized, &BTreeMap::new()).0);
    let (stream, space) = generate(&normalized, &renames);

    for (used, what) in [
        (space.uses_regress(), "regress"),
        (space.uses_chrono(), "chrono"),
        (space.uses_uuid(), "uuid"),
    ] {
        assert!(
            !used,
            "the generated config types now need `{what}`; RFC 0100 keeps them dependency-free"
        );
    }

    let out = Path::new(&std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join("model.rs");
    std::fs::write(&out, stream).expect("the generated model is writable");
}

fn generate(schema: &str, renames: &BTreeMap<String, String>) -> (String, TypeSpace) {
    let root: schemars_typify::schema::RootSchema =
        serde_json::from_str(schema).expect("the normalized schema is a draft-07 root schema");

    let mut settings = TypeSpaceSettings::default();
    settings.with_derive("PartialEq".to_owned());
    settings.with_patch(ROOT, TypeSpacePatch::default().with_rename("SiteConfig"));
    for (from, to) in renames {
        settings.with_patch(from, TypeSpacePatch::default().with_rename(to));
    }

    let mut space = TypeSpace::new(&settings);
    space
        .add_root_schema(root)
        .expect("the config schema converts to Rust types");
    (space.to_stream().to_string(), space)
}

/// `LiyasaSiteConfigurationThemeColors` reads as `ThemeColors` everywhere it is
/// used; the prefix says only that it came from this schema.
fn shorter_names(generated: &str) -> BTreeMap<String, String> {
    let declared = declared_names(generated);
    let mut renames = BTreeMap::new();
    for name in &declared {
        let Some(short) = name.strip_prefix(ROOT) else {
            continue;
        };
        if short.is_empty() {
            continue; // the root itself, already renamed to `SiteConfig`
        }
        renames.insert(name.clone(), short.to_owned());
    }

    let mut after: Vec<&str> = declared
        .iter()
        .map(|name| renames.get(name).map_or(name.as_str(), String::as_str))
        .collect();
    let before = after.len();
    after.sort_unstable();
    after.dedup();
    assert_eq!(
        before,
        after.len(),
        "dropping the `{ROOT}` prefix collides two generated type names"
    );
    renames
}

fn declared_names(generated: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for keyword in ["pub struct ", "pub enum "] {
        for (at, _) in generated.match_indices(keyword) {
            out.insert(
                generated[at + keyword.len()..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect(),
            );
        }
    }
    out
}

/// The 2020-12 schema as schemars 0.8 expects to read it (RFC 0100).
fn normalize(text: &str) -> String {
    let mut root: Value = serde_json::from_str(text).expect("the config schema is valid JSON");
    drop_patterns(&mut root);
    title_branches(&mut root);
    if let Some(object) = root.as_object_mut()
        && let Some(defs) = object.remove("$defs")
    {
        object.insert("definitions".to_owned(), defs);
    }
    serde_json::to_string(&root)
        .expect("a JSON value re-serializes")
        .replace("#/$defs/", "#/definitions/")
}

/// `pattern` is validated by `jsonschema`, not by the generated types; leaving
/// it in makes typify emit `regress` and `unwrap` on user input.
fn drop_patterns(node: &mut Value) {
    match node {
        Value::Object(object) => {
            object.remove("pattern");
            for value in object.values_mut() {
                drop_patterns(value);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(drop_patterns),
        _ => {}
    }
}

/// Names the branches of a `oneOf` so the generated enum reads
/// `NavigationNode::Group` rather than `NavigationNode::Variant1`. A `title` is
/// an annotation, so this does not change what the schema accepts; a branch set
/// that would produce the same title twice is left to typify to number.
fn title_branches(node: &mut Value) {
    match node {
        Value::Object(object) => {
            for keyword in ["oneOf", "anyOf"] {
                let Some(Value::Array(branches)) = object.get(keyword) else {
                    continue;
                };
                let titles: Vec<Option<String>> = branches.iter().map(branch_title).collect();
                let mut named: Vec<&String> = titles.iter().flatten().collect();
                let distinct = named.len();
                named.sort_unstable();
                named.dedup();
                if named.len() != distinct {
                    continue;
                }
                let Some(Value::Array(branches)) = object.get_mut(keyword) else {
                    continue;
                };
                for (branch, title) in branches.iter_mut().zip(titles) {
                    if let (Value::Object(branch), Some(title)) = (branch, title) {
                        branch.insert("title".to_owned(), Value::String(title));
                    }
                }
            }
            for value in object.values_mut() {
                title_branches(value);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(title_branches),
        _ => {}
    }
}

/// An object branch is named for the key that discriminates it (§8.4); every
/// other branch is named for its JSON type.
fn branch_title(branch: &Value) -> Option<String> {
    let branch = branch.as_object()?;
    if ["title", "$ref", "enum"]
        .iter()
        .any(|k| branch.contains_key(*k))
    {
        return None;
    }
    Some(match branch.get("type")?.as_str()? {
        "object" => match discriminator(branch) {
            Some(key) => pascal_case(key),
            None => "Options".to_owned(),
        },
        "array" => "List".to_owned(),
        "string" => "Text".to_owned(),
        "integer" | "number" => "Number".to_owned(),
        "boolean" => "Flag".to_owned(),
        _ => return None,
    })
}

fn discriminator(branch: &Map<String, Value>) -> Option<&str> {
    branch.get("required")?.as_array()?.first()?.as_str()
}

fn pascal_case(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut upper = true;
    for c in key.chars() {
        match c {
            '-' | '_' | '.' | ' ' => upper = true,
            _ if upper => {
                out.extend(c.to_uppercase());
                upper = false;
            }
            _ => out.push(c),
        }
    }
    out
}
