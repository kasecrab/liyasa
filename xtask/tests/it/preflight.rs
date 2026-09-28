//! The preflight's own claim: that it checks the versions CI checks.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace root is xtask's parent")
        .to_path_buf()
}

#[test]
fn the_versions_come_from_ci_and_match_what_ci_pins() {
    let (commonmark, gfm) = xtask::preflight::versions(&root()).expect("ci.yml declares both");

    // Not a shape check. The point of reading them from `ci.yml` is that a
    // preflight checking a different CommonMark release than CI would report a
    // green CI does not have, so the values are compared against the file.
    let ci = std::fs::read_to_string(root().join(".github/workflows/ci.yml")).expect("ci.yml");
    assert!(
        ci.contains(&format!("COMMONMARK_VERSION: \"{commonmark}\"")),
        "read {commonmark}, which ci.yml does not pin"
    );
    assert!(
        ci.contains(&format!("GFM_SPEC_REF: \"{gfm}\"")),
        "read {gfm}, which ci.yml does not pin"
    );

    // And they are versions, not the empty string a lenient parse would give
    // for `COMMONMARK_VERSION:` with nothing after it.
    assert!(
        commonmark.starts_with(|c: char| c.is_ascii_digit()),
        "{commonmark} is not a version"
    );
    assert!(gfm.contains("gfm"), "{gfm} is not a GFM spec ref");
}

#[test]
fn a_workflow_missing_the_versions_is_not_a_default() {
    // The failure that would matter: a preflight that silently fell back to a
    // hardcoded version would keep passing after CI moved, which is the exact
    // drift this reads the file to avoid.
    assert_eq!(
        xtask::preflight::versions_in("name: ci\non: push\njobs:\n  build:\n"),
        None
    );
    // Declared but empty is the same thing, and a lenient parse would return
    // an empty string here and then fetch a 404 URL.
    assert_eq!(
        xtask::preflight::versions_in("env:\n  COMMONMARK_VERSION:\n  GFM_SPEC_REF:\n"),
        None
    );
    // One without the other is still not enough to run.
    assert_eq!(
        xtask::preflight::versions_in("env:\n  COMMONMARK_VERSION: \"0.31.2\"\n"),
        None
    );
    // Quoted, unquoted and single-quoted all read the same.
    assert_eq!(
        xtask::preflight::versions_in(
            "env:\n  COMMONMARK_VERSION: 0.31.2\n  GFM_SPEC_REF: '0.29.0.gfm.13'\n"
        ),
        Some(("0.31.2".to_owned(), "0.29.0.gfm.13".to_owned()))
    );
}

#[test]
fn the_host_linker_flag_is_removed_and_nothing_else_is() {
    use xtask::parity::without_mold;

    // What `bin/buildenv` actually exports, and the reason parity could not run
    // locally at all: rust-lld answers `unknown argument: -fuse-ld=mold`.
    assert_eq!(
        without_mold("-C link-arg=-fuse-ld=mold"),
        Some(String::new())
    );
    // The one-token spelling cargo also accepts.
    assert_eq!(
        without_mold("-Clink-arg=-fuse-ld=mold"),
        Some(String::new())
    );

    // Anything else a session set on purpose survives. Clearing RUSTFLAGS
    // wholesale would drop these silently.
    assert_eq!(
        without_mold("-D warnings -C link-arg=-fuse-ld=mold --cfg foo"),
        Some("-D warnings --cfg foo".to_owned())
    );

    // Removing half of the two-token form would leave a bare `-C` and rustc
    // would reject the command line — a worse failure than the one being fixed.
    let rest = without_mold("-C link-arg=-fuse-ld=mold -C debuginfo=1").expect("mold was there");
    assert!(!rest.split_whitespace().any(|t| t == "-C") || rest.contains("-C debuginfo=1"));
    assert_eq!(rest, "-C debuginfo=1");

    // No mold, no change: the caller leaves the environment alone rather than
    // rewriting it, so a machine without mold keeps its exact flags.
    assert_eq!(without_mold("-D warnings"), None);
    assert_eq!(without_mold(""), None);
}
