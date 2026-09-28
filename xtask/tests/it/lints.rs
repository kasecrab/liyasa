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

#[test]
fn no_crate_opens_a_nightly_feature_gate() {
    // NFR-42 says "no nightly features". A stable toolchain makes
    // `#![feature(...)]` fail to compile, which looks like enforcement until
    // somebody builds with `cargo +nightly` locally and commits what worked:
    // the next person on stable then gets a compile error about an unstable
    // feature rather than a statement of the rule it broke.
    let gates = lints::nightly_gates(&root()).expect("the workspace is readable");
    assert!(
        gates.is_empty(),
        "these open a nightly feature gate: {gates:?}"
    );
}

#[test]
fn the_nightly_check_finds_a_gate_that_is_there() {
    // The half that matters. An empty result is what this check returns when
    // it is working AND when it is looking in the wrong place, and those are
    // the same colour, so the detector is driven against a file that has one.
    let dir = std::env::temp_dir().join(format!("liyasa-nightly-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("crates/liyasa-example/src")).expect("fixture");
    std::fs::write(
        dir.join("crates/liyasa-example/src/lib.rs"),
        "#![feature(let_chains)]\npub fn f() {}\n",
    )
    .expect("fixture");
    // A file that merely mentions the words must NOT be reported: the check is
    // for the attribute, not for prose about it.
    std::fs::write(
        dir.join("crates/liyasa-example/src/notes.rs"),
        "//! We do not use feature gates; see NFR-42.\n",
    )
    .expect("fixture");
    // Nor a file carrying the marker inside a string. The first version of this
    // check was a substring search and it reported its OWN source and this very
    // test, because both spell the attribute to look for it.
    std::fs::write(
        dir.join("crates/liyasa-example/src/detector.rs"),
        "const GATE: &str = \"#![feature(\";\npub fn find() -> &'static str { GATE }\n",
    )
    .expect("fixture");

    let found = lints::nightly_gates(&dir).expect("readable");
    assert_eq!(found, vec!["crates/liyasa-example/src/lib.rs".to_owned()]);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_pinned_version_and_the_msrv_floor_are_one_number() {
    // They are two files and they drift silently: `rust-toolchain.toml` decides
    // what a developer compiles with and `Cargo.toml`'s `rust-version` decides
    // what CI's msrv job tests, so a mismatch means the floor is never the
    // thing anybody actually builds.
    //
    // A channel is not a failure here. NFR-42 does ask for a pinned version and
    // the toolchain currently names `stable`, but that is the requirement's
    // status — `bin/requirement` records it — and not something this test may
    // assert, because a test that fails for an unmet requirement reddens every
    // branch for a decision nobody on that branch made. The pin was tried on
    // 2026-09-28 and reverted; see state/wp-32/NOTES.md for why.
    match lints::pin_and_floor(&root()).expect("both files are readable") {
        Some((pin, floor)) => assert_eq!(pin, floor, "the pin and the floor disagree"),
        None => eprintln!("rust-toolchain.toml names a channel, so there is no version to compare"),
    }
}

#[test]
fn a_pin_that_disagrees_with_the_floor_is_caught() {
    // The half that matters, driven against a fixture: the real tree names a
    // channel today, so the comparison above does not execute and an empty
    // result would look identical to a working check.
    let dir = std::env::temp_dir().join(format!("liyasa-pin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture");
    std::fs::write(
        dir.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.98.1\"\n",
    )
    .expect("fixture");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[workspace.package]\nrust-version = \"1.97.0\"\n",
    )
    .expect("fixture");
    assert_eq!(
        lints::pin_and_floor(&dir).expect("readable"),
        Some(("1.98.1".to_owned(), "1.97.0".to_owned())),
        "a disagreement must be reported, not smoothed over"
    );

    // And a channel really does return None rather than being compared.
    std::fs::write(
        dir.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"stable\"\n",
    )
    .expect("fixture");
    assert_eq!(lints::pin_and_floor(&dir).expect("readable"), None);

    let _ = std::fs::remove_dir_all(&dir);
}
