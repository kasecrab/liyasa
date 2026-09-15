//! What every `ComponentRegistry` must do (PRD §34.9).

use std::collections::BTreeSet;

use super::require;
use crate::components::ComponentRegistry;

pub fn check(registry: &dyn ComponentRegistry) {
    let names = registry.names();
    require!(
        !names.is_empty(),
        "a registry with no components cannot resolve anything"
    );

    let mut seen = BTreeSet::new();
    for name in &names {
        require!(seen.insert(*name), "`{name}` is listed twice");
        let component = registry.get(name).unwrap_or_else(|| {
            panic!("contract violated: names() lists `{name}` but get() misses it")
        });
        require!(
            component.name() == *name,
            "get(\"{name}\") returned a component that calls itself `{}`",
            component.name()
        );
        require!(
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "`{name}` is not kebab-case"
        );

        for alias in component.aliases() {
            let resolved = registry.get(alias).unwrap_or_else(|| {
                panic!(
                    "contract violated: `{name}` declares alias `{alias}`, which does not resolve"
                )
            });
            require!(
                resolved.name() == component.name(),
                "alias `{alias}` resolves to `{}`, not to `{name}`",
                resolved.name()
            );
        }

        let schema = component.schema();
        for prop in &schema.props {
            require!(
                !(prop.required && prop.default.is_some()),
                "`{name}.{}` is required and also has a default",
                prop.name
            );
            require!(
                !prop.doc.is_empty(),
                "`{name}.{}` has no documentation",
                prop.name
            );
        }
        let mut prop_names = BTreeSet::new();
        for prop in &schema.props {
            require!(
                prop_names.insert(prop.name),
                "`{name}` declares `{}` twice",
                prop.name
            );
        }

        let editor = component.editor_block();
        for field in &editor.form {
            require!(
                schema.prop(&field.prop).is_some(),
                "`{name}`'s editor form edits `{}`, which is not in its schema",
                field.prop
            );
        }
    }

    require!(
        registry.get("definitely-not-a-component").is_none(),
        "an unknown name resolves to None"
    );
}
