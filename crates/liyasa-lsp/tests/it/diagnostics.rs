//! Liyasa diagnostics as the editor receives them.

use liyasa_lsp::analysis::Analysis;
use liyasa_lsp::diagnostics::{self, SOURCE};
use liyasa_lsp::protocol::{DiagnosticSeverity, PositionEncoding};
use liyasa_lsp::workspace::Workspace;

const URI: &str = "file:///docs/a.md";

fn convert(source: &str) -> Vec<liyasa_lsp::protocol::Diagnostic> {
    let workspace = Workspace::new();
    let analysis = Analysis::of("a.md", source, &workspace);
    diagnostics::convert_all(
        analysis.diagnostics.clone(),
        URI,
        &analysis.text,
        PositionEncoding::Utf16,
    )
}

#[test]
fn the_code_and_its_help_article_reach_the_editor() {
    let published = convert(":::note\nBody.\n");
    let unclosed = published
        .iter()
        .find(|d| d.code == "E0310")
        .expect("the unclosed container is published");
    assert_eq!(unclosed.severity, DiagnosticSeverity::Error);
    assert_eq!(unclosed.source, SOURCE);
    assert!(
        unclosed.code_description.href.ends_with("E0310"),
        "the code links to its article: {}",
        unclosed.code_description.href
    );
}

#[test]
fn help_text_travels_on_the_message_because_the_protocol_has_no_field_for_it() {
    let published = convert(":::nte\nBody.\n:::\n");
    let unknown = published
        .iter()
        .find(|d| d.code == "E0313")
        .expect("the unknown component is published");
    assert!(
        unknown.message.contains("note"),
        "the suggestion survives: {}",
        unknown.message
    );
}

#[test]
fn a_range_lands_on_the_construct_that_is_wrong() {
    let published = convert("# Title\n\n:::note\nBody.\n");
    let unclosed = published
        .iter()
        .find(|d| d.code == "E0310")
        .expect("the unclosed container is published");
    assert_eq!(unclosed.range.start.line, 2);
    assert_eq!(unclosed.range.start.character, 0);
}

#[test]
fn a_diagnostic_with_no_span_belongs_to_the_top_of_the_file() {
    let workspace = Workspace::new();
    let analysis = Analysis::of("a.md", "# Title\n", &workspace);
    let raw = liyasa_core::Diagnostic::new(
        liyasa_core::diagnostics::code::E0202,
        "something the scanner could not place",
    );
    let converted = diagnostics::convert(&raw, URI, &analysis.text, PositionEncoding::Utf16);
    assert_eq!(converted.range, liyasa_lsp::protocol::Range::default());
}

#[test]
fn the_wire_form_is_the_json_an_editor_expects() {
    let published = convert(":::note\nBody.\n");
    let json = serde_json::to_value(&published[0]).expect("a diagnostic serializes");
    for field in [
        "range",
        "severity",
        "code",
        "codeDescription",
        "source",
        "message",
    ] {
        assert!(json.get(field).is_some(), "{field} is on the wire: {json}");
    }
    assert_eq!(json["severity"], serde_json::json!(1), "Error is 1");
}
