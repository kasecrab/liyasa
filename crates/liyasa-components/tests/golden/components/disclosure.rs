//! CMP-10 to CMP-15: accordions, expandables, tabs, steps, trees, and the TOC.

use liyasa_components::{inst, nodes};
use liyasa_core::document::{Block, BlockKind, Node, PropValue};
use liyasa_core::ids::BlockId;

use crate::support::Gallery;

fn str(value: &str) -> PropValue {
    PropValue::Str(value.to_owned())
}

fn list(items: Vec<Node>) -> Node {
    nodes::block(
        BlockKind::List {
            ordered: false,
            start: 1,
            tight: true,
        },
        items,
    )
}

fn item(label: &str, nested: Option<Node>) -> Node {
    let mut children = vec![Node::Inline(liyasa_core::document::Inline::Text(
        label.to_owned(),
    ))];
    children.extend(nested);
    Node::Block(Block {
        id: BlockId::implicit("item", label, "", 0),
        explicit_id: None,
        kind: BlockKind::ListItem { checked: None },
        origin: Default::default(),
        children,
    })
}

#[test]
fn accordion() {
    Gallery::new("accordion")
        .case(
            "default",
            inst::new("accordion")
                .prop("title", str("What is a fact?"))
                .child(nodes::paragraph("A value with a source."))
                .build(),
        )
        .case(
            "every prop",
            inst::new("accordion")
                .prop("title", str("What is a fact?"))
                .prop("icon", str("help"))
                .prop("open", PropValue::Bool(true))
                .prop("id", str("facts"))
                .child(nodes::paragraph("A value with a source."))
                .build(),
        )
        .case("empty body", inst::new("accordion").build())
        .case(
            "grouped",
            inst::new("accordions")
                .prop("one", PropValue::Bool(true))
                .child(inst::nested(
                    inst::new("accordion")
                        .prop("title", str("First"))
                        .child(nodes::paragraph("One.")),
                ))
                .child(inst::nested(
                    inst::new("accordion")
                        .prop("title", str("Second"))
                        .child(nodes::paragraph("Two.")),
                ))
                .build(),
        )
        .check();
}

#[test]
fn expandable() {
    Gallery::new("expandable")
        .case(
            "default",
            inst::new("expandable")
                .prop("title", str("child attributes"))
                .child(nodes::paragraph("Nested fields."))
                .build(),
        )
        .case(
            "every prop",
            inst::new("expandable")
                .prop("title", str("child attributes"))
                .prop("open", PropValue::Bool(true))
                .build(),
        )
        .case("empty body", inst::new("expandable").build())
        .check();
}

#[test]
fn tabs() {
    Gallery::new("tabs")
        .case(
            "default",
            inst::new("tabs")
                .prop("title", str("Install"))
                .child(inst::nested(
                    inst::new("tab")
                        .prop("title", str("npm"))
                        .child(nodes::code_block(Some("sh"), "npm i liyasa\n")),
                ))
                .child(inst::nested(
                    inst::new("tab")
                        .prop("title", str("cargo"))
                        .prop("icon", str("rust"))
                        .child(nodes::code_block(Some("sh"), "cargo install liyasa\n")),
                ))
                .build(),
        )
        .case(
            "synced",
            inst::new("tabs")
                .prop("sync", str("lang"))
                .child(inst::nested(
                    inst::new("tab")
                        .prop("title", str("Rust"))
                        .prop("sync", str("rust"))
                        .child(nodes::paragraph("Rust body.")),
                ))
                .build(),
        )
        .case("empty body", inst::new("tabs").build())
        .case(
            "untitled tab",
            inst::new("tabs")
                .child(inst::nested(
                    inst::new("tab").child(nodes::paragraph("No title.")),
                ))
                .build(),
        )
        .check();
}

#[test]
fn steps() {
    Gallery::new("steps")
        .case(
            "default",
            inst::new("steps")
                .child(inst::nested(
                    inst::new("step")
                        .prop("title", str("Install"))
                        .child(nodes::paragraph("Run the installer.")),
                ))
                .child(inst::nested(
                    inst::new("step")
                        .prop("title", str("Configure"))
                        .child(nodes::paragraph("Write liyasa.json.")),
                ))
                .build(),
        )
        .case(
            "every prop",
            inst::new("steps")
                .prop("style", str("icon"))
                .prop("start", PropValue::Num(3.0))
                .child(inst::nested(
                    inst::new("step")
                        .prop("title", str("Deploy"))
                        .prop("icon", str("rocket"))
                        .prop("number", PropValue::Num(9.0))
                        .child(nodes::paragraph("Ship it.")),
                ))
                .build(),
        )
        .case("empty body", inst::new("steps").build())
        .check();
}

#[test]
fn tree() {
    Gallery::new("tree")
        .case(
            "default",
            inst::new("tree")
                .prop("root", str("liyasa/"))
                .prop("active", str("lib.rs"))
                .prop("expanded", PropValue::Bool(true))
                .child(list(vec![
                    item(
                        "src/",
                        Some(list(vec![item("lib.rs", None), item("main.rs", None)])),
                    ),
                    item("Cargo.toml", None),
                ]))
                .build(),
        )
        .case("empty body", inst::new("tree").build())
        .check();
}

#[test]
fn toc() {
    Gallery::new("toc")
        .case("default", inst::new("toc").build())
        .case(
            "every prop",
            inst::new("toc")
                .prop("depth", PropValue::Num(2.0))
                .prop("from", str("/guides"))
                .build(),
        )
        .case(
            "depth out of range",
            inst::new("toc").prop("depth", PropValue::Num(99.0)).build(),
        )
        .check();
}
