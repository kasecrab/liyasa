//! `spec/markdown/cm-50/inline/`.

use liyasa_core::document::PropValue;

use super::*;

fn text(value: &str) -> Inline {
    Inline::Text(value.to_owned())
}

fn component(name: &str, children: Vec<Inline>) -> Inline {
    Inline::InlineComponent {
        name: name.to_owned(),
        props: Props::default(),
        children,
    }
}

#[test]
fn a_directive_between_text() {
    assert_eq!(
        scan(vec![text("Press :kbd[Ctrl+K] now.")]),
        [
            text("Press "),
            component("kbd", vec![text("Ctrl+K")]),
            text(" now."),
        ]
    );
}

#[test]
fn a_directive_alone() {
    assert_eq!(
        scan(vec![text(":kbd[Ctrl+K]")]),
        [component("kbd", vec![text("Ctrl+K")])]
    );
}

#[test]
fn props_follow_the_brackets() {
    let scanned = scan(vec![text(r#":kbd[Ctrl+K]{.wide}"#)]);
    let [Inline::InlineComponent { name, props, .. }] = scanned.as_slice() else {
        panic!("expected one component, got {scanned:?}");
    };
    assert_eq!(name, "kbd");
    assert_eq!(props.get("class"), Some(&PropValue::Str("wide".to_owned())));
}

#[test]
fn a_brace_that_is_not_props_stays_text() {
    assert_eq!(
        scan(vec![text(":kbd[Ctrl+K] {not props}")]),
        [component("kbd", vec![text("Ctrl+K")]), text(" {not props}"),]
    );
}

#[test]
fn content_keeps_the_markdown_comrak_already_parsed() {
    assert_eq!(
        scan(vec![
            text("see :note[a "),
            Inline::Emph(vec![text("b")]),
            text(" c] end"),
        ]),
        [
            text("see "),
            component(
                "note",
                vec![text("a "), Inline::Emph(vec![text("b")]), text(" c")]
            ),
            text(" end"),
        ]
    );
}

#[test]
fn directives_nest() {
    assert_eq!(
        scan(vec![text(":a[x :b[y] z]")]),
        [component(
            "a",
            vec![text("x "), component("b", vec![text("y")]), text(" z"),]
        )]
    );
}

#[test]
fn a_leaf_marker_is_not_an_inline_directive() {
    assert_eq!(scan(vec![text("::image[x]")]), [text("::image[x]")]);
}

#[test]
fn a_container_marker_is_not_an_inline_directive() {
    assert_eq!(scan(vec![text(":::note[x]")]), [text(":::note[x]")]);
}

#[test]
fn a_bare_colon_is_not_an_inline_directive() {
    assert_eq!(scan(vec![text("a: b")]), [text("a: b")]);
    assert_eq!(scan(vec![text(":[x]")]), [text(":[x]")]);
}

#[test]
fn a_link_is_not_an_inline_directive() {
    assert_eq!(
        scan(vec![text("[label](/route)")]),
        [text("[label](/route)")]
    );
}

/// An unterminated directive is prose, put back the way it was written.
#[test]
fn an_unclosed_directive_is_restored() {
    assert_eq!(
        scan(vec![text("Press :kbd[Ctrl+K now.")]),
        [text("Press :kbd[Ctrl+K now.")]
    );
}

#[test]
fn an_unclosed_nested_directive_is_restored() {
    assert_eq!(scan(vec![text(":a[x :b[y")]), [text(":a[x :b[y")]);
}

#[test]
fn a_stray_bracket_stays_text() {
    assert_eq!(scan(vec![text("a ] b")]), [text("a ] b")]);
}

#[test]
fn a_directive_may_span_inline_nodes() {
    assert_eq!(
        scan(vec![
            text("Press :kbd[Ctrl+"),
            Inline::SoftBreak,
            text("K] now."),
        ]),
        [
            text("Press "),
            component("kbd", vec![text("Ctrl+"), Inline::SoftBreak, text("K")]),
            text(" now."),
        ]
    );
}

#[test]
fn directives_inside_emphasis_are_found() {
    assert_eq!(
        scan(vec![Inline::Emph(vec![text(":kbd[K]")])]),
        [Inline::Emph(vec![component("kbd", vec![text("K")])])]
    );
}

#[test]
fn code_spans_are_left_alone() {
    assert_eq!(
        scan(vec![Inline::Code(":kbd[K]".to_owned())]),
        [Inline::Code(":kbd[K]".to_owned())]
    );
}

#[test]
fn an_empty_directive_has_no_children() {
    assert_eq!(scan(vec![text(":kbd[]")]), [component("kbd", Vec::new())]);
}
