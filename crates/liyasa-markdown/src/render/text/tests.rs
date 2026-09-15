use crate::directives::testing::*;

fn text(source: &str) -> String {
    super::render(&document(source).root)
}

#[test]
fn prose_is_the_text_a_reader_would_read() {
    assert_eq!(text("A *b* **c** d\n"), "A b c d");
}

#[test]
fn blocks_are_separated_by_lines() {
    assert_eq!(text("one\n\ntwo\n"), "one\ntwo");
}

#[test]
fn a_heading_is_its_text() {
    assert_eq!(text("## Install the CLI {#install}\n"), "Install the CLI");
}

#[test]
fn a_component_collapses_to_its_content() {
    assert_eq!(text(":::note\nbody\n:::\n"), "body");
    assert_eq!(text("a :kbd[Ctrl+K] b\n"), "a Ctrl+K b");
}

#[test]
fn a_slot_is_indexed_with_the_component_that_holds_it() {
    assert_eq!(
        text("::::card\nbody\n:::slot{name=\"footer\"}\nfooter\n:::\n::::\n"),
        "body\nfooter"
    );
}

#[test]
fn a_code_block_is_searchable() {
    assert_eq!(text("```rust\nfn main() {}\n```\n"), "fn main() {}");
}

#[test]
fn an_image_contributes_its_alt_text() {
    assert_eq!(text("![Request flow](/a.png)\n"), "Request flow");
}

#[test]
fn a_link_contributes_its_label_and_not_its_target() {
    assert_eq!(text("[the docs](/getting-started)\n"), "the docs");
}

/// CM-55: no markup reaches the index.
#[test]
fn no_markup_survives() {
    for source in [
        "<div class=\"x\">body</div>\n",
        "text <b>bold</b> text\n",
        ":::note{title=\"A\"}\nbody\n:::\n",
        "::image{src=\"/a.png\" alt=\"A\"}\n",
        "| a | b |\n|---|---|\n| 1 | 2 |\n",
    ] {
        let rendered = text(source);
        for forbidden in ['<', '>'] {
            assert!(!rendered.contains(forbidden), "{source} -> {rendered}");
        }
        assert!(!rendered.contains(":::"), "{source} -> {rendered}");
    }
}

/// A component's name is not content, so searching for it must not match.
#[test]
fn a_component_name_is_not_indexed() {
    assert!(!text(":::callout\nbody\n:::\n").contains("callout"));
    assert!(!text("::divider\n").contains("divider"));
}
