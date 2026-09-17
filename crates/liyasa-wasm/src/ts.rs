//! `ts/liyasa-wasm.d.ts`, generated from the types in [`crate::api`].
//!
//! The declaration is not written by hand and not inferred by `wasm-bindgen`,
//! which would type every payload as `any`. It is emitted from the same
//! `schemars` schemas the Rendered AST and the diagnostic payload are already
//! published from (`schemas/ast.json`, `schemas/diagnostic.json`), so the
//! TypeScript the editor compiles against and the Rust the module runs are one
//! source.
//!
//! `tests/it/typescript.rs` compares the checked-in file with what this
//! produces, so a field that changes here is a changed file in the diff rather
//! than a surprise in `web/editor`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::{Map, Value};

/// The whole declaration file.
pub fn declaration() -> String {
    let mut roots: Vec<(&str, Value)> = Vec::new();
    let mut defs: BTreeMap<String, Value> = BTreeMap::new();

    macro_rules! root {
        ($($name:ident),* $(,)?) => {
            $({
                let schema = serde_json::to_value(schemars::schema_for!(crate::api::$name))
                    .unwrap_or(Value::Bool(true));
                let (body, found) = split(schema);
                defs.extend(found);
                roots.push((stringify!($name), body));
            })*
        };
    }

    root!(
        Highlight,
        OpenRequest,
        Options,
        ParseRequest,
        ParseResponse,
        PreviewRequest,
        PreviewResponse,
        SearchHit,
        SearchRequest,
        SearchResponse,
        SeedEntry,
        SerializeRequest,
        SerializeResponse,
        SessionStatus,
        SiteMeta,
        Snippet,
        ValidateMode,
        ValidateRequest,
        ValidateResponse,
    );

    // A root type is also reachable as a `$def` of another root; the root's own
    // rendering wins, so the definition does not appear twice.
    for (name, _) in &roots {
        defs.remove(*name);
    }

    let mut out = String::new();
    out.push_str(HEADER);
    for (name, schema) in &roots {
        emit(&mut out, name, schema);
    }
    out.push_str("\n// ---- shared types, from `liyasa-core` ----\n");
    for (name, schema) in &defs {
        emit(&mut out, name, schema);
    }
    out.push_str(CLASSES);
    out
}

const HEADER: &str = "\
// Generated from `crates/liyasa-wasm/src/api.rs` by `liyasa_wasm::ts`.
// Do not edit: `cargo test -p liyasa-wasm typescript` rewrites it and fails
// when this file and the Rust types disagree.

";

/// The two objects `web/editor` and the search worker import. Written here
/// rather than derived, because `wasm-bindgen` generates the bodies and only
/// the shapes are this crate's contract.
const CLASSES: &str = "
// ---- the objects ----

/** One editor session over one draft (ED-07). */
export declare class Session {
  static open(request: OpenRequest): Session;
  free(): void;
  /**
   * Hands over a path a response named in `missing`. The host fetched it
   * through `/_liyasa/editor/fs/<path>`; the module does no I/O of its own.
   */
  seed(path: string, text: string): void;
  parse(request: ParseRequest): ParseResponse;
  preview(request: PreviewRequest): PreviewResponse;
  validate(request: ValidateRequest): ValidateResponse;
  serialize(request: SerializeRequest): SerializeResponse;
  status(): SessionStatus;
}

/** The browser search worker over `liyasa-idx` shard bytes (SRC-05). */
export declare class Searcher {
  /** The bytes of `manifest.json`, which the worker fetches first. */
  static open(manifest: Uint8Array): Searcher;
  free(): void;
  addFile(name: string, bytes: Uint8Array): void;
  search(request: SearchRequest): SearchResponse;
}
";

/// Lifts a schema's `$defs` out, leaving the root body behind.
fn split(schema: Value) -> (Value, BTreeMap<String, Value>) {
    let Value::Object(mut object) = schema else {
        return (schema, BTreeMap::new());
    };
    let defs = match object.remove("$defs") {
        Some(Value::Object(defs)) => defs.into_iter().collect(),
        _ => BTreeMap::new(),
    };
    object.remove("$schema");
    object.remove("$id");
    object.remove("title");
    (Value::Object(object), defs)
}

