//! The reader runtime's own tests, run from the workspace.
//!
//! `bin/gate` runs `cargo test`, so a suite that only `npm test` knows about
//! would never run in CI. This target is the bridge: it runs
//! `node --test web/reader/test/*.test.ts` and fails with what the runner
//! printed. A machine without Node skips it rather than failing, the way
//! `tests/budget/thm_31.rs` treats a missing `gzip`.

use std::process::Command;

#[test]
fn the_reader_runtime_passes_its_own_tests() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../web/reader");
    let files: Vec<String> = std::fs::read_dir(format!("{root}/test"))
        .expect("the test directory is readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path().display().to_string())
        .filter(|path| path.ends_with(".test.ts"))
        .collect();
    assert!(!files.is_empty(), "the reader has no unit tests");

    let Ok(output) = Command::new("node")
        .arg("--disable-warning=ExperimentalWarning")
        .arg("--test")
        .args(&files)
        .current_dir(root)
        .output()
    else {
        eprintln!("node is not installed; the reader's unit tests did not run");
        return;
    };

    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
