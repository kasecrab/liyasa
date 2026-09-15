//! `spec/markdown/cm-52/slots/`.

use liyasa_core::document::BlockKind;

use crate::directives::testing::*;

fn slot_names(source: &str) -> Vec<String> {
    let document = document(source);
    let BlockKind::Component { slots, .. } = &component_named(&document.root, "card").kind else {
        panic!("expected a component");
    };
    slots.0.keys().cloned().collect()
}

#[test]
fn one_named_slot() {
    assert_eq!(
        slot_names("::::card\nbody\n:::slot{name=\"footer\"}\nf\n:::\n::::\n"),
        ["footer"]
    );
}

/// Sibling slots need a longer fence on the component, or comrak's container
/// algorithm closes the component at the first `:::` — which is what it does
/// for any two same-length fences.
#[test]
fn two_named_slots() {
    assert_eq!(
        slot_names(
            "::::card\n:::slot{name=\"header\"}\nh\n:::\n:::slot{name=\"footer\"}\nf\n:::\n::::\n"
        ),
        ["footer", "header"]
    );
}

#[test]
fn the_rest_of_the_body_stays_where_it_was() {
    let document = document("::::card\nbody\n:::slot{name=\"footer\"}\nf\n:::\n::::\n");
    let card = component_named(&document.root, "card");
    assert_eq!(card.children.len(), 1, "only the body is left as a child");
    assert!(components(&document.root).iter().all(|b| !matches!(
        &b.kind,
        BlockKind::Component { name, .. } if name == "slot"
    )));
}

#[test]
fn a_slot_outside_a_component_is_e0350_and_keeps_its_content() {
    let document = document(":::slot{name=\"footer\"}\nf\n:::\n");
    assert_eq!(codes(&document), ["E0350"]);
    assert!(
        blocks(&document.root)
            .into_iter()
            .any(|b| matches!(b.kind, BlockKind::Paragraph))
    );
}

#[test]
fn a_slot_without_a_name_is_e0350() {
    assert_eq!(
        codes(&document("::::card\n:::slot\nf\n:::\n::::\n")),
        ["E0350"]
    );
}

#[test]
fn a_slot_the_component_does_not_declare_is_e0350() {
    assert_eq!(
        codes(&document(
            "::::card\n:::slot{name=\"nope\"}\nx\n:::\n::::\n"
        )),
        ["E0350"]
    );
}
