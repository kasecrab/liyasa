//! Spec drift: which operations moved between two versions of a spec (VER-12).
//!
//! The diff is over the document rather than over a parsed model, so it works
//! on any OpenAPI version and on a snapshot taken before `liyasa-openapi`
//! could read it. What it compares is exactly what a reader of the page sees:
//! the operation's parameters, its request body, its responses, and what it
//! takes to call it.
//!
//! Operations are named the way the graph names them. `operationId` when the
//! document gives one — that is the name `openapi("petstore", "listPets")`
//! writes — and `GET /pets` when it does not, because an operation with no ID
//! still has exactly one method and path.

use std::collections::BTreeMap;

use serde_json::Value;

use super::impact::OperationChange;

/// The facets VER-12 flags a page for. `requestBody` travels with
/// `parameters`: both are what a caller has to send, and a page that shows a
/// request is wrong when either moves.
const FACETS: &[(&str, &[&str])] = &[
    ("parameters", &["parameters", "requestBody"]),
    ("responses", &["responses"]),
    ("auth", &["security"]),
];

const METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// One operation as the differ sees it: its facets, with the document's
/// defaults already folded in.
#[derive(Debug, Clone, Default, PartialEq)]
struct Operation {
    /// Facet name → the part of the document it covers.
    facets: BTreeMap<String, Value>,
}

/// Every operation in a document, keyed by the name the graph uses.
fn operations(document: &Value) -> BTreeMap<String, Operation> {
    let mut out = BTreeMap::new();
    let Some(paths) = document.get("paths").and_then(Value::as_object) else {
        return out;
    };
    // A document-level `security` applies to every operation that does not
    // state its own, so an operation that inherits it has to inherit it here
    // too — otherwise removing it from the document changes nothing visible.
    let inherited_security = document.get("security");
    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        // `parameters` on the path item apply to every operation under it.
        let shared = item.get("parameters");
        for method in METHODS {
            let Some(operation) = item.get(*method) else {
                continue;
            };
            let name = operation
                .get("operationId")
                .and_then(Value::as_str)
                .map_or_else(
                    || format!("{} {path}", method.to_ascii_uppercase()),
                    ToOwned::to_owned,
                );
            let mut facets = BTreeMap::new();
            for (facet, keys) in FACETS {
                let mut parts = serde_json::Map::new();
                for key in *keys {
                    let found = match *key {
                        "parameters" => merged_parameters(shared, operation.get("parameters")),
                        "security" => operation.get("security").or(inherited_security).cloned(),
                        other => operation.get(other).cloned(),
                    };
                    if let Some(value) = found {
                        parts.insert((*key).to_owned(), value);
                    }
                }
                facets.insert((*facet).to_owned(), Value::Object(parts));
            }
            out.insert(name, Operation { facets });
        }
    }
    out
}

/// Path-level parameters then the operation's own, which is the order OpenAPI
/// resolves them in.
fn merged_parameters(shared: Option<&Value>, own: Option<&Value>) -> Option<Value> {
    match (
        shared.and_then(Value::as_array),
        own.and_then(Value::as_array),
    ) {
        (None, None) => own.cloned(),
        (shared, own) => {
            let mut all = shared.cloned().unwrap_or_default();
            all.extend(own.cloned().unwrap_or_default());
            Some(Value::Array(all))
        }
    }
}

/// VER-12: the operations whose parameters, responses, or auth moved.
///
/// An operation that was added or removed is reported too, with that word as
/// its diff: a page documenting an operation the spec no longer has is as
/// wrong as one documenting a changed parameter, and a page cannot be flagged
/// for a change nobody reported.
pub fn operation_changes(spec: &str, old: &Value, new: &Value) -> Vec<OperationChange> {
    let before = operations(old);
    let after = operations(new);
    let mut out = Vec::new();
    for (name, operation) in &after {
        let diff = match before.get(name) {
            None => vec!["added".to_owned()],
            Some(previous) => {
                let moved: Vec<String> = FACETS
                    .iter()
                    .map(|(facet, _)| *facet)
                    .filter(|facet| previous.facets.get(*facet) != operation.facets.get(*facet))
                    .map(ToOwned::to_owned)
                    .collect();
                if moved.is_empty() {
                    continue;
                }
                moved
            }
        };
        out.push(OperationChange {
            spec: spec.to_owned(),
            op: name.clone(),
            diff,
        });
    }
    for name in before.keys() {
        if !after.contains_key(name) {
            out.push(OperationChange {
                spec: spec.to_owned(),
                op: name.clone(),
                diff: vec!["removed".to_owned()],
            });
        }
    }
    out.sort_by(|a, b| a.op.cmp(&b.op));
    out
}

#[cfg(test)]
mod tests;
