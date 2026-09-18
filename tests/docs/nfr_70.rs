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
fn the_hosting_guide_includes_the_generated_matrix() {
    // The matrix is a snippet of its own rather than a region spliced into the
    // middle of the guide. `every_generated_page_matches_its_source` keeps the
    // snippet equal to the build's `MATRIX.md`; this keeps the guide pointing
    // at it, so the table cannot quietly stop being on the page.
    let page = fs::read_to_string(generate::repository().join("docs/guides/hosting.md"))
        .expect("the hosting guide");
    assert!(
        page.contains(r#"{% snippet "host-matrix" %}"#),
        "the hosting guide no longer includes the generated matrix; {REGENERATE}"
    );
    assert!(
        !page.contains("| Check | GitHub Pages |"),
        "the hosting guide carries a copy of the matrix as well as the include"
    );

    let snippet = fs::read_to_string(
        generate::repository()
            .join("docs")
            .join(generate::MATRIX_SNIPPET),
    )
    .expect("the generated matrix snippet");
    assert!(snippet.contains(&generate::matrix()), "{REGENERATE}");
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

// The codes with no hand-written body are pinned as a sorted list rather than
// counted. A single integer is one line that two packages documenting codes in
// the same window must both edit, and the resolution cannot be known without
// running the test: main and wp/18 reached 101 and 102 on 2026-09-18 and the
// answer was 97. A list lets them delete different lines, which git merges.
use generate::UNDOCUMENTED_PINS;

/// Every registered code with no `docs/errors/_notes/<CODE>.md`.
fn undocumented() -> BTreeSet<String> {
    generate::undocumented_codes().into_iter().collect()
}

/// The entries of a pin file, comments and blanks dropped. The shape matches
/// `tests/pins/` next door, which `xtask::pins::read` parses the same way.
fn pinned(file: &str) -> BTreeSet<String> {
    let path = generate::repository().join(file);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_undocumented_codes_are_the_pinned_ones() {
    let actual = undocumented();
    let pinned = pinned(UNDOCUMENTED_PINS);

    let documented: Vec<&String> = pinned.difference(&actual).collect();
    assert!(
        documented.is_empty(),
        "these codes now have a body, so DELETE their line from {UNDOCUMENTED_PINS}: \
         {documented:?}"
    );

    let undocumented: Vec<&String> = actual.difference(&pinned).collect();
    assert!(
        undocumented.is_empty(),
        "these codes have no body at `docs/errors/_notes/<CODE>.md`: {undocumented:?}. \
         A new code needs its note in the same commit that adds the row (CLAUDE.md). \
         If you are deliberately deferring one, add its line to {UNDOCUMENTED_PINS}"
    );
}

#[test]
fn the_pin_file_carries_its_own_instructions() {
    let text = fs::read_to_string(generate::repository().join(UNDOCUMENTED_PINS))
        .expect("the pin file is readable");
    assert!(
        text.contains("DELETE"),
        "it does not say how an entry comes out"
    );
    assert!(
        text.contains("--pins"),
        "it does not say how to regenerate itself"
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