fn emit(out: &mut String, name: &str, schema: &Value) {
    if let Some(doc) = schema.get("description").and_then(Value::as_str) {
        out.push_str(&comment(doc, ""));
    }
    if let Some(properties) = object_properties(schema) {
        let _ = writeln!(out, "export interface {name} {{");
        out.push_str(&fields(properties, required(schema), "  "));
        out.push_str("}\n\n");
        return;
    }
    let _ = writeln!(out, "export type {name} = {};\n", render(schema, ""));
}

fn object_properties(schema: &Value) -> Option<&Map<String, Value>> {
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return None;
    }
    schema.get("properties")?.as_object()
}

fn required(schema: &Value) -> Vec<&str> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn fields(properties: &Map<String, Value>, required: Vec<&str>, indent: &str) -> String {
    let mut out = String::new();
    for (name, schema) in properties {
        if let Some(doc) = schema.get("description").and_then(Value::as_str) {
            out.push_str(&comment(doc, indent));
        }
        let optional = if required.contains(&name.as_str()) {
            ""
        } else {
            "?"
        };
        let _ = writeln!(
            out,
            "{indent}{name}{optional}: {};",
            render(schema, indent)
        );
    }
    out
}

/// One schema as a TypeScript type expression.
fn render(schema: &Value, indent: &str) -> String {
    match schema {
        // `true` and `{}` accept anything, which is what `serde_json::Value`
        // becomes.
        Value::Bool(true) => return "unknown".to_owned(),
        Value::Bool(false) => return "never".to_owned(),
        Value::Object(object) if object.is_empty() => return "unknown".to_owned(),
        _ => {}
    }

    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return reference
            .rsplit('/')
            .next()
            .unwrap_or("unknown")
            .to_owned();
    }
    if let Some(constant) = schema.get("const") {
        return literal(constant);
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(Value::Array(variants)) = schema.get(key) {
            let rendered: Vec<String> = variants.iter().map(|v| render(v, indent)).collect();
            return rendered.join(" | ");
        }
    }
    if let Some(Value::Array(values)) = schema.get("enum") {
        let rendered: Vec<String> = values.iter().map(literal).collect();
        return rendered.join(" | ");
    }

    match schema.get("type") {
        Some(Value::Array(types)) => {
            let rendered: Vec<String> = types
                .iter()
                .filter_map(Value::as_str)
                .map(|name| scalar(name, schema, indent))
                .collect();
            rendered.join(" | ")
        }
        Some(Value::String(name)) => scalar(name, schema, indent),
        _ => "unknown".to_owned(),
    }
}

fn scalar(name: &str, schema: &Value, indent: &str) -> String {
    match name {
        "string" => "string".to_owned(),
        "integer" | "number" => "number".to_owned(),
        "boolean" => "boolean".to_owned(),
        "null" => "null".to_owned(),
        "array" => array(schema, indent),
        "object" => object(schema, indent),
        _ => "unknown".to_owned(),
    }
}

/// A tuple when the schema pins its members, an array otherwise.
fn array(schema: &Value, indent: &str) -> String {
    if let Some(Value::Array(members)) = schema.get("prefixItems") {
        let rendered: Vec<String> = members.iter().map(|m| render(m, indent)).collect();
        return format!("[{}]", rendered.join(", "));
    }
    match schema.get("items") {
        Some(items) => {
            let inner = render(items, indent);
            if inner.contains(' ') {
                format!("({inner})[]")
            } else {
                format!("{inner}[]")
            }
        }
        None => "unknown[]".to_owned(),
    }
}

fn object(schema: &Value, indent: &str) -> String {
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        let inner = format!("{indent}  ");
        return format!(
            "{{\n{}{indent}}}",
            fields(properties, required(schema), &inner)
        );
    }
    match schema.get("additionalProperties") {
        Some(Value::Bool(false)) => "Record<string, never>".to_owned(),
        Some(values) => format!("Record<string, {}>", render(values, indent)),
        None => "Record<string, unknown>".to_owned(),
    }
}

fn literal(value: &Value) -> String {
    match value {
        Value::String(text) => format!("\"{text}\""),
        other => other.to_string(),
    }
}

/// A doc comment, one `*` line per source line, so a paragraph in Rust reads as
/// a paragraph in the editor's tooltip.
fn comment(text: &str, indent: &str) -> String {
    let mut out = format!("{indent}/**\n");
    for line in text.lines() {
        let _ = writeln!(out, "{indent} * {line}");
    }
    let _ = writeln!(out, "{indent} */");
    out
}
