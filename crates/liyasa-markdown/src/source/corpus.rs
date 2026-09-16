//! The corpus, run against the scanner (PRD §30.9).
//!
//! `xtask conformance` reports every `source-document` section and most of the
//! `diagnostics` sections as skipped, because the engines it drives are parser
//! candidates: none of them segments a page or raises a Liyasa code. That is
//! what makes those sections inert. `ast::corpus` is the parser's answer to it;
//! this is the scanner's.
//!
//! Case discovery and the `%%%` format live in [`crate::ast::corpus`]; only the
//! `source-document` section, which that loader has no use for, is read here.

use std::sync::Arc;

use liyasa_core::markdown::TemplateContext;
use liyasa_core::source_map::SourceMap;
use liyasa_core::span::SourceId;
use liyasa_core::vfs::VfsPath;

pub use crate::ast::corpus::{Case, load};

/// The codes only this pass raises.
///
/// `E0310`, `E0311`, and `E0312` are deliberately absent: the scanner and the
/// parser both raise them, and holding both to the same case would make a
/// corpus entry assert twice and fail whichever pass did not get there first.
/// They are `ast::corpus`'s.
pub const RAISED_HERE: &[&str] = &["E0101", "E0102", "E0202", "E0210", "E0212", "E0301"];

/// Raised where a value *enters* the context rather than by scanning a page, so
/// a case that expects one is fed to `escape_untrusted_markdown` instead.
pub const RAISED_ON_ENTRY: &[&str] = &["E0320"];

/// The codes a case is expanded to settle.
///
/// `E0208` is settled before anything is rendered; the rest are the filters and
/// functions of CM-14 and CM-15 answering out of [`fixture_host`] and
/// [`fixture_context`].
pub const RAISED_ON_EXPANSION: &[&str] = &[
    "E0208", "E0209", "E0211", "E0213", "E0214", "E0215", "E0216",
];

/// The build a `cm-14` or `cm-15` case is expanded against.
///
/// One fixture for the whole corpus, so a case is a statement about the filter
/// rather than about a build it carries with it. `spec/markdown/README.md`
/// records what is in it.
pub fn fixture_host() -> super::host::Host {
    use std::collections::BTreeMap;

    use super::host::{Host, PageEntry};
    Host {
        pages: vec![
            PageEntry {
                id: "index".to_owned(),
                route: "/".to_owned(),
                data: serde_json::json!({ "title": "Home" }),
            },
            PageEntry {
                id: "guides/install".to_owned(),
                route: "/guides/install".to_owned(),
                data: serde_json::json!({ "title": "Install", "draft": false }),
            },
        ],
        assets: BTreeMap::from([(
            "img/logo.svg".to_owned(),
            "/_liyasa/img/logo.9f8e7d.svg".to_owned(),
        )]),
        openapi: BTreeMap::from([(
            "petstore".to_owned(),
            BTreeMap::from([(
                "listPets".to_owned(),
                serde_json::json!({ "summary": "List pets" }),
            )]),
        )]),
        region: Some("eu".to_owned()),
        features: BTreeMap::from([("logs".to_owned(), vec!["eu".to_owned()])]),
        now: Some("2026-01-01T00:00:00Z".to_owned()),
    }
}

/// The template context a `cm-14` or `cm-15` case is expanded against.
pub fn fixture_context() -> TemplateContext {
    let mut layers = super::context::Layers {
        site_variables: serde_json::json!({ "product": "Acme" }),
        facts: serde_json::json!({ "plan": { "pro": { "price": 20 } } }),
        page: serde_json::json!({ "title": "Rate limits" }),
        site: serde_json::json!({ "name": "Acme Docs" }),
        ..super::context::Layers::default()
    };
    layers.env = serde_json::json!({ "CI": "true" });
    layers.build()
}

/// The case's source without the newline the `%%%` delimiter contributed.
///
/// The section is line-delimited, so its last newline belongs to the format
/// rather than to the page; leaving it on adds a one-byte Markdown segment that
/// no expectation in the corpus carries.
pub fn source_of(case: &Case) -> &str {
    case.source
        .strip_suffix('\n')
        .unwrap_or(case.source.as_str())
}

/// The codes `scan` raises on a case.
pub fn scanned(case: &Case) -> Vec<String> {
    let (_, diagnostics) = super::scan(source_of(case), SourceId(0));
    diagnostics
        .iter()
        .map(|d| d.code.as_str().to_owned())
        .collect()
}

/// The codes escaping the case's source as one untrusted value raises.
///
/// The value is the source without its delimiter newline: an untrusted value is
/// a scalar out of a fact or a `reader.*` field, and leaving the newline on
/// would make every such case an `E0320` about the file rather than the value.
pub fn on_entry(case: &Case) -> Vec<String> {
    match super::escape_untrusted_markdown(source_of(case), true) {
        Ok(_) => Vec::new(),
        Err(diagnostic) => vec![diagnostic.code.as_str().to_owned()],
    }
}

/// The codes expansion raises before it renders anything.
///
/// `E0208` is the one the corpus asks for: reading `reader.*` on a page that
/// did not declare `personalized: true` is a fact about the page's front matter
/// and its template text, so it is settled without a context and without a
/// value for `reader`.
pub fn on_expansion(case: &Case) -> Vec<String> {
    let text = source_of(case);
    let mut map = SourceMap::new();
    let id = map.intern(VfsPath::new("page.md"), Arc::from(text));
    let (document, diagnostics) = super::scan(text, id);
    if diagnostics.has_errors() {
        return diagnostics
            .iter()
            .map(|d| d.code.as_str().to_owned())
            .collect();
    }
    let context = fixture_context();
    let mut env = super::expand::environment(&super::expand::ExpandOptions::default());
    super::host::install(&mut env, Arc::new(fixture_host()));
    match super::expand(&map, &document, &context, &env) {
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics
            .iter()
            .map(|d| d.code.as_str().to_owned())
            .collect(),
    }
}

/// The `source-document` section, parsed, or `None` when the case has none.
pub fn expected_document(case: &Case) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(&case.path).ok()?;
    let mut body: Option<String> = None;
    for line in text.split_inclusive('\n') {
        match line.strip_prefix("%%% ").map(str::trim_end) {
            Some("source-document") => body = Some(String::new()),
            Some(name) if body.is_some() && is_section(name) => break,
            _ => {
                if let Some(body) = body.as_mut() {
                    body.push_str(line);
                }
            }
        }
    }
    serde_json::from_str(&body?).ok()
}

/// A `%%%` line is a delimiter only in front of a known section name, so `%%%`
/// in content needs no escaping (`spec/markdown/README.md`).
fn is_section(name: &str) -> bool {
    matches!(
        name,
        "case" | "source" | "html" | "markdown" | "ast" | "source-document" | "diagnostics" | "end"
    )
}

/// The scanner's answer to [`expected_document`], in the same shape.
pub fn scanned_document(case: &Case) -> serde_json::Value {
    let (document, _) = super::scan(source_of(case), SourceId(0));
    serde_json::to_value(&document).unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests;
