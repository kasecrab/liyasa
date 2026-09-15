//! Publishes the JSON forms of the frozen types (PRD §34.9).
//!
//! The Rust declarations are the source of truth for these four; the site
//! serves the generated files at `/_liyasa/schema/<name>.json`. The reverse
//! direction holds for `liyasa.schema.json`, which defines `SiteConfig` and is
//! written by hand (CFG-94).

use std::path::Path;

use liyasa_core::diagnostics::Diagnostic;
use liyasa_core::document::{Document, SourceDocument};
use liyasa_core::frontmatter::FrontmatterFields;

/// `major.minor`; adding an optional field or an enum variant bumps the minor,
/// anything else bumps the major (§34.9).
pub const SCHEMA_VERSION: &str = "1.0";

pub struct Generated {
    pub file: &'static str,
    pub json: String,
}

pub fn generate() -> Vec<Generated> {
    vec![
        one::<SourceDocument>("source-document.json", "Liyasa Source Document"),
        one::<Document>("ast.json", "Liyasa Rendered AST"),
        one::<Diagnostic>("diagnostic.json", "Liyasa diagnostic"),
        one::<FrontmatterFields>("frontmatter.json", "Liyasa page front matter"),
    ]
}

fn one<T: schemars::JsonSchema>(file: &'static str, title: &str) -> Generated {
    let mut schema = schemars::schema_for!(T);
    let object = schema.ensure_object();
    object.insert(
        "$id".into(),
        format!("https://liyasa.dev/schema/v1/{file}").into(),
    );
    object.insert("title".into(), title.into());
    object.insert("schemaVersion".into(), SCHEMA_VERSION.into());
    let mut json = serde_json::to_string_pretty(&schema).expect("schema serializes");
    json.push('\n');
    Generated { file, json }
}

/// Writes the generated schemas, or reports which are out of date.
pub fn run(dir: &Path, check: bool) -> Result<(), String> {
    let mut stale = Vec::new();
    for generated in generate() {
        let path = dir.join(generated.file);
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current == generated.json {
            continue;
        }
        if check {
            stale.push(generated.file);
            continue;
        }
        std::fs::write(&path, &generated.json).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    if stale.is_empty() {
        return Ok(());
    }
    Err(format!(
        "out of date: {}\nrun `cargo run -p xtask -- schemas` and commit the result",
        stale.join(", ")
    ))
}
