//! CMP-20 and CMP-21: the semantic callouts and the custom one.

use liyasa_components::{inst, nodes};
use liyasa_core::document::PropValue;

use crate::support::Gallery;

#[test]
fn semantic_callouts() {
    Gallery::new("note")
        .case(
            "default",
            inst::new("note")
                .child(nodes::paragraph("Read this first."))
                .build(),
        )
        .case(
            "every prop",
            inst::new("note")
                .prop("title", PropValue::Str("Before you start".into()))
                .prop("icon", PropValue::Str("rocket".into()))
                .prop("collapsible", PropValue::Bool(true))
                .prop("open", PropValue::Bool(true))
                .build(),
        )
        .case("empty body", inst::new("note").build())
        .case(
            "nesting",
            inst::new("note")
                .child(nodes::paragraph("Outer."))
                .child(inst::nested(
                    inst::new("tip").child(nodes::paragraph("Inner.")),
                ))
                .build(),
        )
        .case(
            "invalid prop",
            inst::new("note")
                .prop("titel", PropValue::Str("typo".into()))
                .prop("collapsible", PropValue::Num(3.0))
                .build(),
        )
        .case(
            "escaping",
            inst::new("note")
                .prop("title", PropValue::Str(r#"<script>"&'"#.into()))
                .child(nodes::paragraph("5 < 6 & 7 > 6"))
                .build(),
        )
        .check();
}

#[test]
fn every_semantic_variant() {
    let mut gallery = Gallery::new("callouts");
    for name in ["note", "tip", "warning", "info", "check", "danger"] {
        gallery = gallery.case(
            name,
            inst::new(name)
                .child(nodes::paragraph("Body text."))
                .build(),
        );
    }
    gallery.check();
}

#[test]
fn custom_callout() {
    Gallery::new("callout")
        .case(
            "default",
            inst::new("callout")
                .child(nodes::paragraph("Plain."))
                .build(),
        )
        .case(
            "every prop",
            inst::new("callout")
                .prop("title", PropValue::Str("Heads up".into()))
                .prop("icon", PropValue::Str("bell".into()))
                .prop("color", PropValue::Str("#ff8800".into()))
                .prop("variant", PropValue::Str("solid".into()))
                .prop("collapsible", PropValue::Bool(true))
                .child(nodes::paragraph("Body."))
                .build(),
        )
        .case(
            "invalid variant",
            inst::new("callout")
                .prop("variant", PropValue::Str("neon".into()))
                .build(),
        )
        .check();
}
