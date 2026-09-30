//! The pinned lists, and the command that re-derives them.
//!
//! Each pins a set that should only shrink: the codes nothing raises
//! (RFC 0008), the flags prose names but the CLI does not define, and the
//! fixture paths keyed on the process id alone. The lists
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
pub const PID_FIXTURES: &str = "tests/pins/pid-keyed-fixtures.txt";

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

const FIXTURES_HEADER: &str = "\
# Fixture paths built from `process::id()` and nothing else (defect 248).
#
# `bin/gate` runs nextest, one process per test, so two tests keyed on the pid
# alone get different directories and the gate stays green. CI runs `cargo
# test`, which threads them through ONE process, so they get the same directory
# and one test's cleanup deletes what the other is asserting against. The gate
# cannot see this class at all.
#
# Each line is safe only while exactly one test reaches it. DELETE a line by
# interpolating something else as well — the test name, a counter, the thread
# id — which is what every other fixture in the tree already does.
#
# Regenerate with: cargo run -p xtask -- pins --update
";

/// Said when the pid-fixture set GREW, because the pin file's own header is
/// the second thing the reader meets and the mechanism is not guessable: the
/// gate cannot reproduce this failure at all.
const PID_FIXTURES_HINT: &str = "\
  A fixture path keyed on process::id() alone collides on CI and cannot collide
  under bin/gate: nextest is one process per test, `cargo test` is threads in
  one process. Interpolate the test name or a counter as well. Pinning the new
  line instead is only correct while exactly one test reaches it.";

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
    let fixtures: BTreeSet<String> = crate::fixtures::audit(root)?
        .into_iter()
        .map(|f| f.entry(root))
        .collect();

    if update {
        let mut changed = Vec::new();
        if write(root, UNRAISED_CODES, CODES_HEADER, &codes)? {
            changed.push(UNRAISED_CODES);
        }
        if write(root, PHANTOM_FLAGS, FLAGS_HEADER, &flags)? {
            changed.push(PHANTOM_FLAGS);
        }
        if write(root, PID_FIXTURES, FIXTURES_HEADER, &fixtures)? {
            changed.push(PID_FIXTURES);
        }
        if changed.is_empty() {
            println!(
                "pins: already current ({} codes, {} flags, {} pid fixtures)",
                codes.len(),
                flags.len(),
                fixtures.len()
            );
        } else {
            println!("pins: rewrote {} — commit the diff", changed.join(" and "));
        }
        return Ok(());
    }

    let mut stale = Vec::new();
    for (file, actual, gained_hint) in [
        (UNRAISED_CODES, &codes, ""),
        (PHANTOM_FLAGS, &flags, ""),
        (PID_FIXTURES, &fixtures, PID_FIXTURES_HINT),
    ] {
        let pinned = read(root, file)?;
        let gained: Vec<&String> = actual.difference(&pinned).collect();
        let lost: Vec<&String> = pinned.difference(actual).collect();
        if !gained.is_empty() || !lost.is_empty() {
            stale.push(format!("{file}: new {gained:?}, gone {lost:?}"));
            if !gained.is_empty() && !gained_hint.is_empty() {
                stale.push(gained_hint.to_owned());
            }
        }
    }
    if stale.is_empty() {
        println!(
            "pins: current ({} codes, {} flags, {} pid fixtures)",
            codes.len(),
            flags.len(),
            fixtures.len()
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
