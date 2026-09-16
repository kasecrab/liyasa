//! NFR-70: every error code has a page, every config key has an entry, every
//! component has a live example, and none of them is written by hand.
//!
//! The generated pages are checked in so that the site builds from a clean
//! checkout, which means they can go stale. These tests are what stops that:
//! they regenerate from the code registry, the schemas, the component
//! registry, and the build's own host matrix, and compare.

use std::collections::BTreeSet;
use std::fs;

use liyasa_core::diagnostics::registry;
use liyasa_tests::docs::generate;

/// `cargo run -p liyasa-tests --bin docs-reference` is the fix for a failure here.
const REGENERATE: &str = "run `cargo run -p liyasa-tests --bin docs-reference`";

#[test]
fn every_generated_page_matches_its_source() {
    let docs = generate::repository().join("docs");
    let mut stale = Vec::new();
    for file in generate::files() {
        let path = docs.join(&file.path);
        match fs::read_to_string(&path) {
            Ok(current) if current == file.text => {}
            Ok(_) => stale.push(format!("{} differs", file.path)),
            Err(error) => stale.push(format!("{}: {error}", file.path)),
        }
    }
    assert!(stale.is_empty(), "{stale:#?}; {REGENERATE}");
}

#[test]
fn the_hosting_guide_carries_the_matrix_verbatim() {
    let page = fs::read_to_string(generate::hosting_page()).expect("the hosting guide");
    assert!(
        page.contains(&generate::matrix()),
        "the hosting guide does not carry the current host matrix verbatim; {REGENERATE}"
    );
    assert_eq!(page, generate::splice_matrix(&page), "{REGENERATE}");
}

#[test]
fn every_error_code_has_a_page() {
    let docs = generate::repository().join("docs/errors");
    let missing: Vec<String> = registry()
        .iter()
        .map(|info| info.code.to_string())
        .filter(|code| !docs.join(format!("{code}.md")).exists())
        .collect();
    assert!(missing.is_empty(), "codes with no page: {missing:?}");
}

#[test]
fn every_error_page_belongs_to_a_registered_code() {
    let known: BTreeSet<String> = registry()
        .iter()
        .map(|info| info.code.to_string())
        .collect();
    let mut orphans = Vec::new();
    for entry in fs::read_dir(generate::repository().join("docs/errors")).expect("docs/errors") {
        let entry = entry.expect("a directory entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".md") else {
            continue;
        };
        if stem == "index" || !known.contains(stem) {
            if stem != "index" {
                orphans.push(name);
            }
        }
    }
    assert!(
        orphans.is_empty(),
        "pages for codes that are not in codes.toml: {orphans:?}"
    );
}

#[test]
fn every_top_level_config_key_has_a_reference_entry() {
    let schema: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(generate::repository().join("schemas/liyasa.schema.json"))
            .expect("the config schema"),
    )
    .expect("valid JSON");
    let properties = schema["properties"]
        .as_object()
        .expect("the schema has properties");

    let reference = generate::repository().join("docs/reference/config");
    let missing: Vec<&String> = properties
        .keys()
        .filter(|key| !key.starts_with('$'))
        .filter(|key| !reference.join(format!("{key}.md")).exists())
        .collect();
    assert!(missing.is_empty(), "config keys with no entry: {missing:?}");
}

#[test]
fn every_component_has_a_live_example() {
    let registry = liyasa_components::registry::Registry::builtins();
    let pages: String = fs::read_dir(generate::repository().join("docs/reference/components"))
        .expect("docs/reference/components")
        .map(|entry| fs::read_to_string(entry.expect("a directory entry").path()).unwrap_or_default())
        .collect();

    // `all_names` includes every alias; a component documents under its
    // canonical name, so resolve each back to that first.
    let canonical: BTreeSet<&str> = registry
        .all_names()
        .filter_map(|name| registry.resolve(name))
        .map(|component| component.name())
        .collect();

    let mut missing = Vec::new();
    for name in canonical {
        // The heading the generator writes for the component's own section.
        if !pages.contains(&format!("## `{name}`")) {
            missing.push(name.to_owned());
            continue;
        }
        // An example is the directive itself, in one of its three forms.
        let used = pages.contains(&format!(":::{name}"))
            || pages.contains(&format!("::{name}"))
            || pages.contains(&format!(":{name}["))
            || pages.contains(&format!(":{name}{{"));
        if !used {
            missing.push(name.to_owned());
        }
    }
    assert!(
        missing.is_empty(),
        "components with no rendered example: {missing:?}"
    );
}
