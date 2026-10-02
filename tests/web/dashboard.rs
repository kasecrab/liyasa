//! The dashboard application's own tests, run from the workspace.
//!
//! `bin/gate` runs `cargo test`, so a suite that only `node --test` knows
//! about never runs in CI. `tests/web/reader.rs` and `tests/web/editor.rs`
//! are that bridge for the reader runtime and the editor; this is the one the
//! dashboard was missing.
//!
//! It was missing for the whole of WP-17. The thirteen files under
//! `web/dashboard/test/` have 107 tests and have only ever run when a session
//! typed `node --test` by hand — including `wiring.test.ts`, which is what
//! caught three endpoints that every page fetched and no renderer read, and
//! `css.test.ts`, which caught twelve emitted classes with no rule. Those
//! findings were real and the suite that found them was not protecting
//! anything afterwards: a regression in `pages.ts` would have been invisible
//! to the gate and to CI both.
//!
//! A suite nothing runs is worse than no suite, because the files look like
//! coverage to whoever reads the directory next.

use std::process::Command;

#[test]
fn the_dashboard_passes_its_own_tests() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../web/dashboard");
    let files: Vec<String> = std::fs::read_dir(format!("{root}/test"))
        .expect("the test directory is readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path().display().to_string())
        .filter(|path| path.ends_with(".test.ts"))
        .collect();
    assert!(!files.is_empty(), "the dashboard has no unit tests");

    let Ok(output) = Command::new("node")
        .arg("--disable-warning=ExperimentalWarning")
        .arg("--test")
        .args(&files)
        .current_dir(root)
        .output()
    else {
        eprintln!("node is not installed; the dashboard's unit tests did not run");
        return;
    };

    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
