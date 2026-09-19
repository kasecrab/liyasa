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

use std::path::Path;

use crate::workspace;

pub const FORBIDDEN: &str = "unsafe_code";

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
