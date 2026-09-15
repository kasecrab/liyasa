//! End to end: expanded text in, Rendered AST out.
//!
//! These mirror `spec/markdown/cm-50/`, `cm-52/`, and `cm-53/`, which assert the
//! same structures against an engine the harness does not have yet.

use liyasa_core::document::{BlockKind, Inline, PropValue};
use liyasa_core::markdown::HtmlMode;

use crate::directives::testing::*;

fn names(source: &str) -> Vec<String> {
    let document = document(source);
    components(&document.root)
        .into_iter()
        .filter_map(|b| match &b.kind {
            BlockKind::Component { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_container_directive_becomes_a_component() {
    let document = document(":::note\nbody\n:::\n");
    let note = component_named(&document.root, "note");
    assert_eq!(note.children.len(), 1);
    assert_eq!(codes(&document), Vec::<&str>::new());
}

#[test]
fn containers_nest_with_longer_fences() {
    assert_eq!(
        names("::::note\n:::tip\nbody\n:::\n::::\n"),
        ["note", "tip"]
    );
}

#[test]
fn a_directive_carries_its_props() {
    let document = document(":::card{title=\"Install\" columns=2}\nbody\n:::\n");
    let BlockKind::Component { props, .. } = &component_named(&document.root, "card").kind else {
        panic!("expected a component");
    };
    assert_eq!(
        props.get("title"),
        Some(&PropValue::Str("Install".to_owned()))
    );
    assert_eq!(props.get("columns"), Some(&PropValue::Num(2.0)));
}

#[test]
fn a_directive_inside_a_list_item_is_still_a_directive() {
    assert_eq!(names("- item\n  :::note\n  body\n  :::\n"), ["note"]);
}

#[test]
fn a_directive_inside_a_blockquote_is_still_a_directive() {
    assert_eq!(names("> :::note\n> body\n> :::\n"), ["note"]);
}

#[test]
fn a_leaf_directive_becomes_a_childless_component() {
    let document = document("::image{src=\"/a.png\"}\n");
    let image = component_named(&document.root, "image");
    assert!(image.children.is_empty());
    assert_eq!(codes(&document), Vec::<&str>::new());
}

#[test]
fn an_inline_directive_becomes_an_inline_component() {
    let document = document("Press :kbd[Ctrl+K] now.\n");
    assert!(inlines(&document.root).into_iter().any(|inline| matches!(
        inline,
        Inline::InlineComponent { name, .. } if name == "kbd"
    )));
    assert_eq!(codes(&document), Vec::<&str>::new());
}

#[test]
fn a_directive_inside_a_code_fence_is_literal() {
    assert_eq!(
        names("```\n:::note\nbody\n:::\n```\n"),
        Vec::<String>::new()
    );
}

/// CM-34: `> [!NOTE]` and `:::note` reach the theme as the same component.
#[test]
fn a_github_alert_is_the_callout_the_directive_would_have_made() {
    assert_eq!(names("> [!NOTE]\n> body\n"), ["note"]);
    assert_eq!(names("> [!WARNING]\n> body\n"), ["warning"]);
    assert_eq!(names("> [!TIP]\n> body\n"), ["tip"]);
    assert_eq!(names("> [!IMPORTANT]\n> body\n"), ["important"]);
    assert_eq!(names("> [!CAUTION]\n> body\n"), ["caution"]);
}

/// CM-53: the first is a component, the second is raw HTML.
#[test]
fn the_tag_form_is_a_component_and_a_lowercase_tag_is_not() {
    let document = document("<Card title=\"x\">\n\nbody\n\n</Card>\n");
    let BlockKind::Component { name, props, .. } = &component_named(&document.root, "card").kind
    else {
        panic!("expected a component");
    };
    assert_eq!(name, "card");
    assert_eq!(props.get("title"), Some(&PropValue::Str("x".to_owned())));

    assert!(names("<div class=\"x\">\n\nbody\n\n</div>\n").is_empty());
}

#[test]
fn tag_form_components_nest() {
    assert_eq!(
        names("<Tabs>\n\n<Tab title=\"npm\">\n\nnpm i\n\n</Tab>\n\n</Tabs>\n"),
        ["tabs", "tab"]
    );
}

/// An unmatched open tag was raw HTML all along, and the sanitizer then makes
/// the same decision about it that it makes about any unknown element.
#[test]
fn an_unclosed_tag_stays_raw_html() {
    let kept = document_with("<Card title=\"x\">\n\nbody\n", HtmlMode::Allow);
    assert!(components(&kept.root).is_empty());
    assert!(html_of(&kept.root).contains("<Card"));

    let sanitized = document("<Card title=\"x\">\n\nbody\n");
    assert_eq!(codes(&sanitized), ["E0304"]);
}

#[test]
fn a_tag_inside_a_fence_is_literal() {
    assert!(names("```\n<Card title=\"x\">\n```\n").is_empty());
}

/// CM-52: the body is Markdown and a named slot appears in its place.
#[test]
fn a_named_slot_is_lifted_out_of_the_body() {
    let document = document("::::card\nbody\n:::slot{name=\"footer\"}\nfooter\n:::\n::::\n");
    let BlockKind::Component { slots, .. } = &component_named(&document.root, "card").kind else {
        panic!("expected a component");
    };
    assert_eq!(slots.0.keys().collect::<Vec<_>>(), ["footer"]);
    assert_eq!(codes(&document), Vec::<&str>::new());
}

#[test]
fn a_component_body_is_parsed_as_markdown() {
    let document = document(":::note\n# Heading\n\n- item\n:::\n");
    let kinds: Vec<_> = blocks(&document.root)
        .into_iter()
        .map(|b| crate::ast::identity::kind_name(&b.kind))
        .collect();
    assert!(kinds.contains(&"heading"));
    assert!(kinds.contains(&"list"));
}

/// CM-31, through the whole parser rather than the anchor table alone.
#[test]
fn heading_anchors_are_slugged_deduplicated_and_overridable() {
    let document = document("## Setup\n\n## Setup\n\n## Anything {#custom}\n");
    let anchors: Vec<_> = blocks(&document.root)
        .into_iter()
        .filter_map(|b| match &b.kind {
            BlockKind::Heading { anchor, .. } => Some(anchor.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(anchors, ["setup", "setup-1", "custom"]);
}

#[test]
fn an_explicit_id_is_taken_off_the_heading_text() {
    let document = document("## Install the CLI {#install}\n");
    let heading = blocks(&document.root)
        .into_iter()
        .find(|b| matches!(b.kind, BlockKind::Heading { .. }))
        .expect("a heading");
    assert_eq!(heading.explicit_id.as_deref(), Some("install"));
    assert_eq!(
        inlines(heading),
        [&Inline::Text("Install the CLI".to_owned())]
    );
}

/// A class is not an identity, so it stays where the author wrote it.
#[test]
fn a_class_attribute_is_not_an_id() {
    let document = document("## A {.wide}\n");
    let heading = blocks(&document.root)
        .into_iter()
        .find(|b| matches!(b.kind, BlockKind::Heading { .. }))
        .expect("a heading");
    assert_eq!(heading.explicit_id, None);
    assert_eq!(inlines(heading), [&Inline::Text("A {.wide}".to_owned())]);
}

#[test]
fn a_duplicated_explicit_id_is_e0318() {
    assert_eq!(codes(&document("## A {#x}\n\n## B {#x}\n")), ["E0318"]);
}

/// §7.16: identical siblings differ by ordinal, and the same text under a
/// different heading is a different block.
#[test]
fn block_identity_separates_what_a_reader_would_separate() {
    let siblings = document("para\n\npara\n\npara\n");
    let ids: Vec<_> = blocks(&siblings.root)
        .into_iter()
        .filter(|b| matches!(b.kind, BlockKind::Paragraph))
        .map(|b| b.id)
        .collect();
    assert_eq!(ids.len(), 3);
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 3);

    let under_headings = document("# A\n\npara\n\n# B\n\npara\n");
    let ids: Vec<_> = blocks(&under_headings.root)
        .into_iter()
        .filter(|b| matches!(b.kind, BlockKind::Paragraph))
        .map(|b| b.id)
        .collect();
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn an_explicit_id_survives_an_edit_elsewhere_on_the_page() {
    let before = document("## A {#x}\n\npara\n");
    let after = document("intro\n\n## A {#x}\n\npara\n");
    let id = |document: &liyasa_core::Document| {
        blocks(&document.root)
            .into_iter()
            .find(|b| b.explicit_id.as_deref() == Some("x"))
            .map(|b| b.id)
    };
    assert_eq!(id(&before), id(&after));
}

#[test]
fn a_table_keeps_its_alignment() {
    let document = document("| a | b |\n|:--|--:|\n| 1 | 2 |\n");
    let BlockKind::Table { align } = &blocks(&document.root)
        .into_iter()
        .find(|b| matches!(b.kind, BlockKind::Table { .. }))
        .expect("a table")
        .kind
    else {
        panic!("expected a table");
    };
    use liyasa_core::document::Align;
    assert_eq!(align, &[Align::Left, Align::Right]);
}

#[test]
fn a_code_fence_keeps_its_language_and_attributes() {
    let document = document("```rust title=\"main.rs\" {1,3-5}\nfn main() {}\n```\n");
    let BlockKind::CodeBlock { lang, attrs, .. } = &blocks(&document.root)
        .into_iter()
        .find(|b| matches!(b.kind, BlockKind::CodeBlock { .. }))
        .expect("a code block")
        .kind
    else {
        panic!("expected a code block");
    };
    assert_eq!(lang.as_deref(), Some("rust"));
    assert_eq!(attrs.highlight, [(1, 1), (3, 5)]);
    assert_eq!(attrs.kv.get("title").map(String::as_str), Some("main.rs"));
}

#[test]
fn an_unknown_fence_attribute_is_w0302() {
    assert_eq!(codes(&document("```rust sparkle\nx\n```\n")), ["W0302"]);
}

#[test]
fn a_task_list_records_what_is_checked() {
    let document = document("- [x] done\n- [ ] not\n");
    let checked: Vec<_> = blocks(&document.root)
        .into_iter()
        .filter_map(|b| match b.kind {
            BlockKind::ListItem { checked } => Some(checked),
            _ => None,
        })
        .collect();
    assert_eq!(checked, [Some(true), Some(false)]);
}

#[test]
fn front_matter_is_not_part_of_this_tree() {
    let document = document("---\ntitle: A\n---\n\nbody\n");
    assert!(
        blocks(&document.root)
            .into_iter()
            .all(|b| !matches!(b.kind, BlockKind::HtmlBlock { .. }))
    );
    assert_eq!(codes(&document), Vec::<&str>::new());
}

#[test]
fn a_page_with_nothing_in_it_still_parses() {
    let document = document("");
    assert!(matches!(document.root.kind, BlockKind::Document));
    assert!(document.root.children.is_empty());
}

/// Whatever the input, the parser returns a document rather than panicking.
#[test]
fn malformed_pages_never_panic() {
    for source in [
        ":::",
        "::::",
        ":::\n:::",
        ":::a\n::::b\n:::\n",
        "::",
        ":[",
        ":a[",
        "<Card>",
        "</Card>",
        "<!--ly:0:l:0-->",
        "```\n",
        "| a |\n|--|\n",
        "- [ ]",
        "> [!NOPE]\n> x\n",
        "\u{0}",
        "🙂:::note\n:::\n",
    ] {
        let _ = document(source);
    }
}
