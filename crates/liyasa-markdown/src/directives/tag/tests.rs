//! `spec/markdown/cm-53/tag-form/`.

use super::*;

fn props(html: &str) -> Vec<(String, PropValue)> {
    parse(html)
        .unwrap_or_else(|| panic!("`{html}` is a component tag"))
        .props
        .0
        .into_iter()
        .collect()
}

#[test]
fn an_uppercase_tag_is_a_component() {
    let tag = parse(r#"<Card title="Install">"#).expect("a component");
    assert_eq!(tag.name, "Card");
    assert_eq!(tag.form, Form::Open);
    assert_eq!(
        tag.props.get("title"),
        Some(&PropValue::Str("Install".to_owned()))
    );
}

#[test]
fn a_lowercase_tag_stays_raw_html() {
    assert_eq!(parse(r#"<div class="x">"#), None);
    assert_eq!(parse("</div>"), None);
    assert_eq!(parse("<br/>"), None);
}

#[test]
fn closing_and_self_closing_forms() {
    assert_eq!(parse("</Card>").expect("a component").form, Form::Close);
    let tag = parse(r#"<Image src="/a.png" />"#).expect("a component");
    assert_eq!(tag.form, Form::SelfClosing);
    assert_eq!(
        tag.props.get("src"),
        Some(&PropValue::Str("/a.png".to_owned()))
    );
}

/// `</Card title="x">` is not a closing tag in HTML and is not one here.
#[test]
fn a_closing_tag_carries_nothing() {
    assert_eq!(parse(r#"</Card title="x">"#), None);
}

#[test]
fn attribute_values_are_typed_like_props() {
    assert_eq!(
        props("<Card columns=2 open wide='yes'>"),
        [
            ("columns".to_owned(), PropValue::Num(2.0)),
            ("open".to_owned(), PropValue::Bool(true)),
            ("wide".to_owned(), PropValue::Str("yes".to_owned())),
        ]
    );
}

#[test]
fn an_expression_attribute_stays_unevaluated() {
    assert_eq!(
        props("<Card href={{ page.url }}>"),
        [("href".to_owned(), PropValue::Expr("page.url".to_owned()))]
    );
}

#[test]
fn an_attribute_value_may_contain_a_comment_terminator() {
    assert_eq!(
        props(r#"<Card title="a --> b">"#),
        [("title".to_owned(), PropValue::Str("a --> b".to_owned()))]
    );
}

#[test]
fn an_inline_tag_is_a_component_too() {
    assert_eq!(parse("<Kbd>").expect("a component").name, "Kbd");
}

#[test]
fn tag_names_map_to_kebab_case_directive_names() {
    assert_eq!(directive_name("Card"), "card");
    assert_eq!(directive_name("CodeGroup"), "code-group");
    assert_eq!(directive_name("APIRef"), "a-p-i-ref");
    assert_eq!(directive_name("Code-Group"), "code-group");
}

#[test]
fn text_that_is_not_a_tag_is_not_a_tag() {
    assert_eq!(parse("Card"), None);
    assert_eq!(parse("<>"), None);
    assert_eq!(parse("<1Card>"), None);
    assert_eq!(parse(""), None);
}

/// A malformed attribute list must terminate, whatever it contains.
#[test]
fn malformed_attribute_lists_always_terminate() {
    for html in [
        "<Card =>",
        "<Card ==>",
        "<Card a=>",
        "<Card \">",
        "<Card a='>",
        "<Card {{>",
        "<Card 🙂>",
        "<Card a=🙂>",
        "<Card a=b=c>",
    ] {
        let _ = parse(html);
    }
}
