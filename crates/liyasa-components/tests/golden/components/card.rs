//! CMP-01 to CMP-04: cards, card grids, columns, and tiles.

use liyasa_components::{inst, nodes};
use liyasa_core::document::PropValue;

use crate::support::Gallery;

fn str(value: &str) -> PropValue {
    PropValue::Str(value.to_owned())
}

#[test]
fn card() {
    Gallery::new("card")
        .case(
            "default",
            inst::new("card")
                .prop("title", str("Quickstart"))
                .child(nodes::paragraph("Get running in five minutes."))
                .build(),
        )
        .case(
            "every prop",
            inst::new("card")
                .prop("title", str("Quickstart"))
                .prop("icon", str("rocket"))
                .prop("href", str("/start"))
                .prop("img", str("/img/start.png"))
                .prop("horizontal", PropValue::Bool(true))
                .prop("cta", str("Read the guide"))
                .prop("color", str("#3355ff"))
                .prop("arrow", PropValue::Bool(true))
                .child(nodes::paragraph("Get running in five minutes."))
                .build(),
        )
        .case(
            "empty body",
            inst::new("card").prop("title", str("Bare")).build(),
        )
        .case(
            "no title",
            inst::new("card")
                .child(nodes::paragraph("Just a body."))
                .build(),
        )
        .case(
            "unsafe href",
            inst::new("card")
                .prop("title", str("Nope"))
                .prop("href", str("javascript:alert(1)"))
                .build(),
        )
        .case(
            "invalid prop",
            inst::new("card")
                .prop("horizontal", str("maybe"))
                .prop("titel", str("typo"))
                .build(),
        )
        .case(
            "nesting",
            inst::new("card")
                .prop("title", str("Quickstart"))
                .child(nodes::paragraph("Pick a language."))
                .child(inst::nested(
                    inst::new("note").child(nodes::paragraph("Rust only for now.")),
                ))
                .build(),
        )
        .check();
}

#[test]
fn cards() {
    Gallery::new("cards")
        .case(
            "default",
            inst::new("cards")
                .child(inst::nested(
                    inst::new("card")
                        .prop("title", str("One"))
                        .prop("href", str("/one"))
                        .child(nodes::paragraph("First.")),
                ))
                .child(inst::nested(
                    inst::new("card")
                        .prop("title", str("Two"))
                        .prop("href", str("/two"))
                        .child(nodes::paragraph("Second.")),
                ))
                .build(),
        )
        .case(
            "every prop",
            inst::new("cards")
                .prop("cols", PropValue::Num(4.0))
                .prop("gap", str("1.5rem"))
                .child(inst::nested(inst::new("card").prop("title", str("One"))))
                .build(),
        )
        .case("empty body", inst::new("cards").build())
        .case(
            "wrong child",
            inst::new("cards")
                .child(inst::nested(
                    inst::new("note").child(nodes::paragraph("Nope.")),
                ))
                .build(),
        )
        .case(
            "cols out of range",
            inst::new("cards").prop("cols", PropValue::Num(9.0)).build(),
        )
        .case(
            "nesting",
            inst::new("cards")
                .child(inst::nested(
                    inst::new("card")
                        .prop("title", str("One"))
                        .child(inst::nested(
                            inst::new("tip").child(nodes::paragraph("Start here.")),
                        )),
                ))
                .build(),
        )
        .case(
            "invalid prop",
            inst::new("cards")
                .prop("cols", str("many"))
                .prop("col", PropValue::Num(2.0))
                .build(),
        )
        .check();
}

#[test]
fn columns() {
    Gallery::new("columns")
        .case(
            "default",
            inst::new("columns")
                .child(inst::nested(
                    inst::new("column").child(nodes::paragraph("Left.")),
                ))
                .child(inst::nested(
                    inst::new("column").child(nodes::paragraph("Right.")),
                ))
                .build(),
        )
        .case(
            "every prop",
            inst::new("columns")
                .prop("cols", PropValue::Num(3.0))
                .prop("gap", str("2rem"))
                .prop("align", str("center"))
                .child(inst::nested(
                    inst::new("column")
                        .prop("span", PropValue::Num(2.0))
                        .child(nodes::paragraph("Wide.")),
                ))
                .build(),
        )
        .case(
            "invalid align",
            inst::new("columns").prop("align", str("middle")).build(),
        )
        .case("empty body", inst::new("columns").build())
        .case(
            "nesting",
            inst::new("columns")
                .child(inst::nested(inst::new("column").child(inst::nested(
                    inst::new("card").prop("title", str("Inner")),
                ))))
                .build(),
        )
        .case(
            "invalid prop",
            inst::new("columns")
                .prop("cols", str("three"))
                .prop("column", PropValue::Num(1.0))
                .build(),
        )
        .check();
}

#[test]
fn tiles() {
    Gallery::new("tiles")
        .case(
            "default",
            inst::new("tiles")
                .child(inst::nested(
                    inst::new("tile")
                        .prop("title", str("Guides"))
                        .prop("icon", str("book"))
                        .prop("href", str("/guides")),
                ))
                .build(),
        )
        .case(
            "every prop",
            inst::new("tiles")
                .prop("cols", PropValue::Num(4.0))
                .child(inst::nested(
                    inst::new("tile")
                        .prop("title", str("Reference"))
                        .prop("href", str("/reference")),
                ))
                .build(),
        )
        .case("empty body", inst::new("tiles").build())
        .case(
            "nesting",
            inst::new("tiles")
                .child(inst::nested(
                    inst::new("tile")
                        .prop("title", str("Hub"))
                        .child(inst::nested(
                            inst::new("note").child(nodes::paragraph("Inside a tile.")),
                        )),
                ))
                .build(),
        )
        .case(
            "invalid prop",
            inst::new("tiles")
                .prop("cols", str("four"))
                .prop("gap", str("1rem"))
                .build(),
        )
        .check();
}
