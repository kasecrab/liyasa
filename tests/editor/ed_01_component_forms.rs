//! ED-01's properties form needs each component's prop declarations, and the
//! browser cannot read Rust.
//!
//! > directives become component blocks with a properties form
//!
//! `liyasa_core::components` anticipated this: every `Component` has an
//! `editor_block()` — icon, category, inline, and a `FormField` per prop with
//! its widget, label and help — generated from its `PropSchema` by the
//! `component!` macro, so no component chooses a widget by hand.
//!
//! `EditorBlock` alone is not enough for a form that can say what is *missing*:
//! it carries the widget and the help but not `required`, and not an enum's
//! choices. So this writes both halves.
//!
//! This is a generated file that is checked in, which RFC 2433 says needs a
//! justification: something that cannot read the source needs it, and that is
//! the browser. Unlike the code list this replaced, the source is one package's
//! declarations (`crates/liyasa-components/`), not a file every package appends
//! to — so it drifts when WP-04 changes a component, which is a deliberate act
//! by one package, rather than whenever anybody merges anything. It never
//! writes during a gate: see `blessed`.

use std::path::PathBuf;

use liyasa_components::Registry;
use liyasa_core::components::{PropType, Widget};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/editor/src/component-forms.ts")
}

fn widget_name(widget: Widget) -> &'static str {
    match widget {
        Widget::Text => "text",
        Widget::Number => "number",
        Widget::Toggle => "toggle",
        Widget::Select => "select",
        Widget::Icon => "icon",
        Widget::Asset => "asset",
        Widget::Route => "route",
        Widget::Color => "color",
        Widget::Code => "code",
        // `Widget` is `#[non_exhaustive]`: a new variant must be named here
        // rather than silently becoming a text box, which is what a `_ =>`
        // fallback would do.
        other => panic!("`Widget::{other:?}` has no name in the editor's form table"),
    }
}

/// The choices a `Select` widget offers, and nothing for any other widget.
fn choices(ty: &PropType) -> Option<Vec<String>> {
    match ty {
        PropType::Enum(values) => Some(values.clone()),
        PropType::List(item) => choices(item),
        _ => None,
    }
}

/// The directive name of every builtin, once each.
///
/// `Registry::all_names` yields every *key*, which includes the tag-form
/// aliases — `Accordion`, `AccordionGroup`, `Colour` — so iterating it gives
/// 131 entries for two dozen components, most of them duplicates under a
/// capitalised name no directive ever uses. The properties form is keyed on the
/// directive name the scanner puts in a `Segment::DirectiveOpen`, which is
/// `Component::name`.
fn directive_names(registry: &Registry) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = registry
        .all_names()
        .filter_map(|key| registry.resolve(key).map(|component| component.name()))
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

fn generated() -> String {
    let registry = Registry::builtins();
    let names = directive_names(&registry);

    let mut out = String::new();
    out.push_str(
        "// Generated from `liyasa_components::Registry::builtins()` by\n\
         // `tests/editor/ed_01_component_forms.rs`. Do not edit: that test compares this\n\
         // file with the registry and fails when they differ.\n\
         //\n\
         // Each component's `editor_block()` gives the widget, label and help per prop;\n\
         // `schema()` gives `required` and an enum's choices, which a form needs to say\n\
         // what is missing rather than only what is set.\n\n",
    );
    out.push_str(
        "export interface ComponentProp {\n  \
         prop: string;\n  widget: string;\n  label: string;\n  help: string;\n  \
         required: boolean;\n  choices?: string[];\n}\n\n",
    );
    out.push_str(
        "export interface ComponentForm {\n  \
         name: string;\n  icon: string;\n  category: string;\n  inline: boolean;\n  \
         props: ComponentProp[];\n  /** Slot names, and whether the component needs one. */\n  \
         slots: { name: string; required: boolean; help: string }[];\n}\n\n",
    );
    out.push_str("export const COMPONENT_FORMS: ComponentForm[] = [\n");

    for name in names {
        let Some(component) = registry.resolve(name) else {
            continue;
        };
        let block = component.editor_block();
        let schema = component.schema();
        out.push_str("  {\n");
        out.push_str(&format!("    name: {},\n", json(name)));
        out.push_str(&format!("    icon: {},\n", json(&block.icon)));
        out.push_str(&format!("    category: {},\n", json(&block.category)));
        out.push_str(&format!("    inline: {},\n", block.inline));
        out.push_str("    props: [\n");
        for field in &block.form {
            let def = schema.prop(&field.prop);
            out.push_str("      { ");
            out.push_str(&format!("prop: {}, ", json(&field.prop)));
            out.push_str(&format!("widget: {}, ", json(widget_name(field.widget))));
            out.push_str(&format!("label: {}, ", json(&field.label)));
            out.push_str(&format!("help: {}, ", json(&field.help)));
            out.push_str(&format!(
                "required: {}",
                def.is_some_and(|def| def.required)
            ));
            if let Some(values) = def.and_then(|def| choices(&def.ty)) {
                let listed: Vec<String> = values.iter().map(|value| json(value)).collect();
                out.push_str(&format!(", choices: [{}]", listed.join(", ")));
            }
            out.push_str(" },\n");
        }
        out.push_str("    ],\n");
        out.push_str("    slots: [\n");
        for slot in &schema.slots {
            out.push_str(&format!(
                "      {{ name: {}, required: {}, help: {} }},\n",
                json(slot.name),
                slot.required,
                json(slot.doc)
            ));
        }
        out.push_str("    ],\n");
        out.push_str("  },\n");
    }
    out.push_str("];\n");
    out
}

