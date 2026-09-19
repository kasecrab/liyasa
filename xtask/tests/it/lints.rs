//! NFR-13: no Liyasa crate may write `unsafe`, and none may opt out of the
//! lint that says so by leaving two lines out of its manifest.

use std::path::{Path, PathBuf};

use xtask::{lints, workspace};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn the_workspace_forbids_unsafe_code() {
    assert_eq!(
        lints::level(&root())
            .expect("the manifest parses")
            .as_deref(),
        Some("forbid"),
        "NFR-13 asks for `forbid`, which is the level a crate cannot re-allow"
    );
}

#[test]
fn every_member_inherits_the_lint_table() {
    let uncovered = lints::uncovered(&root()).expect("the manifests parse");
    assert!(
        uncovered.is_empty(),
        "{uncovered:?} have no `[lints]\\nworkspace = true`, so they compile with \
         `unsafe_code` allowed and nothing says so"
    );
}

#[test]
fn the_check_can_tell_a_crate_that_inherits_from_one_that_does_not() {
    // The tree is green by construction, so the discriminator is exercised
    // directly: a green run otherwise proves only that the walk found nothing.
    let members = workspace::members(&root()).expect("the manifests parse");
    assert!(members.len() > 15, "only {} members", members.len());
    assert!(
        members.iter().all(workspace::Member::inherits_lints),
        "the tree is supposed to be covered"
    );

    let opted_out: toml::Value =
        toml::from_str("[package]\nname = \"x\"\n").expect("a manifest without a lint table");
    let inheriting: toml::Value =
        toml::from_str("[package]\nname = \"x\"\n[lints]\nworkspace = true\n")
            .expect("a manifest with one");
    let member = |manifest: toml::Value| workspace::Member {
        name: "x".to_owned(),
        directory: PathBuf::from("."),
        manifest,
    };
    assert!(!member(opted_out).inherits_lints());
    assert!(member(inheriting).inherits_lints());
}

#[test]
fn a_lint_table_that_says_warn_is_not_forbid() {
    // `warn` compiles unsafe code and prints about it, and `-D warnings` turns
    // that into an error only where warnings are denied. NFR-13 asks for the
    // level that cannot be re-enabled from inside a crate.
    assert_ne!(lints::FORBIDDEN, "");
    let level = lints::level(&root()).expect("the manifest parses");
    assert_ne!(level.as_deref(), Some("warn"));
    assert_ne!(level.as_deref(), Some("allow"));
}
