//! The one place YAML is parsed (PRD §6.2).
//!
//! Front matter, snippet variables, AsyncAPI documents, and fact sources all go
//! through this API so the underlying crate can be swapped without touching a
//! caller. `serde_norway` today; `saphyr` once its API stabilizes.
//!
//! Every function returns a [`Diagnostic`], never a panic: this runs on every
//! page of every build and is a fuzz target (§30.9).

use crate::diagnostics::{Diagnostic, code};
use crate::span::Span;

/// `Diagnostic` is 160 bytes, so it travels boxed in the `Err` arm of these
/// hot-path helpers.
pub type YamlError = Box<Diagnostic>;

/// Parses a YAML document into the JSON value model.
pub fn parse_value(text: &str, at: Option<Span>) -> Result<serde_json::Value, YamlError> {
    serde_norway::from_str(text).map_err(|e| describe(&e, at))
}

/// Parses a front matter block and its typed view in one pass.
pub fn parse_frontmatter<T: serde::de::DeserializeOwned>(
    text: &str,
    at: Option<Span>,
) -> Result<(serde_json::Value, T), YamlError> {
    let value = parse_value(text, at)?;
    let typed = serde_json::from_value(value.clone()).map_err(|e| {
        let mut diagnostic = Diagnostic::new(code::E0102, e.to_string());
        diagnostic.span = at;
        Box::new(diagnostic)
    })?;
    Ok((value, typed))
}

/// Serializes back to YAML, for the formatter and the editor.
pub fn to_string<T: serde::Serialize>(value: &T) -> Result<String, YamlError> {
    serde_norway::to_string(value).map_err(|e| describe(&e, None))
}

fn describe(error: &serde_norway::Error, at: Option<Span>) -> YamlError {
    let mut diagnostic = Diagnostic::new(code::E0101, error.to_string());
    diagnostic.span = at;
    Box::new(diagnostic)
}
