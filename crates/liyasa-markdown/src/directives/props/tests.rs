//! The prop grammar cases of `spec/markdown/cm-51/props/`, plus the malformed
//! tails the corpus cannot express because comrak would not call them
//! directives.

use liyasa_core::document::PropValue;

use super::*;

fn props(text: &str) -> Vec<(String, PropValue)> {
    let parsed = parse(text);
    assert_eq!(parsed.errors, Vec::new(), "unexpected errors for `{text}`");
    parsed.props.0.into_iter().collect()
}

fn one(text: &str) -> PropValue {
    let mut all = props(text);
    assert_eq!(all.len(), 1, "expected one prop in `{text}`");
    all.remove(0).1
}

#[test]
fn no_props() {
    assert_eq!(props(""), []);
    assert_eq!(props("   "), []);
}

#[test]
fn strings() {
    assert_eq!(
        one(r#"{title="Install the CLI"}"#),
        PropValue::Str("Install the CLI".to_owned())
    );
    assert_eq!(one(r#"{title=""}"#), PropValue::Str(String::new()));
    assert_eq!(
        one(r#"{title="安装 · Установка"}"#),
        PropValue::Str("安装 · Установка".to_owned())
    );
    assert_eq!(
        one(r#"{href="https://example.com"}"#),
        PropValue::Str("https://example.com".to_owned())
    );
}

/// The whole point of keeping props out of a marker comment: no byte in a
/// value can terminate the construct that carries it.
#[test]
fn values_may_contain_delimiters() {
    assert_eq!(
        one(r#"{title="a --> b"}"#),
        PropValue::Str("a --> b".to_owned())
    );
    assert_eq!(
        one(r#"{title="a } b"}"#),
        PropValue::Str("a } b".to_owned())
    );
    assert_eq!(
        one(r#"{title="a `code` b"}"#),
        PropValue::Str("a `code` b".to_owned())
    );
    assert_eq!(
        one(r#"{title="a:b:c"}"#),
        PropValue::Str("a:b:c".to_owned())
    );
}

#[test]
fn numbers_and_bools() {
    assert_eq!(one("{columns=3}"), PropValue::Num(3.0));
    assert_eq!(one("{ratio=1.5}"), PropValue::Num(1.5));
    assert_eq!(one("{offset=-2}"), PropValue::Num(-2.0));
    assert_eq!(one("{open=true}"), PropValue::Bool(true));
    assert_eq!(one("{open=false}"), PropValue::Bool(false));
}

/// `inf` and `nan` parse as `f64` and have no JSON form.
#[test]
fn non_finite_tokens_stay_strings() {
    assert_eq!(one("{ratio=inf}"), PropValue::Str("inf".to_owned()));
    assert_eq!(one("{ratio=NaN}"), PropValue::Str("NaN".to_owned()));
}

#[test]
fn lists() {
    assert_eq!(
        one("{tags=[alpha,beta]}"),
        PropValue::List(vec![
            PropValue::Str("alpha".to_owned()),
            PropValue::Str("beta".to_owned()),
        ])
    );
    assert_eq!(
        one(r#"{tags=["a b","c"]}"#),
        PropValue::List(vec![
            PropValue::Str("a b".to_owned()),
            PropValue::Str("c".to_owned()),
        ])
    );
    assert_eq!(one("{tags=[]}"), PropValue::List(Vec::new()));
}

#[test]
fn a_quoted_comma_does_not_split_a_list() {
    assert_eq!(
        one(r#"{tags=["a,b"]}"#),
        PropValue::List(vec![PropValue::Str("a,b".to_owned())])
    );
}

#[test]
fn expressions_are_kept_unevaluated() {
    assert_eq!(
        one("{href={{ page.url }}}"),
        PropValue::Expr("page.url".to_owned())
    );
}

#[test]
fn shorthands() {
    assert_eq!(one("{.wide}"), PropValue::Str("wide".to_owned()));
    assert_eq!(one("{#install}"), PropValue::Str("install".to_owned()));
    assert_eq!(
        props("{.wide #install}"),
        [
            ("class".to_owned(), PropValue::Str("wide".to_owned())),
            ("id".to_owned(), PropValue::Str("install".to_owned())),
        ]
    );
}

/// `{.a .b}` means both classes, as it does in HTML.
#[test]
fn classes_accumulate() {
    assert_eq!(one("{.a .b}"), PropValue::Str("a b".to_owned()));
}

#[test]
fn keys_may_be_dashed_or_underscored() {
    assert_eq!(
        props(r#"{data-test="x"}"#),
        [("data-test".to_owned(), PropValue::Str("x".to_owned()))]
    );
    assert_eq!(
        props(r#"{data_test="x"}"#),
        [("data_test".to_owned(), PropValue::Str("x".to_owned()))]
    );
}

#[test]
fn spaces_around_the_equals_sign() {
    assert_eq!(
        props(r#"{ title = "A" }"#),
        [("title".to_owned(), PropValue::Str("A".to_owned()))]
    );
}

#[test]
fn many_props_on_one_directive() {
    assert_eq!(
        props(r#"{title="A" columns=2 open=true .wide #x}"#),
        [
            ("class".to_owned(), PropValue::Str("wide".to_owned())),
            ("columns".to_owned(), PropValue::Num(2.0)),
            ("id".to_owned(), PropValue::Str("x".to_owned())),
            ("open".to_owned(), PropValue::Bool(true)),
            ("title".to_owned(), PropValue::Str("A".to_owned())),
        ]
    );
}

#[test]
fn a_prop_span_covers_what_was_written() {
    let parsed = parse(r#"{title="A" columns=2}"#);
    let at = parsed.span_of("columns").expect("columns has a span");
    assert_eq!(
        &r#"{title="A" columns=2}"#[at.start as usize..at.end as usize],
        "columns=2"
    );
}

#[test]
fn a_tail_that_is_not_braced_is_an_error() {
    let parsed = parse(" title=1");
    assert_eq!(parsed.errors.len(), 1);
    assert!(parsed.props.is_empty());
}

#[test]
fn an_unclosed_brace_is_an_error() {
    let parsed = parse(r#"{title="A""#);
    assert_eq!(parsed.errors.len(), 1);
    assert!(parsed.props.is_empty());
}

#[test]
fn an_unterminated_string_is_an_error_and_keeps_what_came_before() {
    let parsed = parse(r#"{columns=2 title="A}"#);
    assert_eq!(parsed.errors.len(), 1);
    assert_eq!(parsed.props.get("columns"), Some(&PropValue::Num(2.0)));
}

#[test]
fn a_prop_without_a_value_is_an_error() {
    let parsed = parse("{open}");
    assert_eq!(parsed.errors.len(), 1);
    assert!(parsed.props.is_empty());
}

#[test]
fn a_bare_shorthand_marker_is_an_error() {
    let parsed = parse("{. }");
    assert_eq!(parsed.errors.len(), 1);
}

/// Every malformed tail must terminate, whatever it contains.
#[test]
fn malformed_tails_always_terminate() {
    for text in [
        "{=}", "{==}", "{ = }", "{.}", "{#}", "{a=}", "{a==}", "{\"}", "{{{}", "{[}", "{a=[}",
        "{a={{}", "{ }", "{..}", "{a=1 =2}", "{🙂=1}", "{a=🙂}",
    ] {
        let _ = parse(text);
    }
}
