//! `get_openapi_operation`: one API operation, read out of the processed
//! specs the build wrote.
//!
//! The documents under `dist/openapi/` are what API-50 publishes: processed —
//! overlays applied, `x-liyasa.hidden` operations already removed — and
//! written for `Audience::public()`, which is precisely why handing one to
//! anyone is correct. So this reads them without a group filter, and the
//! filter's absence is a property of the file rather than an omission here.
//! Per-reader filtering of a spec is API-52 and happens at request time
//! against a document these are not.
//!
//! Parsed with `serde_json` rather than through `liyasa-openapi`, because the
//! question is "which operation is this" and answering it needs the paths
//! object and nothing else. Depending on the modelling crate to walk two
//! levels of a map would put the whole OpenAPI model in the server's graph
//! for no answer it could not give.

use std::fmt::Write as _;

use serde_json::Value;

use crate::routes::bundle::Bundle;

use super::reader::{PageText, Scope, ToolFailure};

const METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// Finds `operation` in any spec this bundle published.
///
/// `scope` is unused and named so: see the module comment. It stays in the
/// signature because [`super::reader::SiteReader`] is one seam and a method
/// that quietly took no caller would be the place a later filter is forgotten.
pub fn find(bundle: &Bundle, operation: &str, _scope: &Scope) -> Result<PageText, ToolFailure> {
    let wanted = operation.trim();
    if wanted.is_empty() {
        return Err(ToolFailure::BadInput(
            "`operation` is required; name it `GET /pets` or by its operationId".to_owned(),
        ));
    }
    let specs = specs(bundle);
    if specs.is_empty() {
        return Err(ToolFailure::Unavailable(
            "this site publishes no OpenAPI specification".to_owned(),
        ));
    }
    let mut known: Vec<String> = Vec::new();
    for (path, document) in &specs {
        if let Some(found) = search_document(document, wanted, &mut known) {
            return Ok(PageText {
                route: format!("/{}", path.trim_start_matches('/')),
                title: found.title,
                anchor: found.anchor,
                markdown: found.markdown,
            });
        }
    }
    known.sort();
    known.dedup();
    Err(ToolFailure::NotFound(format!(
        "no operation `{wanted}`. This site documents: {}",
        summarize(&known)
    )))
}

/// Every processed spec the build wrote, as `(bundle path, document)`.
fn specs(bundle: &Bundle) -> Vec<(String, Value)> {
    bundle
        .manifest()
        .served
        .iter()
        .filter(|file| {
            let path = file.path.trim_start_matches('/');
            path.starts_with("openapi/") && path.ends_with(".json")
        })
        .filter_map(|file| {
            let bytes = bundle.read(&file.path).ok()?;
            let document: Value = serde_json::from_slice(&bytes).ok()?;
            Some((file.path.clone(), document))
        })
        .collect()
}

struct Found {
    title: String,
    anchor: String,
    markdown: String,
}

fn search_document(document: &Value, wanted: &str, known: &mut Vec<String>) -> Option<Found> {
    let paths = document.get("paths")?.as_object()?;
    for (path, item) in paths {
        let item = item.as_object()?;
        for method in METHODS {
            let Some(operation) = item.get(*method) else {
                continue;
            };
            let id = operation.get("operationId").and_then(Value::as_str);
            let name = format!("{} {path}", method.to_uppercase());
            known.push(match id {
                Some(id) => format!("{name} (`{id}`)"),
                None => name.clone(),
            });
            if !names_this(wanted, &name, id) {
                continue;
            }
            return Some(Found {
                anchor: id.map(str::to_owned).unwrap_or_else(|| slug(&name)),
                markdown: render(document, &name, operation, item),
                title: operation
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or(&name)
                    .to_owned(),
            });
        }
    }
    None
}

/// Whether `wanted` names this operation.
///
/// Three spellings, because an agent gets the name from three places: a
/// search result gives `GET /pets`, a spec download gives `listPets`, and a
/// page heading gives either with different spacing.
fn names_this(wanted: &str, name: &str, id: Option<&str>) -> bool {
    if id.is_some_and(|id| id == wanted) {
        return true;
    }
    let normalize = |text: &str| -> String {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_uppercase()
    };
    normalize(wanted) == normalize(name)
}

