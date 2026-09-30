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

#[test]
fn cases_are_counted_at_any_depth_not_at_the_top() {
    use xtask::preflight::md_files;

    // The importer writes `<suite>/<section>/<case>.md`, so counting top-level
    // entries reported "2 cases imported" for a corpus of 674. That matters
    // because the count IS the empty-corpus guard: two suite directories
    // holding nothing would satisfy a `> 0` check while the conformance run
    // over them passed vacuously — defect 147 reintroduced by its own guard.
    //
    // The fixture is shaped so the two readings give DIFFERENT numbers. A
    // check against the real corpus does not discriminate: counting its
    // directories also beats counting its root files, so the old bug would
    // have passed that test.
    let dir = std::env::temp_dir().join(format!("liyasa-preflight-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("commonmark/blank-lines")).expect("fixture");
    std::fs::create_dir_all(dir.join("gfm/tables-extension")).expect("fixture");
    for case in [
        "commonmark/blank-lines/one.md",
        "commonmark/blank-lines/two.md",
        "gfm/tables-extension/three.md",
    ] {
        std::fs::write(dir.join(case), "# case\n").expect("fixture");
    }
    // A stray non-case file must not be counted as one.
    std::fs::write(dir.join("commonmark/README.txt"), "notes\n").expect("fixture");

    let counted = md_files(&dir).expect("the fixture is readable");
    let top_level_dirs = std::fs::read_dir(&dir)
        .expect("readable")
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .count();

    assert_eq!(counted, 3, "three cases, at two different depths");
    assert_eq!(
        top_level_dirs, 2,
        "the fixture has the shape the bug needed"
    );
    assert_ne!(
        counted, top_level_dirs,
        "the fixture must distinguish the two readings, or this test cannot fail"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_corpus_of_empty_suite_directories_counts_zero() {
    use xtask::preflight::md_files;

    // The failure the guard exists for, stated directly: an import that made
    // the directories and no cases. The old count returned 2 here and the
    // `cases == 0` check passed.
    let dir = std::env::temp_dir().join(format!("liyasa-preflight-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("commonmark")).expect("fixture");
    std::fs::create_dir_all(dir.join("gfm")).expect("fixture");

    assert_eq!(
        md_files(&dir).expect("readable"),
        0,
        "two empty suite directories are zero cases, not two"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_cross_compiles_cover_every_package_and_target_ci_builds() {
    use xtask::preflight::CROSS;

    let ci = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join(".github/workflows/ci.yml"),
    )
    .expect("ci.yml");

    // PACKAGE and target together, not the target alone. The first version of
    // this compared targets only and passed with `liyasa-core` for
    // wasm32-wasip1 deleted from CROSS, because xtask's entry for the same
    // target satisfied it — a test that could not fail for the one case it was
    // written to catch. Proved by removing that row and watching it stay green.
    let mut wanted: Vec<(String, String)> = Vec::new();
    for line in ci.lines() {
        let (Some(p_at), Some(t_at)) = (line.find("-p "), line.find("--target ")) else {
            continue;
        };
        let package = line[p_at + 3..].split_whitespace().next().unwrap_or("");
        let target = line[t_at + 9..].split_whitespace().next().unwrap_or("");
        if target.starts_with("${{") || package.is_empty() || target.is_empty() {
            continue;
        }
        wanted.push((package.to_owned(), target.to_owned()));
    }
    assert!(
        !wanted.is_empty(),
        "ci.yml names no `-p ... --target ...` build; this test would pass over nothing"
    );

    for (package, target) in &wanted {
        assert!(
            CROSS.iter().any(|(_, p, t)| p == package && t == target),
            "ci.yml builds {package} for {target} and preflight does not"
        );
    }

    // And the one CI does NOT build directly, because nothing else compiles it
    // without a corpus: `parity` builds xtask for WASI, and parity needs the
    // upstream suites fetched.
    assert!(
        CROSS
            .iter()
            .any(|(_, p, t)| *p == "xtask" && *t == "wasm32-wasip1"),
        "xtask for WASI is the binary parity runs the corpus through"
    );

    // Each label is distinct, or two steps report under one name and a reader
    // cannot tell which failed.
    let mut names: Vec<&str> = CROSS.iter().map(|(n, _, _)| *n).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(names.len(), before, "two cross steps share a label");
}
