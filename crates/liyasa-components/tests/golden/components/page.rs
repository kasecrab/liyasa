//! CMP-71, CMP-76 to CMP-80, CMP-82 to CMP-84: the page-level components.

use liyasa_components::{inst, nodes};
use liyasa_core::document::PropValue;

use crate::support::Gallery;

fn str(value: &str) -> PropValue {
    PropValue::Str(value.to_owned())
}

#[test]
fn banner() {
    Gallery::new("banner")
        .case(
            "default",
            inst::new("banner")
                .child(nodes::paragraph("Version 0.5 is out."))
                .build(),
        )
        .case(
            "every prop",
            inst::new("banner")
                .prop("color", str("#3355ff"))
                .prop("dismissible", PropValue::Bool(true))
                .prop("id", str("v05"))
                .child(nodes::paragraph("Version 0.5 is out."))
                .build(),
        )
        .check();
}

#[test]
fn update() {
    Gallery::new("update")
        .case(
            "default",
            inst::new("update")
                .prop("date", str("2026-09-15"))
                .child(nodes::paragraph("Fixed the anchor generator."))
                .build(),
        )
        .case(
            "every prop",
            inst::new("update")
                .prop("date", str("2026-09-15"))
                .prop("version", str("0.5.0"))
                .prop("labels", PropValue::List(vec![str("breaking"), str("api")]))
                .prop("title", str("Anchors are stable"))
                .child(nodes::paragraph("Fixed the anchor generator."))
                .build(),
        )
        .case("missing required prop", inst::new("update").build())
        .check();
}

#[test]
fn prompt() {
    Gallery::new("prompt")
        .case(
            "default",
            inst::new("prompt")
                .child(nodes::paragraph("Summarize this page for a new reader."))
                .build(),
        )
        .case(
            "every prop",
            inst::new("prompt")
                .prop("title", str("Ask an assistant"))
                .prop(
                    "open",
                    PropValue::List(vec![str("cursor"), str("claude"), str("chatgpt")]),
                )
                .child(nodes::paragraph("Summarize this page for a new reader."))
                .build(),
        )
        .case(
            "invalid target",
            inst::new("prompt")
                .prop("open", PropValue::List(vec![str("copilot")]))
                .build(),
        )
        .check();
}

#[test]
fn github() {
    Gallery::new("github")
        .case(
            "default",
            inst::new("github")
                .prop("repo", str("kasecrab/liyasa"))
                .build(),
        )
        .case("missing required prop", inst::new("github").build())
        .check();
}

#[test]
fn md() {
    Gallery::new("md")
        .case(
            "default",
            inst::new("md")
                .child(nodes::paragraph("Parsed as Markdown."))
                .build(),
        )
        .check();
}

#[test]
fn visibility() {
    Gallery::new("visibility")
        .case(
            "agents only",
            inst::new("visibility")
                .prop("agents", PropValue::Bool(true))
                .child(nodes::paragraph("For agents."))
                .build(),
        )
        .case(
            "humans only",
            inst::new("visibility")
                .prop("humans", PropValue::Bool(true))
                .child(nodes::paragraph("For humans."))
                .build(),
        )
        .case(
            "every prop",
            inst::new("visibility")
                .prop("humans", PropValue::Bool(true))
                .prop("agents", PropValue::Bool(true))
                .prop("groups", PropValue::List(vec![str("staff")]))
                .prop("regions", PropValue::List(vec![str("eu")]))
                .prop("locales", PropValue::List(vec![str("de")]))
                .prop("versions", PropValue::List(vec![str("0.5")]))
                .child(nodes::paragraph("Gated."))
                .build(),
        )
        .case(
            "no audience named",
            inst::new("visibility")
                .prop("groups", PropValue::List(vec![str("staff")]))
                .child(nodes::paragraph("Both audiences."))
                .build(),
        )
        .check();
}

#[test]
fn region() {
    Gallery::new("region")
        .case(
            "every prop",
            inst::new("region")
                .prop("only", PropValue::List(vec![str("eu"), str("uk")]))
                .prop("except", PropValue::List(vec![str("us")]))
                .child(nodes::paragraph("EU terms."))
                .build(),
        )
        .check();
}

#[test]
fn feedback_and_assistant() {
    Gallery::new("feedback")
        .case("default", inst::new("feedback").build())
        .case(
            "every prop",
            inst::new("feedback")
                .prop("question", str("Did this answer your question?"))
                .build(),
        )
        .case(
            "assistant",
            inst::new("assistant")
                .prop("prompt", str("How do I verify a code sample?"))
                .prop("label", str("Ask about verification"))
                .build(),
        )
        .case("assistant with no prompt", inst::new("assistant").build())
        .check();
}