fn render(
    document: &Value,
    name: &str,
    operation: &Value,
    item: &serde_json::Map<String, Value>,
) -> String {
    let mut out = format!("# {name}\n");
    if let Some(summary) = operation.get("summary").and_then(Value::as_str) {
        let _ = writeln!(out, "\n{summary}");
    }
    if let Some(id) = operation.get("operationId").and_then(Value::as_str) {
        let _ = writeln!(out, "\n`operationId`: `{id}`");
    }
    if let Some(servers) = document.get("servers").and_then(Value::as_array) {
        let urls: Vec<&str> = servers
            .iter()
            .filter_map(|server| server.get("url").and_then(Value::as_str))
            .collect();
        if !urls.is_empty() {
            let _ = writeln!(out, "\nServers: {}", urls.join(", "));
        }
    }
    if let Some(description) = operation.get("description").and_then(Value::as_str) {
        let _ = writeln!(out, "\n{description}");
    }

    // Path-level parameters apply to every operation under the path and are
    // as load-bearing as the operation's own; an agent given only the second
    // set builds a request that 404s on a path variable it never saw.
    let mut parameters: Vec<&Value> = Vec::new();
    for source in [item.get("parameters"), operation.get("parameters")] {
        if let Some(list) = source.and_then(Value::as_array) {
            parameters.extend(list);
        }
    }
    if !parameters.is_empty() {
        out.push_str("\n## Parameters\n\n");
        for parameter in parameters {
            let pname = parameter
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("(unnamed)");
            let location = parameter.get("in").and_then(Value::as_str).unwrap_or("?");
            let required = parameter
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let _ = write!(
                out,
                "- `{pname}` ({location}{})",
                if required { ", required" } else { "" }
            );
            match parameter.get("description").and_then(Value::as_str) {
                Some(text) => {
                    let _ = writeln!(out, " — {}", text.replace('\n', " "));
                }
                None => out.push('\n'),
            }
        }
    }

    if let Some(body) = operation.get("requestBody") {
        out.push_str("\n## Request body\n\n");
        let _ = writeln!(out, "```json\n{}\n```", pretty(body));
    }

    if let Some(responses) = operation.get("responses").and_then(Value::as_object) {
        out.push_str("\n## Responses\n\n");
        for (status, response) in responses {
            let description = response
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("");
            let _ = writeln!(out, "- `{status}` — {description}");
        }
    }
    out
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn slug(name: &str) -> String {
    liyasa_markdown::ast::anchors::slugify(name)
}

/// The first few operation names, so a refusal is actionable without being a
/// dump of a spec with four hundred paths in it.
fn summarize(known: &[String]) -> String {
    const SHOWN: usize = 12;
    if known.is_empty() {
        return "no operations at all".to_owned();
    }
    let head = known
        .iter()
        .take(SHOWN)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    match known.len() > SHOWN {
        true => format!("{head}, and {} more", known.len() - SHOWN),
        false => head,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Value {
        serde_json::json!({
            "openapi": "3.1.0",
            "servers": [{ "url": "https://api.acme.example" }],
            "paths": {
                "/pets/{id}": {
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "description": "Which pet." }
                    ],
                    "get": {
                        "operationId": "getPet",
                        "summary": "Read one pet",
                        "responses": { "200": { "description": "ok" } }
                    }
                }
            }
        })
    }

    fn found(wanted: &str) -> Option<Found> {
        let mut known = Vec::new();
        search_document(&spec(), wanted, &mut known)
    }

    #[test]
    fn an_operation_is_found_by_method_and_path_or_by_id() {
        for wanted in ["GET /pets/{id}", "get /pets/{id}", "getPet"] {
            assert!(found(wanted).is_some(), "{wanted}");
        }
        assert!(found("GET /pets").is_none());
        // An operationId is case sensitive: `getpet` is a different symbol,
        // and answering for it would teach an agent a name that fails in
        // every generated client.
        assert!(found("getpet").is_none());
    }

    #[test]
    fn the_rendering_carries_the_path_level_parameters() {
        // An agent given only the operation's own parameters builds a request
        // with `{id}` unreplaced.
        let rendered = found("getPet").expect("the operation").markdown;
        assert!(rendered.contains("`id` (path, required)"), "{rendered}");
        assert!(rendered.contains("Which pet."), "{rendered}");
        assert!(rendered.contains("https://api.acme.example"), "{rendered}");
        assert!(rendered.contains("`200` — ok"), "{rendered}");
    }

    #[test]
    fn a_miss_names_what_the_site_does_document() {
        let mut known = Vec::new();
        assert!(search_document(&spec(), "POST /nope", &mut known).is_none());
        assert!(
            known.iter().any(|name| name.contains("GET /pets/{id}")),
            "{known:?}"
        );
        assert!(summarize(&known).contains("getPet"), "{known:?}");
    }

    #[test]
    fn a_long_spec_is_summarized_rather_than_dumped() {
        let many: Vec<String> = (0..40).map(|n| format!("GET /p{n}")).collect();
        let text = summarize(&many);
        assert!(text.contains("and 28 more"), "{text}");
    }
}
