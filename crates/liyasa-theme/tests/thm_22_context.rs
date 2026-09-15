//! THM-22: the documented context of each partial and the context the theme
//! actually passes are the same thing, checked in both directions.

use std::collections::BTreeSet;

use liyasa_theme::context::{RenderContext, reference};

fn resolve<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    path.split('.')
        .try_fold(value, |current, key| current.get(key))
}

#[test]
fn every_documented_key_exists_in_the_context() {
    let context = serde_json::to_value(RenderContext::sample()).expect("context serializes");
    for doc in reference() {
        for key in doc.keys {
            // `item` and `depth` are loop variables the sidebar sets, not
            // fields of the context; they are documented where they are used.
            if matches!(*key, "item" | "depth" | "code" | "callout" | "component") {
                continue;
            }
            assert!(
                resolve(&context, key).is_some(),
                "`{}` documents `{key}`, which the context does not carry",
                doc.partial
            );
        }
    }
}

#[test]
fn every_top_level_context_key_is_documented_by_some_partial() {
    let context = serde_json::to_value(RenderContext::sample()).expect("context serializes");
    let documented: BTreeSet<&str> = reference()
        .iter()
        .flat_map(|doc| doc.keys.iter().copied())
        .map(|key| key.split('.').next().unwrap_or(key))
        .collect();
    for key in context.as_object().expect("an object").keys() {
        assert!(
            documented.contains(key.as_str()),
            "`{key}` is in the context but no partial documents it"
        );
    }
}

#[test]
fn every_partial_has_documentation_and_at_least_one_key() {
    for doc in reference() {
        assert!(
            !doc.doc.is_empty(),
            "`{}` has no documentation",
            doc.partial
        );
        assert!(!doc.keys.is_empty(), "`{}` documents no keys", doc.partial);
    }
}

#[test]
fn the_documented_field_names_are_the_serialized_ones() {
    // A field renamed in Rust without updating the reference must fail here
    // rather than silently render empty under lenient undefined semantics.
    let context = serde_json::to_value(RenderContext::sample()).expect("context serializes");
    assert!(resolve(&context, "page.markdownUrl").is_some());
    assert!(resolve(&context, "page.lastModified").is_some());
    assert!(resolve(&context, "site.builtWith").is_some());
    assert!(resolve(&context, "nav.activeRoute").is_some());
    assert!(resolve(&context, "assets.customCss").is_some());
    assert!(
        resolve(&context, "page.last_modified").is_none(),
        "the context is camelCase; a snake_case key means a rename was missed"
    );
}
