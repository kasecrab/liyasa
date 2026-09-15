//! RX-61: the component serialization rules, checked structurally.
//!
//! The golden files under `expected/` pin the exact bytes. This file pins the
//! *rules*, so a change that keeps the output plausible but breaks the contract
//! — a tab that loses its group prefix, an image link that stops being absolute
//! — fails here with the rule it broke rather than as a diff.

use liyasa_components::render::Shared;
use liyasa_components::{MarkdownCtx, Reference, Registry, inst, nodes};
use liyasa_core::components::ComponentInst;
use liyasa_core::document::{Align, Block, BlockKind, Inline, Node, PropValue};
use liyasa_core::ids::{BlockId, Locale};
use liyasa_core::markdown::SiteMeta;

fn str(value: &str) -> PropValue {
    PropValue::Str(value.to_owned())
}

fn site() -> SiteMeta {
    SiteMeta {
        name: "Liyasa".into(),
        canonical_origin: "https://docs.example.com/".parse().expect("a URL"),
        llms_txt: "https://docs.example.com/llms.txt".parse().expect("a URL"),
        version: None,
        locale: Locale::new("en"),
    }
}

/// One component's Markdown serialization, with the site origin available so
/// absolute links can be produced.
fn serialize(inst: &ComponentInst) -> String {
    let registry = Registry::builtins();
    let reference = Reference::with(&registry);
    let site = site();
    let component = registry
        .resolve(&inst.name)
        .unwrap_or_else(|| panic!("`{}` is not registered", inst.name));
    let mut ctx = MarkdownCtx::with(Shared::new(&reference).site(&site));
    component.markdown(inst, &mut ctx).expect("serializes");
    ctx.finish()
}

/// A whole node tree's serialization, for the rules about runs of siblings.
fn serialize_nodes(nodes: &[Node]) -> String {
    use liyasa_components::Children;

    let registry = Registry::builtins();
    let reference = Reference::with(&registry);
    let site = site();
    let mut ctx = MarkdownCtx::with(Shared::new(&reference).site(&site));
    reference.markdown(nodes, &mut ctx).expect("serializes");
    ctx.finish()
}

#[test]
fn callouts_become_blockquotes_with_a_bold_label() {
    for name in ["note", "tip", "warning", "info", "check", "danger"] {
        let markdown = serialize(&inst::new(name).child(nodes::paragraph("Body.")).build());
        let mut lines = markdown.lines();
        let label = lines.next().unwrap_or_default();
        assert!(
            label.starts_with("> **") && label.ends_with("**"),
            "`{name}` must open with a bold label in a quote, got {label:?}"
        );
        assert!(
            markdown.lines().all(|line| line.starts_with('>')),
            "every line of `{name}` must be quoted:\n{markdown}"
        );
    }
}

#[test]
fn tabs_become_h3_sections_prefixed_by_the_group() {
    let markdown = serialize(
        &inst::new("tabs")
            .prop("title", str("Install"))
            .child(inst::nested(
                inst::new("tab")
                    .prop("title", str("npm"))
                    .child(nodes::paragraph("Run npm.")),
            ))
            .child(inst::nested(
                inst::new("tab")
                    .prop("title", str("cargo"))
                    .child(nodes::paragraph("Run cargo.")),
            ))
            .build(),
    );
    assert!(markdown.contains("### Install: npm"), "{markdown}");
    assert!(markdown.contains("### Install: cargo"), "{markdown}");
    assert!(markdown.contains("Run npm."), "{markdown}");
}

#[test]
fn accordions_become_h3_sections() {
    let markdown = serialize(
        &inst::new("accordion")
            .prop("title", str("What is a fact?"))
            .child(nodes::paragraph("A value with a source."))
            .build(),
    );
    assert!(markdown.starts_with("### What is a fact?"), "{markdown}");
    assert!(markdown.contains("A value with a source."), "{markdown}");
}

#[test]
fn steps_become_an_ordered_list_with_headings() {
    let markdown = serialize(
        &inst::new("steps")
            .child(inst::nested(
                inst::new("step")
                    .prop("title", str("Install"))
                    .child(nodes::paragraph("Run the installer.")),
            ))
            .child(inst::nested(
                inst::new("step")
                    .prop("title", str("Configure"))
                    .child(nodes::paragraph("Write the config.")),
            ))
            .build(),
    );
    let lines: Vec<&str> = markdown.lines().collect();
    assert_eq!(lines.first().copied(), Some("1. **Install**"), "{markdown}");
    assert!(lines.contains(&"2. **Configure**"), "{markdown}");
}

