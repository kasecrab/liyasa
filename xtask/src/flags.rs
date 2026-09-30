//! Prose that names a flag the CLI does not have (WP-29's finding).
//!
//! Twice in one afternoon: a doc comment described "`liyasa import
//! --report-components`", a flag nobody had built, and `E1103`'s help text said
//! "pass `--force` to overwrite", which reaches an end user in a diagnostic.
//! Both were found by a person asking a question.
//!
//! The real flag set comes from `Cli::command()` — clap already holds it, so
//! there is no second list to keep in step.
//!
//! **Scope is the whole design.** A flag-shaped string can be Chromium's, git's
//! or Vale's, and this codebase invokes all three; it can also belong to the
//! `liyasa-server` or `liyasa-search` binaries, which have their own command
//! trees. Scanning every string literal finds 54 such flags and no bugs, and a
//! lint that cries wolf gets switched off. So this reads only the two places
//! the real defects lived: doc comments, and the text of a diagnostic. That
//! drops every foreign flag without an allow-list to maintain. The cost is
//! false negatives — a message assembled somewhere unusual is not seen — which
//! is the right way round for a lint.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use clap::CommandFactory;

/// Flag-shaped strings that are real flags of something other than `liyasa`.
/// Each says whose, because "trust me" in an allow-list is how a lint rots.
pub const FOREIGN: &[(&str, &str)] = &[
    ("--bin", "cargo's, in a `cargo run -p … --bin …` example"),
    (
        "--brand",
        "a CSS custom property in `var(--brand)`; the theme's own are `--ly-`",
    ),
    (
        "--expect",
        "the `liyasa-search` binary's CLI, a different command tree",
    ),
    (
        "--state",
        "the `liyasa-server` binary's CLI, a different command tree",
    ),
    (
        "--acme-domain",
        "the `liyasa-server` binary's CLI, a different command tree",
    ),
    (
        "--acme-contact",
        "the `liyasa-server` binary's CLI, a different command tree",
    ),
    (
        "--acme-directory",
        "the `liyasa-server` binary's CLI, a different command tree",
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phantom {
    pub flag: String,
    pub file: PathBuf,
    pub line: usize,
    /// The line it was found on, trimmed.
    pub text: String,
}

/// Every long flag the `liyasa` binary accepts, at any depth, plus aliases.
pub fn known_flags() -> BTreeSet<String> {
    fn walk(command: &clap::Command, out: &mut BTreeSet<String>) {
        for arg in command.get_arguments() {
            if let Some(long) = arg.get_long() {
                out.insert(format!("--{long}"));
            }
            for alias in arg.get_all_aliases().into_iter().flatten() {
                out.insert(format!("--{alias}"));
            }
        }
        for sub in command.get_subcommands() {
            walk(sub, out);
        }
    }

    let mut out = BTreeSet::new();
    walk(&liyasa_cli::cli::Cli::command(), &mut out);
    // clap adds these itself when it renders help rather than declaring them.
    out.insert("--help".to_owned());
    out.insert("--version".to_owned());
    out
}

pub fn audit(root: &Path) -> Result<Vec<Phantom>, String> {
    let known = known_flags();
    let mut found = Vec::new();
    for file in sources(&root.join("crates"))? {
        let body = std::fs::read_to_string(&file).unwrap_or_default();
        for (at, line) in body.lines().enumerate() {
            if !reaches_a_human(line) {
                continue;
            }
            for flag in flags_in(line) {
                if FOREIGN.iter().any(|(name, _)| *name == flag) {
                    continue;
                }
                if !known.contains(&flag) {
                    found.push(Phantom {
                        flag,
                        file: file.clone(),
                        line: at + 1,
                        text: line.trim().to_owned(),
                    });
                }
            }
        }
    }
    Ok(found)
}

/// Entry point for `xtask flags`.
pub fn run(root: &Path) -> Result<(), String> {
    let phantoms = audit(root)?;
    println!("flags: {} known to the command tree", known_flags().len());
    for p in &phantoms {
        println!("  {}:{}: {}", p.file.display(), p.line, p.flag);
    }
    if phantoms.is_empty() {
        return Ok(());
    }
    Err(explain(&phantoms))
}

/// The failure message.
///
/// It names the tree it read. The first thing this check got wrong was its
/// diagnosis rather than its match: `--acme-domain` is a real flag of
/// `liyasa-server`, parsed five lines above the prose that named it, and the
/// message said "the CLI does not define it" and sent the reader off to build
/// it. A phantom-interface check producing a phantom diagnosis is the failure
/// it exists to prevent, pointed the other way.
pub fn explain(phantoms: &[Phantom]) -> String {
    let mut message = format!(
        "{} flag(s) named in prose that the `liyasa` command tree does not define.\n\
         That tree is `Cli::command()` in liyasa-cli and nothing else: the\n\
         `liyasa-server` and `liyasa-search` binaries parse their own argv, so a\n\
         flag of theirs is not a phantom and belongs in FOREIGN instead.\n",
        phantoms.len()
    );
    for p in phantoms {
        message.push_str(&format!(
            "  {}:{}: {}\n    {}\n",
            p.file.display(),
            p.line,
            p.flag,
            p.text
        ));
    }
    message.push_str(
        "Build the flag, fix the text, or — if it belongs to another tool or another \
         liyasa binary — add it to FOREIGN in xtask/src/flags.rs with a note saying whose.",
    );
    message
}

/// A doc comment, or the text of a diagnostic: the two places prose reaches
/// somebody who will act on it.
fn reaches_a_human(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("///")
        || trimmed.starts_with("//!")
        || ["Diagnostic::new(", ".help(", ".label(", ".note("]
            .iter()
            .any(|marker| line.contains(marker))
}

/// `--word-shaped` runs, minus the theme's CSS custom properties, which are all
/// namespaced `--ly-` and outnumber real flags three to one.
fn flags_in(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut at = 0usize;
    while let Some(start) = line[at..].find("--").map(|i| at + i) {
        at = start + 2;
        if start > 0 && (bytes[start - 1] == b'-' || bytes[start - 1].is_ascii_alphanumeric()) {
            continue;
        }
        let rest = &line[at..];
        let end = rest
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
            .unwrap_or(rest.len());
        let name = rest[..end].trim_end_matches('-');
        if name.len() >= 3 && !name.starts_with("ly-") {
            out.push(format!("--{name}"));
        }
    }
    out
}

fn sources(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let src = entry
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .path()
            .join("src");
        if src.is_dir() {
            walk(&src, &mut out)?;
        }
    }
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}
