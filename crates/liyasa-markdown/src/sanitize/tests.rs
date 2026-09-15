//! `spec/markdown/cm-31/html/` and `spec/markdown/cm-32/sanitizer/`.
//!
//! The sanitizer runs over the Rendered AST, so these go through the whole
//! parser: what is asserted is what a theme would be handed.

use liyasa_core::document::Inline;
use liyasa_core::markdown::HtmlMode;

use crate::directives::testing::*;

fn sanitized(source: &str) -> String {
    html_of(&document(source).root).trim().to_owned()
}

/// The acceptance case of CM-32.
#[test]
fn details_survives_onclick_is_stripped_and_script_is_removed() {
    let html = sanitized("<details>\n<summary>s</summary>\nbody\n</details>\n");
    assert!(html.contains("<details>"), "{html}");
    assert!(html.contains("<summary>"), "{html}");

    let div = document("<div onclick=\"steal()\">x</div>\n");
    let html = html_of(&div.root);
    assert!(html.contains("<div>"), "{html}");
    assert!(!html.contains("onclick"), "{html}");
    assert_eq!(codes(&div), ["E0304"]);

    let script = document("<script>steal()</script>\n");
    assert!(!html_of(&script.root).contains("steal"));
    assert_eq!(codes(&script), ["E0304"]);
}

#[test]
fn with_html_off_every_raw_tag_is_removed_with_e0303() {
    let document = document_with("<div>body</div>\n", HtmlMode::Off);
    assert_eq!(html_of(&document.root).trim(), "");
    assert_eq!(codes(&document), ["E0303"]);
}

#[test]
fn with_html_allow_nothing_is_filtered() {
    let document = document_with("<div onclick=\"x\">body</div>\n", HtmlMode::Allow);
    assert!(html_of(&document.root).contains("onclick"));
    assert!(codes(&document).is_empty());
}

#[test]
fn a_scripts_content_goes_with_it() {
    assert!(!sanitized("<script>\nvar x = \"<b>\";\n</script>\n").contains("var x"));
    assert!(!sanitized("<style>\nbody { color: red }\n</style>\n").contains("color"));
}

#[test]
fn unlisted_elements_are_removed_and_their_text_is_kept() {
    let document = document("<iframe src=\"https://evil\"></iframe>\n");
    assert_eq!(sanitized("<iframe src=\"https://evil\"></iframe>\n"), "");
    assert_eq!(codes(&document), ["E0304"]);

    let html = sanitized("<form><p>keep me</p></form>\n");
    assert!(html.contains("keep me"), "{html}");
    assert!(!html.contains("<form"), "{html}");
}

#[test]
fn an_inline_tag_is_filtered_the_same_way() {
    let document = document("text <b onclick=\"x\">bold</b> text\n");
    let html = html_of(&document.root);
    assert!(html.contains("<b>"), "{html}");
    assert!(!html.contains("onclick"), "{html}");
}

#[test]
fn a_comment_does_not_survive() {
    assert_eq!(sanitized("<!-- <script>x</script> -->\n"), "");
}

#[test]
fn a_javascript_url_is_removed_from_raw_html() {
    let document = document("<a href=\"javascript:alert(1)\">x</a>\n");
    let html = html_of(&document.root);
    assert!(!html.contains("javascript"), "{html}");
    assert_eq!(codes(&document), ["E0304"]);
}

/// A Markdown link goes through the same rule as a raw `<a>`.
#[test]
fn a_javascript_url_is_removed_from_a_markdown_link() {
    let document = document("[x](javascript:alert(1))\n");
    assert!(
        inlines(&document.root)
            .into_iter()
            .any(|inline| matches!(inline, Inline::Link { href, .. } if href.is_empty()))
    );
    assert_eq!(codes(&document), ["E0304"]);
}

#[test]
fn an_ordinary_link_and_image_survive() {
    let document = document("[x](/route) ![alt](/a.png)\n");
    assert!(codes(&document).is_empty());
}

#[test]
fn a_data_url_is_rejected_except_on_an_image() {
    let page = document("[x](data:text/html,<b>)\n");
    assert_eq!(codes(&page), ["E0304"]);

    let image = document("![alt](data:image/png;base64,AAAA)\n");
    assert!(codes(&image).is_empty());
}

#[test]
fn the_style_attribute_keeps_only_the_allow_listed_properties() {
    let html = sanitized("<span style=\"color: red; position: fixed\">x</span>\n");
    assert!(html.contains("color: red"), "{html}");
    assert!(!html.contains("position"), "{html}");
}

#[test]
fn a_style_attribute_with_nothing_allowed_is_removed_with_e0304() {
    let document = document("<span style=\"position: fixed\">x</span>\n");
    assert!(!html_of(&document.root).contains("style"));
    assert_eq!(codes(&document), ["E0304"]);
}

/// A value that closes its own attribute would let markup back in.
#[test]
fn an_attribute_value_cannot_break_out_of_its_quotes() {
    let html = sanitized("<span title='a\" onclick=\"x'>y</span>\n");
    assert!(!html.contains("onclick=\"x\""), "{html}");
    assert!(html.contains("&quot;"), "{html}");
}

#[test]
fn an_attribute_only_survives_on_the_element_that_gives_it_meaning() {
    let html = sanitized("<div href=\"/x\">y</div>\n");
    assert!(!html.contains("href"), "{html}");
}

#[test]
fn sanitizing_is_idempotent() {
    for source in [
        "<div onclick=\"x\">y</div>\n",
        "<a href=\"javascript:x\">y</a>\n",
        "<span style=\"color: red; position: fixed\">x</span>\n",
        "<details><summary>s</summary>b</details>\n",
    ] {
        let once = sanitized(source);
        let twice = sanitized(&once);
        assert_eq!(once, twice, "{source}");
    }
}
