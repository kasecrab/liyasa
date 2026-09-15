//! CM-55: a component round-trips as the directive that produced it, and no
//! HTML reaches an agent.

use crate::directives::testing::*;

fn markdown(source: &str) -> String {
    super::render(&document(source).root)
}

/// The strongest property the serialization has: reparsing its output gives
/// the same Markdown again.
fn assert_round_trips(source: &str) {
    let once = markdown(source);
    let twice = markdown(&once);
    assert_eq!(once, twice, "not idempotent for {source:?}");
}

#[test]
fn prose_and_emphasis() {
    assert_eq!(
        markdown("A *b* **c** ~~d~~ `e`\n"),
        "A *b* **c** ~~d~~ `e`\n"
    );
    assert_round_trips("A *b* **c** ~~d~~ `e`\n");
}

#[test]
fn headings_keep_their_level_and_explicit_id() {
    assert_eq!(markdown("## Install\n"), "## Install\n");
    assert_eq!(
        markdown("## Install the CLI {#install}\n"),
        "## Install the CLI {#install}\n"
    );
    assert_round_trips("## Install the CLI {#install}\n");
}

#[test]
fn a_setext_heading_is_written_as_atx() {
    assert_eq!(markdown("Title\n=====\n"), "# Title\n");
}

#[test]
fn lists_keep_their_markers() {
    assert_eq!(markdown("- a\n- b\n"), "- a\n- b\n");
    assert_eq!(markdown("1. a\n2. b\n"), "1. a\n2. b\n");
    assert_eq!(markdown("3. a\n4. b\n"), "3. a\n4. b\n");
    assert_eq!(
        markdown("- [x] done\n- [ ] not\n"),
        "- [x] done\n- [ ] not\n"
    );
    assert_round_trips("- a\n- b\n");
}

#[test]
fn a_block_quote_keeps_its_marker() {
    assert_eq!(markdown("> a\n> b\n"), "> a\n> b\n");
    assert_round_trips("> a\n> b\n");
}

#[test]
fn a_code_fence_keeps_its_body_and_info_string() {
    assert_eq!(
        markdown("```rust\nfn main() {}\n```\n"),
        "```rust\nfn main() {}\n```\n"
    );
    assert_round_trips("```rust\nfn main() {}\n```\n");
}

#[test]
fn a_fence_whose_body_holds_backticks_gets_a_longer_fence() {
    let rendered = markdown("````\n```\n````\n");
    assert!(rendered.starts_with("````"), "{rendered}");
    assert_round_trips("````\n```\n````\n");
}

#[test]
fn links_and_images() {
    assert_eq!(markdown("[a](/b)\n"), "[a](/b)\n");
    assert_eq!(markdown("[a](/b \"t\")\n"), "[a](/b \"t\")\n");
    assert_eq!(markdown("![alt](/a.png)\n"), "![alt](/a.png)\n");
    assert_round_trips("[a](/b \"t\")\n");
}

#[test]
fn a_table_keeps_its_alignment() {
    assert_eq!(
        markdown("| a | b |\n|:--|--:|\n| 1 | 2 |\n"),
        "| a | b |\n| :--- | ---: |\n| 1 | 2 |\n"
    );
    assert_round_trips("| a | b |\n|:--|--:|\n| 1 | 2 |\n");
}

/// CM-55: the directive, not the HTML a theme would render.
#[test]
fn a_container_component_round_trips_as_its_directive() {
    assert_eq!(markdown(":::note\nbody\n:::\n"), ":::note\nbody\n:::\n");
    assert_round_trips(":::note\nbody\n:::\n");
}

#[test]
fn a_component_keeps_its_props() {
    assert_eq!(
        markdown(":::card{title=\"A\" columns=2}\nbody\n:::\n"),
        ":::card{columns=2 title=\"A\"}\nbody\n:::\n"
    );
    assert_round_trips(":::card{title=\"A\" columns=2}\nbody\n:::\n");
}

#[test]
fn the_shorthand_props_are_written_as_shorthand() {
    assert_eq!(
        markdown(":::card{.wide #x}\nbody\n:::\n"),
        ":::card{.wide #x}\nbody\n:::\n"
    );
    assert_round_trips(":::card{.wide #x}\nbody\n:::\n");
}

#[test]
fn a_leaf_component_round_trips() {
    assert_eq!(
        markdown("::image{src=\"/a.png\"}\n"),
        "::image{src=\"/a.png\"}\n"
    );
    assert_round_trips("::image{src=\"/a.png\"}\n");
}

#[test]
fn an_inline_component_round_trips() {
    assert_eq!(markdown("a :kbd[Ctrl+K] b\n"), "a :kbd[Ctrl+K] b\n");
    assert_round_trips("a :kbd[Ctrl+K] b\n");
}

/// A nested container needs a longer fence than the one inside it.
#[test]
fn nesting_chooses_a_longer_fence() {
    let rendered = markdown("::::note\n:::tip\nbody\n:::\n::::\n");
    assert_eq!(rendered, "::::note\n:::tip\nbody\n:::\n::::\n");
    assert_round_trips("::::note\n:::tip\nbody\n:::\n::::\n");
}

#[test]
fn a_slot_is_written_back_inside_its_component() {
    let rendered = markdown("::::card\nbody\n:::slot{name=\"footer\"}\nfooter\n:::\n::::\n");
    assert!(rendered.contains(":::slot{name=\"footer\"}"), "{rendered}");
    assert!(rendered.contains("footer"), "{rendered}");
    assert_round_trips("::::card\nbody\n:::slot{name=\"footer\"}\nfooter\n:::\n::::\n");
}

/// CM-53: the tag form is written back as the directive it means.
#[test]
fn the_tag_form_is_written_back_as_a_directive() {
    assert_eq!(
        markdown("<Card title=\"x\">\n\nbody\n\n</Card>\n"),
        ":::card{title=\"x\"}\nbody\n:::\n"
    );
}

/// CM-34: a GitHub alert is written back as the callout it maps to.
#[test]
fn a_github_alert_is_written_back_as_a_directive() {
    assert_eq!(markdown("> [!NOTE]\n> body\n"), ":::note\nbody\n:::\n");
}

/// CM-55: no HTML reaches an agent.
#[test]
fn no_html_survives() {
    for source in [
        "<div class=\"x\">\n\nbody\n\n</div>\n",
        "text <b>bold</b> text\n",
        ":::note\n<span>x</span>\n:::\n",
    ] {
        let rendered = markdown(source);
        assert!(!rendered.contains('<'), "{source} -> {rendered}");
    }
}

#[test]
fn an_empty_page_serializes_to_nothing() {
    assert_eq!(markdown(""), "");
}

#[test]
fn every_shape_round_trips() {
    for source in [
        "# A\n\npara\n\n- x\n- y\n",
        ":::note\n# Heading\n\n- item\n:::\n",
        "> quote\n\npara\n",
        "a\n\n---\n\nb\n",
        "$$\nx = 1\n$$\n",
        "term\n\n: details\n",
    ] {
        assert_round_trips(source);
    }
}

/// CM-33: math survives the round trip an agent reads and the formatter writes.
#[test]
fn math_round_trips() {
    assert_eq!(markdown("$x^2$\n"), "$x^2$\n");
    assert_eq!(markdown("$$\ny = mx + b\n$$\n"), "$$\ny = mx + b\n$$\n");
    assert_round_trips("Inline $x^2$ and display:\n\n$$\ny = mx + b\n$$\n");
}
