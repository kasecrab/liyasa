//! `unsafe_code` is forbidden, and a crate cannot opt out by omission (NFR-13).
//!
//! The workspace forbids `unsafe_code` once, in `[workspace.lints.rust]`, and
//! every crate takes it with `[lints] workspace = true`. That inheritance is
//! opt-in: a crate whose manifest omits those two lines compiles with the
//! default `allow`, and nothing says so — not the build, not clippy, not
//! `cargo deny`. The requirement says "`#![forbid(unsafe_code)]` in all Liyasa
//! crates", and today every crate is covered, so this is a ratchet rather than
//! a defect list: the twentieth crate is the one that will forget.
//!
//! `wasm-bindgen`'s generated glue is NFR-13's one reviewed exception, and it
//! is generated into a dependency rather than into a Liyasa crate, so no crate
//! here needs to claim it.
//!
//! This module also checks NFR-42's "no nightly features", for the same reason
//! and with the same shape. Nothing enforced it: `rust-toolchain.toml` pins a
//! stable channel, so a `#![feature(...)]` attribute simply fails to compile —
//! which sounds like enforcement until somebody runs `cargo +nightly` locally,
//! commits what built, and the failure surfaces as a confusing compile error
//! for the next person on stable rather than as the rule it broke.

use std::path::Path;

use crate::workspace;

pub const FORBIDDEN: &str = "unsafe_code";

/// A nightly-only feature gate. `#![feature(...)]` is the whole of it: there is
/// no stable spelling, so the attribute's presence IS the violation.
///
/// Matched at the START of a line, which is where a crate attribute has to be.
/// A plain substring search reported this very file and its test, because both
/// carry the marker inside a string literal — the checker's first run accused
/// itself. Line-anchoring separates the attribute from prose and code that
/// merely names it, and a real gate cannot hide from it: `#![...]` is an inner
/// attribute and must precede every item in the file.
const NIGHTLY_GATE: &str = "#![feature(";

/// Members whose manifest does not take the workspace lint table.
pub fn uncovered(root: &Path) -> Result<Vec<String>, String> {
    Ok(workspace::members(root)?
        .into_iter()
        .filter(|member| !member.inherits_lints())
        .map(|member| member.name)
        .collect())
}

/// What the workspace says about `unsafe_code`, if anything.
pub fn level(root: &Path) -> Result<Option<String>, String> {
    workspace::workspace_lint(root, FORBIDDEN)
}

/// Rust sources under the workspace that open a nightly feature gate.
///
/// Walks the tree rather than asking cargo, because a file that is not in any
/// target's module graph still gets committed, still gets read as an example
/// of how this workspace writes Rust, and is exactly where an experiment that
/// needed nightly would be parked.
pub fn nightly_gates(root: &Path) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    for dir in ["crates", "xtask", "benches", "tests"] {
        let start = root.join(dir);
        if start.is_dir() {
            walk(&start, root, &mut found)?;
        }
    }
    found.sort();
    Ok(found)
}

fn walk(dir: &Path, root: &Path, found: &mut Vec<String>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.is_dir() {
            // `target` holds generated sources from dependencies; a feature
            // gate in one of those is not this workspace writing it.
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            walk(&path, root, found)?;
        } else if path.extension().is_some_and(|e| e == "rs")
            && std::fs::read_to_string(&path)
                .map(|text| {
                    text.lines()
                        .any(|line| line.trim_start().starts_with(NIGHTLY_GATE))
                })
                .unwrap_or(false)
        {
            let shown = path.strip_prefix(root).unwrap_or(&path);
            found.push(shown.display().to_string());
        }
    }
    Ok(())
}

/// The pin in `rust-toolchain.toml` and the floor in `Cargo.toml`, when the
/// pin names a version rather than a channel.
///
/// `None` for `stable`, `nightly` or a dated channel: those are not versions
/// and comparing them to the floor would be comparing a promise to a number.
pub fn pin_and_floor(root: &Path) -> Result<Option<(String, String)>, String> {
    let pin_path = root.join("rust-toolchain.toml");
    let pin_text =
        std::fs::read_to_string(&pin_path).map_err(|e| format!("{}: {e}", pin_path.display()))?;
    let pin: toml::Value =
        toml::from_str(&pin_text).map_err(|e| format!("{}: {e}", pin_path.display()))?;
    let Some(channel) = pin
        .get("toolchain")
        .and_then(|t| t.get("channel"))
        .and_then(toml::Value::as_str)
    else {
        return Err(format!("{} declares no channel", pin_path.display()));
    };
    if !channel.starts_with(|c: char| c.is_ascii_digit()) {
        return Ok(None);
    }

    let manifest_path = root.join("Cargo.toml");
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let manifest: toml::Value =
        toml::from_str(&manifest_text).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let floor = manifest
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("rust-version"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("{} declares no rust-version", manifest_path.display()))?;

    Ok(Some((channel.to_owned(), floor.to_owned())))
}

pub fn run(root: &Path) -> Result<(), String> {
    let level = level(root)?;
    match level.as_deref() {
        Some("forbid") => println!("the workspace forbids {FORBIDDEN}"),
        Some(other) => {
            return Err(format!(
                "[workspace.lints.rust] {FORBIDDEN} is `{other}`; NFR-13 asks for `forbid`"
            ));
        }
        None => {
            return Err(format!(
                "[workspace.lints.rust] does not mention {FORBIDDEN}, so no crate inherits it"
            ));
        }
    }
    match pin_and_floor(root)? {
        Some((pin, floor)) if pin == floor => {
            println!("the toolchain is pinned to {pin}, which is the msrv floor")
        }
        Some((pin, floor)) => {
            return Err(format!(
                "rust-toolchain.toml pins {pin} and Cargo.toml's rust-version is {floor}; \
                 the version developers use and the floor CI tests must be one number"
            ));
        }
        // A channel rather than a version. NFR-42 asks for a pinned version,
        // but saying so is the requirement's business and not this check's:
        // reporting it here would make every branch red for a decision nobody
        // on that branch made.
        None => println!("the toolchain names a channel, not a version"),
    }
    let gates = nightly_gates(root)?;
    if gates.is_empty() {
        println!("no crate opens a nightly feature gate");
    } else {
        for path in &gates {
            println!("  {path} opens a nightly feature gate");
        }
        return Err(format!(
            "{} files use `#![feature(...)]`; the toolchain is stable-only",
            gates.len()
        ));
    }
    let uncovered = uncovered(root)?;
    if uncovered.is_empty() {
        println!("every workspace member inherits it");
        return Ok(());
    }
    for name in &uncovered {
        println!("  {name} has no `[lints] workspace = true`, so it may write unsafe");
    }
    Err(format!(
        "{} members opt out of the lint table",
        uncovered.len()
    ))
}
