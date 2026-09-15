//! CMP-70 and CMP-72 to CMP-75, CMP-81: the inline components.

use liyasa_components::inst;
use liyasa_core::document::{Inline, Node, PropValue};

use crate::support::Gallery;

fn str(value: &str) -> PropValue {
    PropValue::Str(value.to_owned())
}

fn body(text: &str) -> Node {
    Node::Inline(Inline::Text(text.to_owned()))
}

#[test]
fn badge() {
    Gallery::new("badge")
        .case("default", inst::new("badge").child(body("Beta")).build())
        .case(
            "every prop",
            inst::new("badge")
                .prop("color", str("#3355ff"))
                .prop("variant", str("solid"))
                .prop("icon", str("flask"))
                .child(body("Beta"))
                .build(),
        )
        .case("empty body", inst::new("badge").build())
        .case(
            "invalid variant",
            inst::new("badge")
                .prop("variant", str("glow"))
                .child(body("Beta"))
                .build(),
        )
        .check();
}

#[test]
fn color() {
    Gallery::new("color")
        .case(
            "default",
            inst::new("color").prop("value", str("#ff8800")).build(),
        )
        .case(
            "every prop",
            inst::new("color")
                .prop("value", str("rgb(255, 136, 0)"))
                .prop("name", str("Ember"))
                .build(),
        )
        .case(
            "unsafe value",
            inst::new("color")
                .prop("value", str("red; background: url(//evil.example.com)"))
                .build(),
        )
        .case("missing required prop", inst::new("color").build())
        .check();
}

#[test]
fn icon() {
    Gallery::new("icon")
        .case(
            "default",
            inst::new("icon").prop("name", str("rocket")).build(),
        )
        .case(
            "every prop",
            inst::new("icon")
                .prop("name", str("rocket"))
                .prop("type", str("lucide"))
                .prop("size", PropValue::Num(24.0))
                .prop("color", str("#3355ff"))
                .prop("label", str("Launch"))
                .build(),
        )
        .case("missing required prop", inst::new("icon").build())
        .check();
}

#[test]
fn tooltip() {
    Gallery::new("tooltip")
        .case(
            "default",
            inst::new("tooltip")
                .prop("text", str("A value with a source behind it."))
                .child(body("fact"))
                .build(),
        )
        .case(
            "every prop",
            inst::new("tooltip")
                .prop("text", str("A value with a source behind it."))
                .prop("href", str("/concepts/facts"))
                .child(body("fact"))
                .build(),
        )
        .case("missing required prop", inst::new("tooltip").build())
        .check();
}

#[test]
fn kbd() {
    Gallery::new("kbd")
        .case("default", inst::new("kbd").child(body("Ctrl")).build())
        .case("empty body", inst::new("kbd").build())
        .case(
            "invalid prop",
            inst::new("kbd")
                .prop("key", str("Ctrl"))
                .child(body("Ctrl"))
                .build(),
        )
        .check();
}

#[test]
fn fact() {
    Gallery::new("fact")
        .case(
            "default",
            inst::new("fact").prop("id", str("pricing.pro")).build(),
        )
        .case(
            "every prop",
            inst::new("fact")
                .prop("id", str("pricing.pro"))
                .prop("format", str("currency"))
                .child(body("$20"))
                .build(),
        )
        .case("missing required prop", inst::new("fact").build())
        .check();
}
