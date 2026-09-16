//! The behaviour hooks the theme's runtime binds to (`plan/rfcs/0400-component-behaviour-hooks.md`).
//!
//! `data-liyasa` says what an element is; these say what the runtime should
//! attach to it. They are asserted here rather than only in the golden files
//! because a golden diff shows an attribute went missing without saying that a
//! tab group stopped being keyboard-navigable when it did.

use liyasa_components::{HtmlCtx, Reference, Registry, inst, nodes};
use liyasa_core::components::ComponentInst;
use liyasa_core::document::PropValue;

fn str(value: &str) -> PropValue {
    PropValue::Str(value.to_owned())
}

fn html(inst: &ComponentInst) -> String {
    let registry = Registry::builtins();
    let reference = Reference::with(&registry);
    let component = registry
        .resolve(&inst.name)
        .unwrap_or_else(|| panic!("`{}` is not registered", inst.name));
    let mut ctx = HtmlCtx::new(&reference);
    component.html(inst, &mut ctx).expect("renders");
    ctx.finish()
}

#[test]
fn a_tab_group_is_an_enhanceable_tablist() {
    let markup = html(
        &inst::new("tabs")
            .child(inst::nested(
                inst::new("tab")
                    .prop("title", str("npm"))
                    .child(nodes::paragraph("Run npm.")),
            ))
            .build(),
    );
    assert!(markup.contains("data-ly-tabs"), "{markup}");
    assert!(markup.contains(r#"data-liyasa="tabs""#), "{markup}");
}

#[test]
fn a_code_group_is_an_enhanceable_tablist() {
    let markup = html(
        &inst::new("code-group")
            .child(nodes::code_block(Some("sh"), "npm i liyasa\n"))
            .build(),
    );
    assert!(markup.contains("data-ly-tabs"), "{markup}");
}

#[test]
fn every_disclosure_is_an_enhanceable_accordion() {
    for inst in [
        inst::new("accordion").prop("title", str("What?")).build(),
        inst::new("expandable").prop("title", str("fields")).build(),
        inst::new("note")
            .prop("collapsible", PropValue::Bool(true))
            .build(),
    ] {
        let markup = html(&inst);
        assert!(
            markup.starts_with("<details") && markup.contains("data-ly-accordion"),
            "`{}`: {markup}",
            inst.name
        );
    }
}

/// A callout that is not collapsible is not a disclosure, so it must not carry
/// the hook: `accordion.js` would bind a toggle to an element that never opens.
#[test]
fn a_plain_callout_carries_no_disclosure_hook() {
    let markup = html(&inst::new("note").child(nodes::paragraph("Body.")).build());
    assert!(!markup.contains("data-ly-accordion"), "{markup}");
}

#[test]
fn a_copy_button_names_the_body_it_copies_and_starts_hidden() {
    let markup = html(
        &inst::new("code-group")
            .child(nodes::code_block(Some("sh"), "npm i liyasa\n"))
            .build(),
    );
    let at = markup
        .find("data-ly-copy=\"")
        .unwrap_or_else(|| panic!("no copy hook in {markup}"));
    let rest = &markup[at + "data-ly-copy=\"".len()..];
    let target = &rest[..rest.find('"').expect("a closed attribute")];
    assert!(!target.is_empty(), "{markup}");
    assert!(markup.contains(&format!("id=\"{target}\"")), "{markup}");
    assert!(
        markup[..at].ends_with("hidden ") || markup[at..].contains(" hidden"),
        "the copy button must ship hidden: {markup}"
    );
}

#[test]
fn a_dismissible_banner_names_the_id_its_dismissal_is_remembered_against() {
    let markup = html(
        &inst::new("banner")
            .prop("id", str("v05"))
            .prop("dismissible", PropValue::Bool(true))
            .child(nodes::paragraph("Version 0.5 is out."))
            .build(),
    );
    assert!(markup.contains(r#"data-ly-banner-id="v05""#), "{markup}");
}

#[test]
fn a_feedback_form_and_its_answers_are_reachable_by_the_runtime() {
    let markup = html(&inst::new("feedback").build());
    assert!(markup.contains("data-ly-feedback"), "{markup}");
    assert!(
        markup.contains(r#"data-ly-feedback-value="yes""#)
            && markup.contains(r#"data-ly-feedback-value="no""#),
        "{markup}"
    );
}

#[test]
fn an_assistant_button_is_what_the_lazy_loader_looks_for() {
    let markup = html(
        &inst::new("assistant")
            .prop("prompt", str("How do I verify a code sample?"))
            .build(),
    );
    assert!(markup.contains("data-ly-assistant-trigger"), "{markup}");
}

/// A page with two of the same `id` fails axe outright, and a copy button that
/// names a duplicate copies whichever element the browser found first.
#[test]
fn every_copy_target_on_a_page_is_unique() {
    let markup = liyasa_components::gallery::page("tabs", "/theme.css", "/base.js");
    let mut ids: Vec<&str> = Vec::new();
    let mut rest = markup.as_str();
    while let Some(at) = rest.find("<code id=\"") {
        rest = &rest[at + "<code id=\"".len()..];
        let end = rest.find('"').expect("a closed attribute");
        ids.push(&rest[..end]);
    }
    assert!(ids.len() > 1, "the fixture needs more than one fence");
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "duplicate ids: {ids:?}");
}

/// The shell is a document a browser can parse: `<head>` holds the metadata,
/// `<body>` holds the content, and neither closes early.
#[test]
fn a_gallery_page_is_a_well_formed_document() {
    let markup = liyasa_components::gallery::page("card", "/theme.css", "/base.js");
    let head = markup.find("</head>").expect("a closed head");
    let body = markup.find("<body").expect("a body");
    assert!(head < body, "head must close before body opens: {markup}");
    assert!(
        markup
            .find("<link rel=\"stylesheet\"")
            .expect("a stylesheet")
            < head,
        "the stylesheet belongs in the head: {markup}"
    );
    assert!(
        markup.find("<script").expect("the runtime")
            < markup.find("</body>").expect("a closed body"),
        "the runtime belongs in the body: {markup}"
    );
    assert!(markup.trim_end().ends_with("</html>"), "{markup}");
}
