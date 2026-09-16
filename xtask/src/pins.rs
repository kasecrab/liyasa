//! The two pinned lists, and the command that re-derives them.
//!
//! Both checks pin a set that should only shrink: the codes nothing raises
//! (RFC 0008) and the flags prose names but the CLI does not define. The lists
//! live under `tests/pins/` rather than beside their checks for one reason —
//! `bin/path-guard`'s always-writable list covers top-level `tests/` but not
//! `crates/liyasa-core/tests/` or `xtask/`. A package that fixes a flag or
//! makes a code emit has to lower the pin in the same commit, and it can only
//! do that if the file is one it may write. Otherwise every such fix needs
//! WP-00 to follow it, which does not scale to thirty packages.
//!
//! So: `cargo run -p xtask -- pins --update`, commit the diff, done.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const UNRAISED_CODES: &str = "tests/pins/unraised-codes.txt";
pub const PHANTOM_FLAGS: &str = "tests/pins/phantom-flags.txt";

const CODES_HEADER: &str = "\
# Codes registered in codes.toml that nothing in crates/*/src/ raises (RFC 0008).
#
# Claiming a code before the thing that raises it is the intended workflow, so
# this is not a list of defects. It is pinned so the set cannot change without
# someone saying why: add a line when you claim a code ahead of its
# implementation, and DELETE the line in the commit that makes it raise.
#
# Regenerate with: cargo run -p xtask -- pins --update
";

const FLAGS_HEADER: &str = "\
# Flags named in a doc comment or a diagnostic that the `liyasa` CLI does not
# define. Each is a defect: the worse ones reach a user in help text.
#
# DELETE a line in the commit that builds the flag or fixes the text. A flag
# belonging to another tool goes in FOREIGN in xtask/src/flags.rs instead, with
# a note saying whose.
#
# Regenerate with: cargo run -p xtask -- pins --update
";

/// The entries of a pin file, comments and blanks dropped.
pub fn read(root: &Path, file: &str) -> Result<BTreeSet<String>, String> {
    let path = root.join(file);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

fn write(
    root: &Path,
    file: &str,
    header: &str,
    entries: &BTreeSet<String>,
) -> Result<bool, String> {
    let path = root.join(file);
    let mut body = String::from(header);
    for entry in entries {
        body.push_str(entry);
        body.push('\n');
    }
    if std::fs::read_to_string(&path).is_ok_and(|current| current == body) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(true)
}

/// Entry point for `xtask pins [--update]`.
pub fn run(root: &Path, update: bool) -> Result<(), String> {
    let codes = crate::codes::unraised(root)?;
    let flags: BTreeSet<String> = crate::flags::audit(root)?
        .into_iter()
        .map(|p| p.flag)
        .collect();

    if update {
        let mut changed = Vec::new();
        if write(root, UNRAISED_CODES, CODES_HEADER, &codes)? {
            changed.push(UNRAISED_CODES);
        }
        if write(root, PHANTOM_FLAGS, FLAGS_HEADER, &flags)? {
            changed.push(PHANTOM_FLAGS);
        }
        if changed.is_empty() {
            println!(
                "pins: already current ({} codes, {} flags)",
                codes.len(),
                flags.len()
            );
        } else {
            println!("pins: rewrote {} — commit the diff", changed.join(" and "));
        }
        return Ok(());
    }

    let mut stale = Vec::new();
    for (file, actual) in [(UNRAISED_CODES, &codes), (PHANTOM_FLAGS, &flags)] {
        let pinned = read(root, file)?;
        let gained: Vec<&String> = actual.difference(&pinned).collect();
        let lost: Vec<&String> = pinned.difference(actual).collect();
        if !gained.is_empty() || !lost.is_empty() {
            stale.push(format!("{file}: new {gained:?}, gone {lost:?}"));
        }
    }
    if stale.is_empty() {
        println!(
            "pins: current ({} codes, {} flags)",
            codes.len(),
            flags.len()
        );
        return Ok(());
    }
    Err(format!(
        "{}\nrun `cargo run -p xtask -- pins --update` and commit the diff",
        stale.join("\n")
    ))
}

/// The workspace root, from this crate's manifest directory.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}
