//! CMP-82 through a whole build: the variant the build renders reaches the
//! component that gates on it.
//!
//! `crates/liyasa-components/tests/it/gates.rs` pins what a component does with
//! a variant it is given. `tests/build/cmp_80_gates.rs` pins what a gated block
//! ships. Neither of those can tell whether the engine passes the variant at
//! all, because a build whose only variant is the default withholds every gated
//! block for the correct reason — the gate working — and for the wrong one —
//! the variant never arriving. Only a page that declares more than one variant
//! separates them.

use std::fs;
use std::path::PathBuf;

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;

const SECRET: &str = "The admin console lives at /internal/console.";
const PUBLIC: &str = "Everyone may read this paragraph.";

struct Project(PathBuf);

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A one-page site whose page declares an `admin` group and gates a block on it.
fn admin_gated_site() -> Project {
    let root = std::env::temp_dir().join(format!("liyasa-cmp-82-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a project directory");
    fs::write(
        root.join("liyasa.json"),
        r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
    )
    .expect("config");
    fs::write(
        root.join("index.md"),
        &format!(
            "---\ntitle: Home\ngroups: [admin]\n---\n# Home\n\n{PUBLIC}\n\n\
             :::visibility{{groups=[\"admin\"]}}\n{SECRET}\n:::\n"
        ),
    )
    .expect("a page");
    Project(root)
}

fn build(project: &Project) {
    let vfs = OsVfs::new(&project.0);
    let report = engine::build(
        &vfs,
        &NoGit,
        &project.0,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    assert!(!report.failed(false), "{:?}", report.diagnostics);
}

/// The home page's variants, by file name. `404.html` and any other page the
/// build writes of its own accord are not variants of this one.
fn pages(project: &Project) -> Vec<(String, String)> {
    let dist = project.0.join("dist");
    let mut out = Vec::new();
    for entry in fs::read_dir(&dist).expect("dist") {
        let entry = entry.expect("a directory entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "index.html" || (name.starts_with("index.") && name.ends_with(".html")) {
            out.push((name, fs::read_to_string(entry.path()).expect("a page")));
        }
    }
    out.sort();
    out
}

#[test]
fn the_variant_that_admits_a_gated_block_is_the_one_that_ships_it() {
    let project = admin_gated_site();
    build(&project);
    let pages = pages(&project);
    // Names only in every message here: a failing assertion that prints two
    // whole rendered pages buries what it was trying to say.
    let names: Vec<&str> = pages.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.len() > 1, "only one variant was built: {names:?}");

    let carrying: Vec<&str> = pages
        .iter()
        .filter(|(_, body)| body.contains(SECRET))
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(
        carrying.len(),
        1,
        "exactly one of {names:?} admits the block, not {carrying:?}"
    );
    assert_ne!(
        carrying[0], "index.html",
        "the default variant is what every reader without the group gets"
    );
    for (name, body) in &pages {
        assert!(body.contains(PUBLIC), "{name} lost its ungated paragraph");
    }
    assert!(
        names.contains(&"index.g-admin.html"),
        "the group's own variant is what carries it: {names:?}"
    );
}

/// The half that makes the assertion above mean something: without the group,
/// nothing gated is in the file a reader who is not in it receives.
#[test]
fn the_default_variant_carries_nothing_that_was_gated() {
    let project = admin_gated_site();
    build(&project);
    let default = fs::read_to_string(project.0.join("dist/index.html")).expect("dist/index.html");
    assert!(!default.contains(SECRET), "{default}");
    assert!(default.contains(PUBLIC), "{default}");
}
