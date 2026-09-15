//! `Diagnostics` ordering and the JSON form (PRD §34.9).

use liyasa_core::diagnostics::code;
use liyasa_core::{Diagnostic, Diagnostics, Severity, SourceId, Span};

fn at(source: u32, start: u32) -> Span {
    Span::new(SourceId(source), start, start + 1)
}

#[test]
fn diagnostics_stay_sorted_by_source_then_start() {
    let mut list = Diagnostics::new();
    list.push(Diagnostic::new(code::E0310, "third").at(at(1, 5)));
    list.push(Diagnostic::new(code::E0311, "first").at(at(0, 9)));
    list.push(Diagnostic::new(code::E0312, "second").at(at(1, 2)));
    let messages: Vec<_> = list.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(messages, ["first", "second", "third"]);
}

#[test]
fn spanless_diagnostics_sort_first_and_keep_insertion_order() {
    let mut list = Diagnostics::new();
    list.push(Diagnostic::new(code::E0701, "aggregate"));
    list.push(Diagnostic::new(code::E0310, "located").at(at(0, 0)));
    list.push(Diagnostic::new(code::W0702, "later aggregate"));
    let messages: Vec<_> = list.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(messages, ["aggregate", "later aggregate", "located"]);
}

#[test]
fn has_errors_ignores_warnings() {
    let mut list = Diagnostics::new();
    list.push(Diagnostic::new(code::W0306, "heading level skipped"));
    assert!(!list.has_errors());
    list.push(Diagnostic::new(code::E0310, "unclosed"));
    assert!(list.has_errors());
}

#[test]
fn severity_defaults_to_the_registry_row() {
    assert_eq!(
        Diagnostic::new(code::W0713, "no id").severity,
        Severity::Warning
    );
    assert_eq!(
        Diagnostic::new(code::E0208, "reader").severity,
        Severity::Error
    );
}

#[test]
fn the_json_form_round_trips() {
    let diagnostic = Diagnostic::new(code::E0311, "close without open")
        .at(at(3, 12))
        .label(at(3, 4), "opened here")
        .help("use the same number of colons");
    let json = serde_json::to_string(&diagnostic).expect("serializes");
    assert!(json.contains("\"code\":\"E0311\""), "{json}");
    assert!(json.contains("https://kasecrab.github.io/liyasa/docs/errors/E0311"), "{json}");
    let back: Diagnostic = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(back, diagnostic);
}

#[test]
fn empty_optional_fields_are_omitted() {
    let json =
        serde_json::to_string(&Diagnostic::new(code::E0701, "build failed")).expect("serializes");
    assert!(!json.contains("labels"), "{json}");
    assert!(!json.contains("related"), "{json}");
    assert!(!json.contains("span"), "{json}");
}
