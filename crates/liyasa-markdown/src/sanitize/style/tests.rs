use super::*;

#[test]
fn allowed_properties_survive() {
    assert_eq!(
        filter("color: red; text-align: center"),
        Some("color: red; text-align: center".to_owned())
    );
}

#[test]
fn property_names_are_case_insensitive() {
    assert_eq!(filter("COLOR: red"), Some("color: red".to_owned()));
}

#[test]
fn unlisted_properties_are_dropped() {
    assert_eq!(filter("position: fixed"), None);
    assert_eq!(filter("z-index: 99"), None);
    assert_eq!(filter("opacity: 0"), None);
    assert_eq!(
        filter("color: red; position: fixed"),
        Some("color: red".to_owned())
    );
}

/// `url()` is a network request that reports who opened the page.
#[test]
fn url_and_expression_are_rejected_wherever_they_appear() {
    assert_eq!(filter("background-color: url(https://x/y)"), None);
    assert_eq!(filter("background-color: url( https://x/y )"), None);
    assert_eq!(filter("width: expression(alert(1))"), None);
    assert_eq!(filter("color: red /* url(x) */"), None);
}

#[test]
fn an_at_rule_is_rejected() {
    assert_eq!(
        filter("color: red; @import 'x'"),
        Some("color: red".to_owned())
    );
    assert_eq!(filter("@import 'x'"), None);
}

#[test]
fn nothing_left_is_none() {
    assert_eq!(filter(""), None);
    assert_eq!(filter("   "), None);
    assert_eq!(filter("not-a-declaration"), None);
    assert_eq!(filter(";;;"), None);
}

/// `u\72 l(x)` is `url(x)` to a browser and is not to a substring search.
#[test]
fn a_css_escape_cannot_spell_a_rejected_keyword() {
    assert_eq!(filter(r"background-color: u\72 l(https://tracker/x)"), None);
    assert_eq!(filter(r"color: \72 ed"), None);
}
