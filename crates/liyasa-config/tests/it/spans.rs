//! JSON pointer to source span, the thing that makes a config diagnostic
//! point at a line (PRD §34.5).

use liyasa_config::json::SpanIndex;
use liyasa_core::span::SourceId;

const SOURCE: SourceId = SourceId(7);

const TEXT: &str = r##"{
  "name": "Acme",
  "theme": { "colors": { "primary": "#4F46E5" } },
  "navigation": ["index", { "group": "Get started", "pages": ["a/b"] }],
  "build": { "drafts": false, "cacheSize": 2 },
  "odd~key/here": null
}"##;

fn index() -> SpanIndex {
    SpanIndex::scan(SOURCE, TEXT)
}

fn text_at(index: &SpanIndex, pointer: &str) -> String {
    let span = index
        .value(pointer)
        .unwrap_or_else(|| panic!("no span for `{pointer}`"));
    assert_eq!(span.source, SOURCE);
    TEXT[span.start as usize..span.end as usize].to_owned()
}

#[test]
fn the_document_itself_is_the_empty_pointer() {
    assert_eq!(text_at(&index(), ""), TEXT);
}

#[test]
fn scalars_carry_their_own_text() {
    let index = index();
    assert_eq!(text_at(&index, "/name"), "\"Acme\"");
    assert_eq!(text_at(&index, "/build/drafts"), "false");
    assert_eq!(text_at(&index, "/build/cacheSize"), "2");
    assert_eq!(text_at(&index, "/odd~0key~1here"), "null");
}

#[test]
fn nesting_and_array_indices_resolve() {
    let index = index();
    assert_eq!(text_at(&index, "/theme/colors/primary"), "\"#4F46E5\"");
    assert_eq!(text_at(&index, "/navigation/0"), "\"index\"");
    assert_eq!(text_at(&index, "/navigation/1/group"), "\"Get started\"");
    assert_eq!(text_at(&index, "/navigation/1/pages/0"), "\"a/b\"");
    assert_eq!(
        text_at(&index, "/navigation/1"),
        r#"{ "group": "Get started", "pages": ["a/b"] }"#
    );
}

#[test]
fn a_key_has_its_own_span() {
    let index = index();
    let key = index.key("/theme/colors").expect("the key is mapped");
    assert_eq!(&TEXT[key.start as usize..key.end as usize], "\"colors\"");
    assert!(
        index.key("/navigation/0").is_none(),
        "an array element has no key"
    );
}

#[test]
fn an_unmapped_pointer_falls_back_to_its_nearest_parent() {
    let index = index();
    let span = index
        .nearest("/theme/colors/primary/nope/deeper")
        .expect("the fallback walks up");
    assert_eq!(&TEXT[span.start as usize..span.end as usize], "\"#4F46E5\"");
    assert_eq!(index.nearest("/absent"), index.value(""));
}

#[test]
fn escapes_inside_strings_do_not_end_the_value() {
    let text = r#"{ "a": "x\"y\\", "b": 1 }"#;
    let index = SpanIndex::scan(SOURCE, text);
    let span = index.value("/b").expect("`b` is mapped");
    assert_eq!(&text[span.start as usize..span.end as usize], "1");
    let span = index.value("/a").expect("`a` is mapped");
    assert_eq!(&text[span.start as usize..span.end as usize], r#""x\"y\\""#);
}

#[test]
fn a_truncated_document_still_maps_what_it_read() {
    let index = SpanIndex::scan(SOURCE, r#"{ "name": "Acme", "theme": {"#);
    assert!(index.value("/name").is_some());
}
