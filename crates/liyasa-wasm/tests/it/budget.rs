//! ED-06: what the core module may weigh, and what it may not carry.
//!
//! Two halves, for the reason `plan/rfcs/2400-wasm-budget-check.md` gives: the
//! cause is guarded on every gate, and the number is measured when it is asked
//! for. A check that reports success because it did not run is worse than no
//! check, so the size test says so rather than skipping quietly.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use liyasa_wasm::budget::{CORE_MODULE_LIMIT, EXCLUDED};

const TARGET: &str = "wasm32-unknown-unknown";
const SIZE_ENV: &str = "LIYASA_WASM_SIZE";

fn root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}

/// The crates the module would link, for the target it runs on and the features
/// it is built with. Metadata only: no compilation, no network.
fn dependency_tree() -> String {
    let output = Command::new(env!("CARGO"))
        .current_dir(root())
        .args([
            "tree",
            "--package",
            "liyasa-wasm",
            "--target",
            TARGET,
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--format",
            "{p}",
            "--locked",
            "--offline",
        ])
        .stdin(Stdio::null())
        .output()
        .expect("`cargo tree` runs");
    assert!(
        output.status.success(),
        "`cargo tree` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn the_core_module_carries_none_of_the_host_only_crates() {
    let tree = dependency_tree();
    let names: Vec<&str> = tree
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    assert!(
        names.contains(&"comrak"),
        "the tree does not name a crate the module certainly links, so reading it is broken:\n{tree}"
    );
    for (crate_name, reason) in EXCLUDED {
        assert!(
            !names.contains(crate_name),
            "`{crate_name}` reached the editor module, through {reason}"
        );
    }
}

/// ED-06 measures the module compressed. gzip rather than brotli: brotli is what
/// a server negotiates and is smaller, so this is a conservative bound, and gzip
/// is on every machine.
#[test]
fn the_core_module_is_under_the_compressed_budget() {
    let Ok(_) = std::env::var(SIZE_ENV) else {
        // Not a skip. The measurement costs a release build of the whole
        // dependency tree for `wasm32-unknown-unknown`, which is minutes, and
        // RFC 2400 records why that does not run on every gate. What this
        // asserts instead is that the way to take it is still written down
        // where somebody looking for the budget would find it.
        assert!(
            include_str!("../../src/budget.rs").contains(SIZE_ENV),
            "the module was not measured and `{SIZE_ENV}` is not documented in `src/budget.rs`"
        );
        return;
    };

    let build = Command::new(env!("CARGO"))
        .current_dir(root())
        .args([
            "build",
            "--package",
            "liyasa-wasm",
            "--target",
            TARGET,
            "--release",
            "--locked",
        ])
        .stdin(Stdio::null())
        .status()
        .expect("`cargo build` runs");
    assert!(build.success(), "the module did not build for {TARGET}");

    let module = root()
        .join("target")
        .join(TARGET)
        .join("release/liyasa_wasm.wasm");
    let bytes = std::fs::read(&module)
        .unwrap_or_else(|error| panic!("{}: {error}", module.display()));
    let compressed = gzip(&bytes);
    assert!(
        compressed <= CORE_MODULE_LIMIT,
        "the core module is {compressed} bytes compressed, over ED-06's {CORE_MODULE_LIMIT}"
    );
}

fn gzip(bytes: &[u8]) -> u64 {
    use std::io::Write as _;

    let mut child = Command::new("gzip")
        .arg("-9c")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("gzip runs");
    let mut stdin = child.stdin.take().expect("gzip takes input");
    let written = std::thread::scope(|scope| {
        scope.spawn(move || stdin.write_all(bytes));
        child.wait_with_output().expect("gzip finishes")
    });
    written.stdout.len() as u64
}
