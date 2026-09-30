//! Every dispatch arm carries the gate its module carries.
//!
//! `xtask` builds for `wasm32-wasip1` so `parity` can run `conformance`
//! through wasmtime, and the modules that read the CLI's clap tree or shell out
//! are `#[cfg(not(target_family = "wasm"))]`. An attribute covers ONE match
//! arm, so an arm inserted between a gate and the arm it was written for
//! inherits nothing — and the failure is a compile error on a target no local
//! gate builds. `main` went red that way on 2026-09-30.
//!
//! This reads source text, which is the right instrument here: what it checks
//! IS a declaration, not a behaviour a scan could only pretend to measure.

use std::path::{Path, PathBuf};

const GATE: &str = "#[cfg(not(target_family = \"wasm\"))]";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Modules `lib.rs` declares behind the wasm gate.
fn gated_modules(lib: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lines: Vec<&str> = lib.lines().collect();
    for (at, line) in lines.iter().enumerate() {
        let Some(rest) = line.trim().strip_prefix("pub mod ") else {
            continue;
        };
        let name = rest.trim_end_matches(';').to_owned();
        if lines[..at]
            .iter()
            .rev()
            .take_while(|above| {
                let t = above.trim();
                t.starts_with('#') || t.starts_with("///") || t.starts_with("//")
            })
            .any(|above| above.trim() == GATE)
        {
            out.push(name);
        }
    }
    out
}

/// Whether the arm dispatching to `module` carries the gate. `None` if no arm
/// dispatches to it at all.
fn arm_is_gated(main: &str, module: &str) -> Option<bool> {
    let lines: Vec<&str> = main.lines().collect();
    let needle = format!("{module}::");
    let at = lines
        .iter()
        .position(|line| line.trim_start().starts_with("Some(") && line.contains(&needle))?;
    Some(
        lines[..at]
            .iter()
            .rev()
            .take_while(|above| {
                let t = above.trim();
                t.starts_with('#') || t.starts_with("//")
            })
            .any(|above| above.trim() == GATE),
    )
}

#[test]
fn every_arm_for_a_gated_module_is_gated_too() {
    let lib = std::fs::read_to_string(root().join("src/lib.rs")).expect("lib.rs is readable");
    let main = std::fs::read_to_string(root().join("src/main.rs")).expect("main.rs is readable");

    let gated = gated_modules(&lib);
    assert!(
        gated.len() >= 3,
        "only {gated:?} came out of lib.rs; the parse is broken, not the file"
    );

    let mut checked = 0;
    for module in &gated {
        match arm_is_gated(&main, module) {
            None => continue,
            Some(true) => checked += 1,
            Some(false) => panic!(
                "`{module}` is gated in lib.rs and its dispatch arm in main.rs is not.\n\
                 Under wasm the name is not in scope and the arm references it, which is a\n\
                 compile error no local gate reaches: bin/gate does not build for\n\
                 wasm32-wasip1. Reproduce with\n\
                 \x20 RUSTFLAGS= cargo build -p xtask --target wasm32-wasip1\n\
                 (empty, to drop the host's mold flag, which rust-lld rejects)."
            ),
        }
    }
    assert!(
        checked >= 2,
        "no gated module had a dispatch arm, so this test asserted nothing"
    );
}
