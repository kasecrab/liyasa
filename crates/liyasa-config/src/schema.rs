//! `schemas/liyasa.schema.json`, and what it says about one config document.
//!
//! The file is the single source of truth (CFG-94): it is embedded verbatim so
//! `liyasa schema config` can emit it byte for byte, the Rust types are
//! generated from it at build time, and validation here is a plain 2020-12 run
//! against it with no Liyasa-specific relaxation.

use std::sync::OnceLock;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, Severity, code};
use serde_json::{Map, Value};

use crate::json::SpanIndex;

/// The schema, byte for byte as it is on disk (CLI-13).
pub const CONFIG_SCHEMA: &str = include_str!("../../../schemas/liyasa.schema.json");

/// The `$id` the schema publishes, and the `$schema` a config should carry.
pub const CONFIG_SCHEMA_ID: &str = "https://liyasa.dev/schema/v1/liyasa.json";

/// The major version this build understands (CFG-91).
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// What one validation run found.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub diagnostics: Diagnostics,
    /// JSON Pointers of keys the schema does not know. They are warnings, not
    /// errors (RFC 0006), and must be removed before deserializing.
    pub unknown: Vec<String>,
}

pub fn config_schema() -> &'static Value {
    static PARSED: OnceLock<Value> = OnceLock::new();
    PARSED.get_or_init(|| {
        serde_json::from_str(CONFIG_SCHEMA).expect("the embedded config schema is valid JSON")
    })
}

fn validator() -> &'static jsonschema::Validator {
    static COMPILED: OnceLock<jsonschema::Validator> = OnceLock::new();
    COMPILED.get_or_init(|| {
        jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(config_schema())
            .expect("the embedded config schema compiles")
    })
}

/// Validates one parsed config against the schema.
pub fn check(config: &Value, spans: &SpanIndex) -> Report {
    let mut report = Report::default();
    for error in validator().iter_errors(config) {
        let at = error.instance_path().to_string();
        match error.kind() {
            jsonschema::error::ValidationErrorKind::AdditionalProperties { unexpected } => {
                for key in unexpected {
                    let pointer = format!("{at}/{}", escape(key));
                    report
                        .diagnostics
                        .push(unknown_key(&pointer, key, config, spans, &at));
                    report.unknown.push(pointer);
                }
            }
            _ => {
                // The value is what is wrong, so it is what gets underlined; a
                // missing required property has no value and falls back to the
                // object that should have held it.
                let mut diagnostic = Diagnostic::new(code::E0102, error.to_string());
                if let Some(span) = spans.nearest(&at) {
                    diagnostic = diagnostic.at(span);
                }
                report.diagnostics.push(diagnostic);
            }
        }
    }
    report
}

fn unknown_key(
    pointer: &str,
    key: &str,
    config: &Value,
    spans: &SpanIndex,
    parent: &str,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(code::E0103, format!("unknown config key `{key}`"))
        .with_severity(Severity::Warning);
    if let Some(span) = spans.nearest_key(pointer) {
        diagnostic = diagnostic.at(span);
    }
    if let Some(near) = nearest_known(key, config, parent) {
        diagnostic = diagnostic.help(format!("did you mean `{near}`?"));
    }
    diagnostic
}

/// The key the schema does know that is closest to the one written, when it is
/// close enough that a typo is the likely explanation.
fn nearest_known(key: &str, config: &Value, parent: &str) -> Option<String> {
    let subschema = subschema_at(config_schema(), config, parent)?;
    let known = subschema.get("properties")?.as_object()?;
    known
        .keys()
        .map(|candidate| (distance(key, candidate), candidate))
        .filter(|(d, _)| *d <= 2 && *d < key.len())
        .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)))
        .map(|(_, candidate)| candidate.clone())
}

/// Walks the schema alongside the instance to the object that holds `pointer`.
/// Only the shapes the config schema actually uses are followed: nested
/// `properties`, `items`, and the one `oneOf` branch the instance matches by
/// its own keys.
fn subschema_at<'a>(
    schema: &'a Value,
    config: &Value,
    pointer: &str,
) -> Option<&'a Map<String, Value>> {
    let mut schema = schema;
    let mut config = config;
    for token in pointer.split('/').skip(1).filter(|t| !t.is_empty()) {
        let token = unescape(token);
        schema = resolve(schema, config)?;
        (schema, config) = match config {
            Value::Array(_) => (
                schema.get("items")?,
                config.get(token.parse::<usize>().ok()?)?,
            ),
            _ => (schema.get("properties")?.get(&token)?, config.get(&token)?),
        };
    }
    resolve(schema, config)?.as_object()
}

/// Follows `$ref` into `$defs` and picks the `oneOf` branch whose required keys
/// the instance has.
fn resolve<'a>(schema: &'a Value, config: &Value) -> Option<&'a Value> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let name = reference.strip_prefix("#/$defs/")?;
        return resolve(config_schema().get("$defs")?.get(name)?, config);
    }
    let Some(branches) = schema.get("oneOf").and_then(Value::as_array) else {
        return Some(schema);
    };
    branches
        .iter()
        .find(|branch| {
            branch
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|required| {
                    required
                        .iter()
                        .filter_map(Value::as_str)
                        .all(|key| config.get(key).is_some())
                })
        })
        .or_else(|| branches.iter().find(|b| b.get("properties").is_some()))
}

/// Levenshtein distance, capped by the shorter word: only used to decide
/// whether to offer a suggestion at all.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let next = row[j + 1];
            row[j + 1] = if ca == *cb {
                diagonal
            } else {
                1 + diagonal.min(row[j]).min(next)
            };
            diagonal = next;
        }
    }
    row[b.len()]
}

/// The config with every pointer in `unknown` removed, so it can be
/// deserialized into the generated types, which deny unknown fields.
pub fn without(config: &Value, unknown: &[String]) -> Value {
    let mut pruned = config.clone();
    // Deepest first, so removing a parent never invalidates a child's path.
    let mut pointers: Vec<&String> = unknown.iter().collect();
    pointers.sort_by_key(|p| std::cmp::Reverse(p.matches('/').count()));
    for pointer in pointers {
        let (parent, key) = match pointer.rsplit_once('/') {
            Some(split) => split,
            None => continue,
        };
        let key = unescape(key);
        match pruned.pointer_mut(parent) {
            Some(Value::Object(object)) => {
                object.remove(&key);
            }
            Some(Value::Array(items)) => {
                if let Ok(at) = key.parse::<usize>()
                    && at < items.len()
                {
                    items.remove(at);
                }
            }
            _ => {}
        }
    }
    pruned
}

/// The major version in the config's `$schema`, and `E0121` when it is newer
/// than this build understands (CFG-91).
pub fn declared_version(
    config: &Value,
    spans: &SpanIndex,
    diagnostics: &mut Diagnostics,
) -> Option<u32> {
    let declared = config.get("$schema")?.as_str()?;
    let version = declared
        .split('/')
        .find_map(|segment| segment.strip_prefix('v')?.parse::<u32>().ok())?;
    if version > CONFIG_SCHEMA_VERSION {
        let mut diagnostic = Diagnostic::new(
            code::E0121,
            format!(
                "this config declares schema v{version}; this build understands \
                 v{CONFIG_SCHEMA_VERSION}"
            ),
        )
        .help("upgrade Liyasa, or point `$schema` at the version this build reads");
        if let Some(span) = spans.value("/$schema") {
            diagnostic = diagnostic.at(span);
        }
        diagnostics.push(diagnostic);
    }
    Some(version)
}

fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

fn unescape(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}
