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
