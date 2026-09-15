//! The frozen contracts this crate implements (§31.7 gate item 5).

use liyasa_components::Registry;
use liyasa_core::components::ComponentRegistry;
use liyasa_core::conformance::component_registry;

#[test]
fn the_builtin_registry_is_a_component_registry() {
    component_registry::check(&Registry::builtins());
}

#[test]
fn an_empty_registry_resolves_nothing() {
    let registry = Registry::new();
    assert!(registry.is_empty());
    assert!(registry.get("note").is_none());
    assert!(registry.names().is_empty());
}

#[test]
fn every_component_declares_an_editor_form_for_every_prop() {
    let registry = Registry::builtins();
    for name in registry.names() {
        let component = registry.get(name).expect("a listed name resolves");
        let schema = component.schema();
        let form = component.editor_block().form;
        assert_eq!(
            form.len(),
            schema.props.len(),
            "`{name}` has {} props and {} form fields",
            schema.props.len(),
            form.len()
        );
    }
}

#[test]
fn a_typo_suggests_the_component_it_meant() {
    let registry = Registry::builtins();
    assert_eq!(registry.suggest("note"), Some("note"));
    assert_eq!(registry.suggest("Note"), Some("note"));
    assert_eq!(registry.suggest("waring"), Some("warning"));
    assert_eq!(registry.suggest("qqqqqqqqqqqq"), None);
}

#[test]
fn registering_a_name_twice_replaces_it() {
    let mut registry = Registry::builtins();
    let before = registry.len();
    registry.add(liyasa_components::components::callout::Note);
    assert_eq!(registry.len(), before);
}

#[test]
fn a_nested_component_reports_into_the_same_sink() {
    use liyasa_components::{HtmlCtx, Reference, inst, nodes};

    let registry = Registry::builtins();
    let reference = Reference::with(&registry);
    let outer = inst::new("note")
        .child(inst::nested(inst::new("cards").child(inst::nested(
            inst::new("tip").child(nodes::paragraph("Not a card.")),
        ))))
        .build();
    let note = registry.resolve("note").expect("registered");
    let mut ctx = HtmlCtx::new(&reference);
    note.html(&outer, &mut ctx).expect("renders");
    let codes: Vec<&str> = ctx
        .shared
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect();
    assert_eq!(
        codes,
        ["E0354"],
        "a diagnostic two levels down must surface"
    );
}

#[test]
fn a_nested_component_sees_the_site_origin() {
    use liyasa_components::render::Shared;
    use liyasa_components::{MarkdownCtx, Reference, inst};
    use liyasa_core::document::PropValue;
    use liyasa_core::ids::Locale;
    use liyasa_core::markdown::SiteMeta;

    let registry = Registry::builtins();
    let reference = Reference::with(&registry);
    let site = SiteMeta {
        name: "Liyasa".into(),
        canonical_origin: "https://docs.example.com/".parse().expect("a URL"),
        llms_txt: "https://docs.example.com/llms.txt".parse().expect("a URL"),
        version: None,
        locale: Locale::new("en"),
    };
    let group = inst::new("cards")
        .child(inst::nested(
            inst::new("card")
                .prop("title", PropValue::Str("Start".into()))
                .prop("href", PropValue::Str("/start".into())),
        ))
        .build();
    let cards = registry.resolve("cards").expect("registered");
    let mut ctx = MarkdownCtx::with(Shared::new(&reference).site(&site));
    cards.markdown(&group, &mut ctx).expect("serializes");
    assert_eq!(ctx.finish(), "- [Start](https://docs.example.com/start)\n");
}