fn json(text: &str) -> String {
    serde_json::to_string(text).expect("a declaration is a string")
}

#[test]
fn the_checked_in_component_forms_are_the_registrys() {
    blessed(
        &fixture(),
        &generated(),
        "`liyasa_components::Registry::builtins()`",
        "ed_01_component_forms::the_checked_in_component_forms_are_the_registrys",
    );
}

#[test]
fn the_table_is_keyed_on_directive_names_not_on_tag_aliases() {
    // The first version of this generator iterated `all_names`, which yields
    // every registry key including the tag-form aliases, and wrote 131 entries
    // for two dozen components — most of them under a capitalised name no
    // directive uses. A form keyed on those could never match a
    // `Segment::DirectiveOpen`.
    let registry = Registry::builtins();
    let distinct = directive_names(&registry);
    let keys = registry.all_names().count();
    assert!(
        distinct.len() < keys,
        "aliases are not being collapsed: {} distinct names for {keys} keys",
        distinct.len()
    );
    for name in &distinct {
        assert!(
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "`{name}` is not a kebab-case directive name"
        );
    }
}

#[test]
fn every_builtin_component_is_offered_with_an_icon_and_a_category() {
    // A component the editor cannot place in its insert menu is one an author
    // cannot reach, and an empty category would put it nowhere.
    let registry = Registry::builtins();
    assert!(registry.len() > 20, "the builtins are registered");
    for name in directive_names(&registry) {
        let component = registry.resolve(name).expect("a registered name resolves");
        let block = component.editor_block();
        assert!(!block.icon.is_empty(), "`{name}` has no editor icon");
        assert!(
            !block.category.is_empty(),
            "`{name}` has no editor category"
        );
    }
}

#[test]
fn every_prop_in_a_form_has_help_and_a_named_widget() {
    // The form shows the help beside the field, so an empty one is a field
    // nobody can fill without reading the source.
    let registry = Registry::builtins();
    let mut seen = 0usize;
    for name in directive_names(&registry) {
        let component = registry.resolve(name).expect("a registered name resolves");
        for field in &component.editor_block().form {
            assert!(
                !field.help.is_empty(),
                "`{name}.{}` has no help for the properties form",
                field.prop
            );
            assert!(
                !field.label.is_empty(),
                "`{name}.{}` has no label",
                field.prop
            );
            // Panics rather than defaulting if `Widget` grows a variant.
            let _ = widget_name(field.widget);
            seen += 1;
        }
    }
    assert!(seen > 40, "only {seen} props across every component");
}

#[test]
fn a_select_widget_always_carries_its_choices() {
    // A select with no options is a field an author can only leave alone. The
    // widget comes from the prop type, so `Select` and `Enum` must agree.
    let registry = Registry::builtins();
    for name in directive_names(&registry) {
        let component = registry.resolve(name).expect("a registered name resolves");
        let schema = component.schema();
        for field in &component.editor_block().form {
            if field.widget != Widget::Select {
                continue;
            }
            let def = schema.prop(&field.prop).expect("a form field names a prop");
            assert!(
                choices(&def.ty).is_some_and(|values| !values.is_empty()),
                "`{name}.{}` is a select with no choices",
                field.prop
            );
        }
    }
}

/// Rewrites a generated file, but only when asked.
///
/// A test that repairs the tree it is checking leaves a modified tracked file
/// behind every gate that finds drift, which produced a false "main is RED" on
/// 2026-09-17. A gate never sets `LIYASA_BLESS`, so a gate never writes.
fn blessed(path: &std::path::Path, fresh: &str, source: &str, test: &str) {
    let committed = std::fs::read_to_string(path).unwrap_or_default();
    if committed == fresh {
        return;
    }
    if std::env::var_os("LIYASA_BLESS").is_some() {
        std::fs::write(path, fresh).expect("the generated file is writable");
        panic!("{} was rewritten from {source}; commit it", path.display());
    }
    panic!(
        "{} no longer matches {source}. It is generated; do not edit it by hand. Run:\n\n    \
         LIYASA_BLESS=1 cargo test -p liyasa-tests --test it -- {test}\n\n\
         and commit what it writes.",
        path.display(),
    );
}
