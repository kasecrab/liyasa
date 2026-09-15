//! `spec/markdown/cm-37/fences/`.

use super::*;

fn info(text: &str) -> FenceInfo {
    let parsed = parse(text);
    assert_eq!(parsed.unknown, Vec::new(), "unexpected unknown in `{text}`");
    parsed.info
}

#[test]
fn a_bare_language() {
    let info = info("rust");
    assert_eq!(info.lang.as_deref(), Some("rust"));
    assert!(info.attrs.flags.is_empty());
    assert!(info.attrs.kv.is_empty());
}

#[test]
fn an_empty_info_string_has_no_language() {
    assert_eq!(info("").lang, None);
    assert_eq!(info("   ").lang, None);
}

#[test]
fn a_flag_attribute() {
    let info = info("rust wrap");
    assert_eq!(info.lang.as_deref(), Some("rust"));
    assert!(info.attrs.flags.contains("wrap"));
}

#[test]
fn a_key_value_attribute() {
    let info = info("rust start=10");
    assert_eq!(info.attrs.kv.get("start").map(String::as_str), Some("10"));
}

#[test]
fn a_quoted_value_may_contain_spaces() {
    let info = info(r#"rust title="src/main.rs is here""#);
    assert_eq!(
        info.attrs.kv.get("title").map(String::as_str),
        Some("src/main.rs is here")
    );
}

#[test]
fn highlight_ranges() {
    assert_eq!(info("rust {1,3-5}").attrs.highlight, [(1, 1), (3, 5)]);
    assert_eq!(info("rust {2}").attrs.highlight, [(2, 2)]);
    assert_eq!(info("{4}").attrs.highlight, [(4, 4)]);
}

#[test]
fn a_highlight_range_binds_to_no_language() {
    assert_eq!(info("{1}").lang, None);
}

#[test]
fn a_braced_value_on_a_key() {
    assert_eq!(
        info("rust focus={2}")
            .attrs
            .kv
            .get("focus")
            .map(String::as_str),
        Some("2")
    );
}

#[test]
fn several_attributes_at_once() {
    let info = info(r#"rust title="a" {1,2} wrap copy=false"#);
    assert_eq!(info.lang.as_deref(), Some("rust"));
    assert_eq!(info.attrs.highlight, [(1, 1), (2, 2)]);
    assert!(info.attrs.flags.contains("wrap"));
    assert_eq!(info.attrs.kv.get("copy").map(String::as_str), Some("false"));
    assert_eq!(info.attrs.kv.get("title").map(String::as_str), Some("a"));
}

#[test]
fn an_unknown_attribute_is_w0302_and_is_kept() {
    let parsed = parse("rust sparkle=yes");
    assert_eq!(parsed.unknown.len(), 1);
    assert_eq!(parsed.unknown[0].0, "sparkle");
    assert_eq!(
        parsed.info.attrs.kv.get("sparkle").map(String::as_str),
        Some("yes")
    );
    let raised = parsed.diagnostics(SourceId(0), 0);
    assert_eq!(raised.len(), 1);
    assert_eq!(raised[0].code.as_str(), "W0302");
}

#[test]
fn an_unknown_attribute_span_covers_it() {
    let text = "rust sparkle=yes";
    let parsed = parse(text);
    let (_, at) = &parsed.unknown[0];
    assert_eq!(&text[at.start as usize..at.end as usize], "sparkle=yes");
}

#[test]
fn every_recognized_attribute_is_accepted() {
    for name in RECOGNIZED {
        assert!(parse(&format!("rust {name}")).unknown.is_empty(), "{name}");
        assert!(
            parse(&format!("rust {name}=x")).unknown.is_empty(),
            "{name}"
        );
    }
}

#[test]
fn malformed_info_strings_always_terminate() {
    for text in [
        "{",
        "}",
        "rust {",
        "rust \"",
        "rust a=\"",
        "rust =",
        "rust ==",
        "{-}",
        "{a-b}",
    ] {
        let _ = parse(text);
    }
}
