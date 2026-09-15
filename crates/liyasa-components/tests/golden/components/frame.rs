//! CMP-05, CMP-06, CMP-08, CMP-09: frame, panel, hero, and divider.

use liyasa_components::{inst, nodes};
use liyasa_core::document::{Inline, PropValue};

use crate::support::Gallery;

fn str(value: &str) -> PropValue {
    PropValue::Str(value.to_owned())
}

fn image(src: &str, alt: &str) -> liyasa_core::document::Node {
    nodes::paragraph_of(vec![Inline::Image {
        src: src.to_owned(),
        alt: alt.to_owned(),
        title: None,
        dark: None,
    }])
}

#[test]
fn frame() {
    Gallery::new("frame")
        .case(
            "default",
            inst::new("frame")
                .child(image("/img/ui.png", "The dashboard"))
                .build(),
        )
        .case(
            "every prop",
            inst::new("frame")
                .prop("caption", str("The dashboard"))
                .prop("hint", str("Click to zoom"))
                .prop("video", PropValue::Bool(true))
                .prop("align", str("full"))
                .child(image("/img/ui.png", "The dashboard"))
                .build(),
        )
        .case("empty body", inst::new("frame").build())
        .case(
            "invalid align",
            inst::new("frame").prop("align", str("middle")).build(),
        )
        .check();
}

#[test]
fn panel() {
    Gallery::new("panel")
        .case(
            "default",
            inst::new("panel")
                .child(nodes::paragraph("On this page"))
                .build(),
        )
        .case(
            "invalid prop",
            inst::new("panel").prop("title", str("nope")).build(),
        )
        .check();
}

#[test]
fn hero() {
    Gallery::new("hero")
        .case(
            "default",
            inst::new("hero").prop("title", str("Liyasa")).build(),
        )
        .case(
            "every prop",
            inst::new("hero")
                .prop("title", str("Liyasa"))
                .prop("subtitle", str("Documentation that verifies itself."))
                .prop("image", str("/img/hero.svg"))
                .prop(
                    "actions",
                    PropValue::List(vec![
                        str("Get started -> /start"),
                        str("GitHub -> https://github.com/kasecrab/liyasa"),
                        str("Nothing here"),
                    ]),
                )
                .child(nodes::paragraph("Extra body copy."))
                .build(),
        )
        .case("empty body", inst::new("hero").build())
        .check();
}

#[test]
fn divider() {
    Gallery::new("divider")
        .case("default", inst::new("divider").build())
        .case(
            "every prop",
            inst::new("divider").prop("label", str("Since 0.4")).build(),
        )
        .case(
            "invalid prop",
            inst::new("divider").prop("labels", str("nope")).build(),
        )
        .check();
}