#[test]
fn cards_become_a_list_of_links_with_descriptions() {
    let markdown = serialize(
        &inst::new("cards")
            .child(inst::nested(
                inst::new("card")
                    .prop("title", str("Quickstart"))
                    .prop("href", str("/start"))
                    .child(nodes::paragraph("Five minutes.")),
            ))
            .child(inst::nested(
                inst::new("card")
                    .prop("title", str("Reference"))
                    .prop("href", str("/reference"))
                    .child(nodes::paragraph("Every option.")),
            ))
            .build(),
    );
    assert_eq!(
        markdown,
        "- [Quickstart](https://docs.example.com/start)\n  Five minutes.\n\
         - [Reference](https://docs.example.com/reference)\n  Every option.\n",
        "{markdown}"
    );
}

#[test]
fn code_groups_become_consecutive_fences_carrying_their_titles() {
    let fence = |info: &str, body: &str| {
        let (parsed, _) = liyasa_components::fence::parse_info(info);
        nodes::code_block_with(parsed.lang.as_deref(), body, parsed.attrs)
    };
    let markdown = serialize(
        &inst::new("code-group")
            .child(fence(r#"sh title="npm""#, "npm i liyasa\n"))
            .child(fence(r#"sh title="cargo""#, "cargo install liyasa\n"))
            .build(),
    );
    let fences: Vec<&str> = markdown
        .lines()
        .filter(|line| line.starts_with("```"))
        .collect();
    assert_eq!(
        fences.len(),
        4,
        "two fences, opened and closed:\n{markdown}"
    );
    assert_eq!(fences[0], r#"```sh title="npm""#, "{markdown}");
    assert_eq!(fences[2], r#"```sh title="cargo""#, "{markdown}");
}

#[test]
fn images_become_markdown_images_with_absolute_urls() {
    let markdown = serialize(
        &inst::new("image")
            .prop("src", str("/img/dashboard.png"))
            .prop("alt", str("The dashboard"))
            .build(),
    );
    assert_eq!(
        markdown, "![The dashboard](https://docs.example.com/img/dashboard.png)\n",
        "{markdown}"
    );
}

#[test]
fn tables_stay_tables() {
    let cell = |text: &str| {
        Node::Block(Block {
            id: BlockId::implicit("cell", text, "", 0),
            explicit_id: None,
            kind: BlockKind::TableCell,
            origin: Default::default(),
            children: vec![Node::Inline(Inline::Text(text.to_owned()))],
        })
    };
    let row = |header: bool, cells: Vec<Node>| {
        Node::Block(Block {
            id: BlockId::implicit("row", "", "", 0),
            explicit_id: None,
            kind: BlockKind::TableRow { header },
            origin: Default::default(),
            children: cells,
        })
    };
    let table = nodes::block(
        BlockKind::Table {
            align: vec![Align::Left, Align::Right],
        },
        vec![
            row(true, vec![cell("Name"), cell("Count")]),
            row(false, vec![cell("pages"), cell("42")]),
        ],
    );
    let markdown = serialize_nodes(&[table]);
    assert_eq!(
        markdown, "| Name  | Count |\n| ----- | ----- |\n| pages | 42    |\n",
        "{markdown}"
    );
}

#[test]
fn mermaid_stays_a_fence() {
    let markdown = serialize_nodes(&[nodes::code_block(
        Some("mermaid"),
        "flowchart LR\n  a --> b\n",
    )]);
    assert_eq!(
        markdown, "```mermaid\nflowchart LR\n  a --> b\n```\n",
        "{markdown}"
    );
}

#[test]
fn api_parameter_fields_become_a_table() {
    let markdown = serialize_nodes(&[
        inst::nested(
            inst::new("param")
                .prop("name", str("limit"))
                .prop("type", str("integer"))
                .prop("required", PropValue::Bool(true))
                .child(nodes::paragraph("How many items.")),
        ),
        inst::nested(
            inst::new("response-field")
                .prop("name", str("items"))
                .prop("type", str("object[]"))
                .child(nodes::paragraph("The page.")),
        ),
    ]);
    let lines: Vec<&str> = markdown.lines().collect();
    assert!(
        lines.len() == 4 && lines.iter().all(|line| line.starts_with('|')),
        "one table, header plus rule plus two rows:\n{markdown}"
    );
    assert!(
        lines[0].contains("Name") && lines[0].contains("Description"),
        "{markdown}"
    );
    assert!(
        lines[2].contains("`limit`") && lines[2].contains("required"),
        "{markdown}"
    );
    assert!(lines[3].contains("`items`"), "{markdown}");
}
