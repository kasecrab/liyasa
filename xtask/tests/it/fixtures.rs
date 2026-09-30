//! A fixture path keyed on the process id alone (defect 248).

use xtask::fixtures;

/// The exact line that took `main` red. `cargo test` threads both `#[test]`s
/// through one process, so `site()` resolved to one directory for both and the
/// second call's `remove_dir_all` deleted what the first was asserting against.
#[test]
fn a_lone_process_id_is_reported() {
    let found = fixtures::scan(
        r#"
        let root = std::env::temp_dir().join(format!("liyasa-api-10-{}", std::process::id()));
        "#,
    );
    assert_eq!(
        found.len(),
        1,
        "the pid-only fixture was not reported: {found:?}"
    );
    assert_eq!(found[0].1, "liyasa-api-10-{}");
}

/// The fix, and every other fixture in the tree: a second hole. The pid still
/// separates concurrent runs; the name separates tests within one run.
#[test]
fn a_second_hole_is_enough() {
    for line in [
        r#"let root = std::env::temp_dir().join(format!("liyasa-api-10-{name}-{}", std::process::id()));"#,
        r#"let p = std::env::temp_dir().join(format!("liyasa-store-{name}-{}-{n}", std::process::id()));"#,
        r#"let d = root.join(format!("job-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));"#,
    ] {
        assert!(
            fixtures::scan(line).is_empty(),
            "{line} was reported, but the pid is not its only discriminator"
        );
    }
}

/// A pid in a string that is not a path is not this defect. `serve.rs` names a
/// worker after its process and has nothing to collide with.
#[test]
fn a_pid_outside_a_path_is_not_reported() {
    for line in [
        r#"let worker = std::env::var("X").unwrap_or_else(|_| format!("worker-{}", std::process::id()));"#,
        r#"let bytes = blake3::hash(&std::process::id().to_le_bytes());"#,
    ] {
        assert!(
            fixtures::scan(line).is_empty(),
            "{line} was reported and is not a fixture path"
        );
    }
}

/// Multi-line `format!` is the common spelling once a template has two holes,
/// and `rustfmt` chooses it, so a scanner that reads single lines would pass
/// over most of the tree without saying so.
#[test]
fn the_scan_reads_across_lines() {
    let one_hole = r#"
        let path = std::env::temp_dir().join(format!(
            "liyasa-analytics-{}",
            std::process::id()
        ));
    "#;
    assert_eq!(fixtures::scan(one_hole).len(), 1, "{one_hole}");

    let two_holes = r#"
        let path = std::env::temp_dir().join(format!(
            "liyasa-analytics-{name}-{}-{n}",
            std::process::id()
        ));
    "#;
    assert!(fixtures::scan(two_holes).is_empty(), "{two_holes}");
}
