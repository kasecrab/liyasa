//! The `json` and `yaml` schema runner (VER-02.3).
//!
//! Two assertions, in order: the block is a well-formed document in its
//! language, and — when the block declared `schema=` — the document satisfies
//! that schema. `CheckInput::Schema` carries the schema text, not the path:
//! `$ref` resolution and file reading go through `Vfs` and `HttpClient` before
//! the check reaches a runner (§6.2, RFC 1303).

use std::time::Instant;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::net::BoxFut;
use liyasa_core::verify::{
    CheckInput, CheckOutcome, CheckResult, CheckSpec, Isolation, Runner, Sandbox, SecretSource,
};
use serde_json::Value;

use super::{fail, finish, ready, scrubber_for, skip};
use crate::core::scrub::Scrubber;

pub struct SchemaRunner;

impl SchemaRunner {
    pub const ID: &'static str = "schema";
}

impl Runner for SchemaRunner {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn languages(&self) -> &'static [&'static str] {
        &["json", "jsonc", "yaml", "yml"]
    }

    fn isolation(&self) -> Isolation {
        Isolation::InProcess
    }

    fn run<'a>(
        &'a self,
        spec: &'a CheckSpec,
        _sandbox: &'a dyn Sandbox,
        secrets: &'a dyn SecretSource,
    ) -> BoxFut<'a, CheckResult> {
        let started = Instant::now();
        let scrubber = scrubber_for(spec, secrets);
        let outcome = match &spec.input {
            CheckInput::Schema {
                lang,
                source,
                schema,
            } => self.check(lang, source, schema, &scrubber),
            CheckInput::Code { lang, source, .. } => self.check(lang, source, "", &scrubber),
            _ => skip("the schema runner reads code blocks only"),
        };
        ready(finish(spec, Self::ID, outcome, started))
    }
}

impl SchemaRunner {
    fn check(&self, lang: &str, source: &str, schema: &str, scrubber: &Scrubber) -> CheckOutcome {
        let lang = lang.trim().to_ascii_lowercase();
        if !self.languages().contains(&lang.as_str()) {
            return skip(format!("the schema runner does not claim `{lang}`"));
        }
        let document = match parse(&lang, source) {
            Ok(document) => document,
            Err(message) => return fail(scrubber, message),
        };
        if schema.trim().is_empty() {
            // No `schema=`: the block asserted only that it parses, and it did.
            return CheckOutcome::Pass;
        }
        // The schema is the author's, so an unreadable one is an authoring
        // error rather than a failing document.
        let schema_value = match parse_either(schema) {
            Ok(value) => value,
            Err(message) => {
                return CheckOutcome::Error(Diagnostic::new(
                    code::E0601,
                    format!("the `schema=` document is not readable: {message}"),
                ));
            }
        };
        let validator = match jsonschema::validator_for(&schema_value) {
            Ok(validator) => validator,
            Err(error) => {
                return CheckOutcome::Error(Diagnostic::new(
                    code::E0601,
                    format!("the `schema=` document is not a JSON Schema: {error}"),
                ));
            }
        };
        let problems: Vec<String> = validator
            .iter_errors(&document)
            .map(|error| format!("{}: {error}", pointer(error.instance_path())))
            .collect();
        if problems.is_empty() {
            CheckOutcome::Pass
        } else {
            fail(scrubber, problems.join("\n"))
        }
    }
}

fn parse(lang: &str, source: &str) -> Result<Value, String> {
    match lang {
        "json" | "jsonc" => serde_json::from_str(source).map_err(|e| format!("not JSON: {e}")),
        _ => serde_norway::from_str(source).map_err(|e| format!("not YAML: {e}")),
    }
}

/// A schema may be written in either language; JSON is tried first because
/// every JSON document is also YAML and the JSON error message is better.
fn parse_either(text: &str) -> Result<Value, String> {
    serde_json::from_str(text)
        .or_else(|_| serde_norway::from_str(text))
        .map_err(|e: serde_norway::Error| e.to_string())
}

fn pointer(path: &jsonschema::paths::Location) -> String {
    let text = path.to_string();
    if text.is_empty() {
        "the document".to_owned()
    } else {
        text
    }
}
