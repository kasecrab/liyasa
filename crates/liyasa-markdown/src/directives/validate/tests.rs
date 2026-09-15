//! `spec/markdown/cm-52/slots/` and `spec/markdown/cm-54/unknown/`.

use super::nearest;
use crate::directives::testing::*;

#[test]
fn a_known_component_raises_nothing() {
    assert_eq!(codes(&document(":::note\nbody\n:::\n")), Vec::<&str>::new());
    assert_eq!(codes(&document("::divider\n")), Vec::<&str>::new());
    assert_eq!(codes(&document("a :kbd[K] b\n")), Vec::<&str>::new());
}

/// CM-54: the message has to name what the author probably meant.
#[test]
fn an_unknown_component_is_e0313_with_a_suggestion() {
    let document = document(":::calout\nbody\n:::\n");
    assert_eq!(codes(&document), ["E0313"]);
    let help = document
        .diagnostics
        .iter()
        .next()
        .and_then(|d| d.help.clone())
        .expect("a suggestion");
    assert!(help.contains("callout"), "{help}");
}

#[test]
fn an_unknown_leaf_and_an_unknown_inline_component_are_e0313_too() {
    assert_eq!(codes(&document("::imag{src=\"/a.png\"}\n")), ["E0313"]);
    assert_eq!(codes(&document("Press :kbdd[X].\n")), ["E0313"]);
}

#[test]
fn a_name_nothing_resembles_gets_no_suggestion() {
    let document = document(":::zzzzzzzzz\nbody\n:::\n");
    assert_eq!(codes(&document), ["E0313"]);
    assert!(
        document
            .diagnostics
            .iter()
            .next()
            .expect("one")
            .help
            .is_none()
    );
}

/// CM-52: written in the wrong form.
#[test]
fn a_container_written_as_a_leaf_is_e0317() {
    assert_eq!(codes(&document("::card\n")), ["E0317"]);
}

#[test]
fn an_inline_component_written_as_a_container_is_e0317() {
    assert_eq!(codes(&document(":::kbd\nbody\n:::\n")), ["E0317"]);
}

/// An empty container body is still a container, not a leaf.
#[test]
fn an_empty_container_is_not_mistaken_for_a_leaf() {
    assert_eq!(codes(&document(":::note\n:::\n")), Vec::<&str>::new());
}

#[test]
fn a_missing_required_prop_is_e0314() {
    assert_eq!(codes(&document("::image{alt=\"x\"}\n")), ["E0314"]);
}

#[test]
fn an_unknown_prop_is_w0316() {
    assert_eq!(
        codes(&document(":::card{nosuch=1}\nbody\n:::\n")),
        ["W0316"]
    );
}

/// `.class` and `#id` are shorthand every component accepts.
#[test]
fn the_shorthand_props_are_never_unknown() {
    assert_eq!(
        codes(&document(":::card{.wide #x}\nbody\n:::\n")),
        Vec::<&str>::new()
    );
}

#[test]
fn a_prop_of_the_wrong_type_is_e0315() {
    assert_eq!(
        codes(&document(":::card{columns=\"three\"}\nbody\n:::\n")),
        ["E0315"]
    );
    assert_eq!(
        codes(&document(":::card{open=\"yes\"}\nbody\n:::\n")),
        ["E0315"]
    );
    assert_eq!(
        codes(&document(":::card{variant=\"huge\"}\nbody\n:::\n")),
        ["E0315"]
    );
}

#[test]
fn a_prop_of_the_right_type_raises_nothing() {
    for source in [
        ":::card{columns=2}\nbody\n:::\n",
        ":::card{open=true}\nbody\n:::\n",
        ":::card{variant=\"wide\"}\nbody\n:::\n",
        ":::card{tags=[a,b]}\nbody\n:::\n",
        ":::card{href=\"/route\"}\nbody\n:::\n",
    ] {
        assert_eq!(codes(&document(source)), Vec::<&str>::new(), "{source}");
    }
}

/// An expression is unevaluated until expansion, so it fits every type.
#[test]
fn an_expression_prop_is_never_a_type_mismatch() {
    assert_eq!(
        codes(&document(
            ":::card{columns={{ page.columns }}}\nbody\n:::\n"
        )),
        Vec::<&str>::new()
    );
}

#[test]
fn a_tag_form_component_is_validated_like_a_directive() {
    assert_eq!(
        codes(&document("<Image alt=\"x\" />\n")),
        ["E0314"],
        "a tag form still needs its required props"
    );
}

#[test]
fn the_suggestion_is_the_nearest_name_within_a_budget() {
    let names = ["callout", "card", "note", "image"];
    assert_eq!(nearest("calout", &names), Some("callout"));
    assert_eq!(nearest("crad", &names), Some("card"));
    assert_eq!(nearest("note", &names), Some("note"));
    assert_eq!(nearest("zzzzzzzzz", &names), None);
    assert_eq!(nearest("", &names), None);
}

/// Two names the same distance away must not make the suggestion depend on
/// the registry's order.
#[test]
fn the_suggestion_is_stable_when_two_names_tie() {
    let names = ["bat", "cat"];
    assert_eq!(nearest("at", &names), nearest("at", &["cat", "bat"]));
}
