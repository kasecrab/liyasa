use liyasa_core::SourceId;
use liyasa_core::document::PropValue;

use super::*;

const SOURCE: SourceId = SourceId(0);

#[test]
fn bare_name() {
    let info = parse("note");
    assert_eq!(info.name, "note");
    assert!(info.props.is_empty());
    assert!(info.diagnostics(SOURCE, 0).is_empty());
}

#[test]
fn name_and_props() {
    let info = parse(r#"card{title="A" columns=2}"#);
    assert_eq!(info.name, "card");
    assert_eq!(
        info.props.get("title"),
        Some(&PropValue::Str("A".to_owned()))
    );
    assert_eq!(info.props.get("columns"), Some(&PropValue::Num(2.0)));
    assert!(info.diagnostics(SOURCE, 0).is_empty());
}

/// comrak reports a bare `:::` as a directive with an empty info string.
#[test]
fn an_empty_info_string_has_no_name() {
    let info = parse("");
    assert!(info.is_empty());
    assert!(info.diagnostics(SOURCE, 0).is_empty());
}

#[test]
fn a_prop_span_points_into_the_info_string() {
    let text = r#"card{title="A"}"#;
    let info = parse(text);
    let at = info.span_of("title").expect("title has a span");
    assert_eq!(&text[at.start as usize..at.end as usize], r#"title="A""#);
}

#[test]
fn the_name_span_covers_the_name() {
    let text = "  note{.wide}";
    let info = parse(text);
    assert_eq!(
        &text[info.name_at.start as usize..info.name_at.end as usize],
        "note"
    );
}

#[test]
fn a_malformed_prop_list_is_e0312_and_not_a_rejection() {
    let info = parse(r#"card{title="A}"#);
    assert_eq!(info.name, "card");
    let raised = info.diagnostics(SOURCE, 0);
    assert_eq!(raised.len(), 1);
    assert_eq!(raised[0].code.as_str(), "E0312");
}

#[test]
fn props_without_a_name_are_e0312() {
    let info = parse("{.wide}");
    assert!(info.name.is_empty());
    let raised = info.diagnostics(SOURCE, 0);
    assert_eq!(raised.len(), 1);
    assert_eq!(raised[0].code.as_str(), "E0312");
}

/// The whole reason containers left the marker rewrite behind.
#[test]
fn a_value_containing_a_comment_terminator_is_just_bytes() {
    let info = parse(r#"card{title="a --> b"}"#);
    assert_eq!(
        info.props.get("title"),
        Some(&PropValue::Str("a --> b".to_owned()))
    );
    assert!(info.diagnostics(SOURCE, 0).is_empty());
}
