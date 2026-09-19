//! CI may not name a package or a binary the workspace does not have.

use std::path::{Path, PathBuf};

use xtask::workflows::{self, Kind};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn the_workspace_is_readable_and_its_binaries_are_found() {
    let members = workflows::members(&root()).expect("the manifests parse");
    assert!(
        members.len() > 15,
        "only {} members came out of the workspace; the walk is broken",
        members.len()
    );
    for expected in ["liyasa-core", "liyasa-cli", "xtask", "liyasa-benches"] {
        assert!(members.contains_key(expected), "the walk lost {expected}");
    }
    assert!(
        members["liyasa-cli"].contains("liyasa"),
        "the CLI's binary is named `liyasa`, not after its package: {:?}",
        members["liyasa-cli"]
    );
    assert!(
        members["liyasa-tests"].contains("docs-reference"),
        "an explicit [[bin]] row must be found: {:?}",
        members["liyasa-tests"]
    );
}

#[test]
fn every_name_ci_uses_exists() {
    let unknown = workflows::audit(&root()).expect("the workflows are readable");
    assert!(
        unknown.is_empty(),
        "{unknown:?}\n\
         A workflow names a package or a binary the workspace does not have. \
         Fix the name, or add the target."
    );
}

#[test]
fn a_name_that_is_not_there_is_reported() {
    // The check is green on this tree by construction, so drive the comparison
    // against a workspace whose members are known rather than trusting that a
    // green run means it ran.
    let members = workflows::members(&root()).expect("the manifests parse");
    assert!(!members.contains_key("liyasa-bench"), "the near-miss name");
    assert!(
        !members.contains_key("bench"),
        "the binary is not a package"
    );
    assert!(
        !members.values().flatten().any(|bin| bin == "liyasa-cli"),
        "the package name is not the binary name"
    );
}

#[test]
fn a_github_expression_is_not_a_name() {
    // `cargo check ${{ matrix.packages || '--workspace' }}` must not be read as
    // a package called `${{`, and `--bin` with nothing after it must not panic.
    let unknown = workflows::audit(&root()).expect("the workflows are readable");
    assert!(
        !unknown.iter().any(|u| u.name.contains("${{")),
        "an expression was read as a name: {unknown:?}"
    );
    assert!(
        !unknown
            .iter()
            .any(|u| u.kind == Kind::Package && u.name.starts_with('-')),
        "a flag was read as a name: {unknown:?}"
    );
}

#[test]
fn another_tool_s_dash_p_is_not_a_package() {
    // The first run of this check reported `mkdir -p corpus` in ci.yml as a
    // package called `corpus`. `-p` belongs to mkdir, cp and grep as well as to
    // cargo, so a flag counts only where cargo is what is being run. Both
    // branches are driven here, because a fix that switched the check off
    // would pass the negative half on its own.
    assert_eq!(
        workflows::values("          mkdir -p corpus", "-p"),
        Vec::<&str>::new()
    );
    assert_eq!(workflows::values("cp -p a b", "-p"), Vec::<&str>::new());
    assert_eq!(
        workflows::values(
            "      - run: cargo run -p xtask -- conformance corpus",
            "-p"
        ),
        vec!["xtask"]
    );
    assert_eq!(
        workflows::values(
            "cargo run --release -p liyasa-cli --bin liyasa -- build",
            "--bin"
        ),
        vec!["liyasa"]
    );
    // A rustup shim is spelled as a path, which is how `ps` shows it and how a
    // workflow may write it.
    assert_eq!(
        workflows::values("~/.cargo/bin/cargo build -p liyasa-core", "-p"),
        vec!["liyasa-core"]
    );
    // And the real file no longer reports it.
    let unknown = workflows::audit(&root()).expect("the workflows are readable");
    assert!(
        !unknown.iter().any(|u| u.name == "corpus"),
        "`mkdir -p corpus` is not a package reference: {unknown:?}"
    );
}
