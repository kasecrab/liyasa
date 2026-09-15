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

use liyasa_core::span::SourceId;

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

/// The codes expansion's syntactic pass raises before anything is rendered.
pub const RAISED_ON_EXPANSION: &[&str] = &["E0208"];

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
