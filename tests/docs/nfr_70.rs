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

/// How many codes may still be waiting for a hand-written body.
///
/// A ratchet, not a target. `CLAUDE.md` requires a note in the same commit as a
/// new code, so this can only go down; if it goes up, someone added a code
/// without writing what it means, and a page of generated scaffolding that says
/// "not written yet" is honest but is not documentation.
const UNWRITTEN_CAP: usize = 106;

#[test]
fn the_unwritten_pages_are_counted_and_shrinking() {
    let errors = generate::repository().join("docs/errors");
    let mut unwritten: Vec<String> = Vec::new();
    for entry in fs::read_dir(&errors).expect("docs/errors") {
        let entry = entry.expect("a directory entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".md") || name == "index.md" {
            continue;
        }
        if fs::read_to_string(entry.path()).is_ok_and(|text| text.contains(generate::UNWRITTEN)) {
            unwritten.push(name);
        }
    }
    unwritten.sort();
    assert!(
        unwritten.len() <= UNWRITTEN_CAP,
        "{} codes have no written body, over the cap of {UNWRITTEN_CAP}. A new code \
         needs its note in `docs/errors/_notes/<CODE>.md` in the same commit \
         (CLAUDE.md). Undocumented: {unwritten:#?}",
        unwritten.len()
    );
    assert_eq!(
        unwritten.len(),
        UNWRITTEN_CAP,
        "the undocumented count has dropped to {}; lower UNWRITTEN_CAP to match so \
         it cannot drift back up",
        unwritten.len()
    );
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
        if stem != "index" && !known.contains(stem) {
            orphans.push(name);
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
    let pages: String = fs::read_dir(generate::repository().join("docs/reference/gallery"))
        .expect("docs/reference/gallery")
        .map(|entry| {
            fs::read_to_string(entry.expect("a directory entry").path()).unwrap_or_default()
        })
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

#[test]
fn every_gallery_example_parses_cleanly() {
    let registry = liyasa_components::registry::Registry::builtins();
    let canonical: BTreeSet<&str> = registry
        .all_names()
        .filter_map(|name| registry.resolve(name))
        .map(|component| component.name())
        .collect();

    let mut broken = Vec::new();
    for name in canonical {
        let Some(example) = generate::example(name) else {
            continue;
        };
        let diagnostics = build_one_page(name, example);
        if !diagnostics.is_empty() {
            broken.push(format!("{name}: {diagnostics:?}"));
        }
    }
    assert!(broken.is_empty(), "{broken:#?}");
}

/// Builds a one-page site whose body is the example, and returns its errors.
fn build_one_page(name: &str, body: &str) -> Vec<String> {
    let root = std::env::temp_dir().join(format!(
        "liyasa-example-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("assets")).expect("a project directory");
    fs::write(
        root.join("liyasa.json"),
        r#"{"name":"Examples","seo":{"canonicalOrigin":"https://example.com"},
            "regions":{"enabled":true,"list":["us","eu"],"default":"us"}}"#,
    )
    .expect("a config");
    fs::write(
        root.join("assets/example.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
    )
    .expect("an asset");
    fs::write(
        root.join("index.md"),
        format!("---\ntitle: Example\ndescription: One component.\n---\n\n# Example\n\n{body}\n"),
    )
    .expect("a page");

    let vfs = liyasa_config::vfs::OsVfs::new(&root);
    let report = liyasa_build::engine::build(
        &vfs,
        &liyasa_build::git::NoGit,
        &root,
        &liyasa_build::engine::Options {
            build_time: Some(liyasa_tests::docs::BUILD_TIME),
            ..Default::default()
        },
    );
    let out = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.is_error())
        .map(|diagnostic| format!("{} {}", diagnostic.code, diagnostic.message))
        .collect();
    let _ = fs::remove_dir_all(&root);
    out
}

#[test]
fn every_cli_command_is_documented() {
    use clap::CommandFactory;
    let page = fs::read_to_string(generate::repository().join("docs/reference/cli.md"))
        .expect("the CLI reference");

    let mut missing = Vec::new();
    walk_commands(
        &liyasa_cli::cli::Cli::command(),
        "liyasa",
        &mut |full, command| {
            if !page.contains(&format!("`{full}`")) {
                missing.push(full.to_owned());
            }
            for arg in command.get_arguments() {
                if arg.is_hide_set() || arg.get_id() == "help" || arg.get_id() == "version" {
                    continue;
                }
                let Some(long) = arg.get_long() else { continue };
                if !page.contains(&format!("`--{long}`")) {
                    missing.push(format!("{full} --{long}"));
                }
            }
        },
    );
    assert!(
        missing.is_empty(),
        "undocumented commands and flags: {missing:#?}; {REGENERATE}"
    );
}

/// Every command in the tree, with the full invocation that names it.
fn walk_commands(command: &clap::Command, path: &str, out: &mut impl FnMut(&str, &clap::Command)) {
    for sub in command.get_subcommands().filter(|sub| !sub.is_hide_set()) {
        let full = format!("{path} {}", sub.get_name());
        out(&full, sub);
        walk_commands(sub, &full, out);
    }
}
