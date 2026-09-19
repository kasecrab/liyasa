//! CI that names a package or a binary the workspace does not have.
//!
//! The same defect as `flags`, one directory over. A workflow step is prose
//! until somebody pushes: `cargo run -p liyasa-bench` and `--bin liyasa-cli`
//! are both plausible and both wrong, and the branch that renames a package
//! turns a job red on `main` rather than in the pull request that caused it.
//!
//! Scope is deliberately two things. A workflow line is shell, and reading it
//! as anything more would mean a shell parser; `-p` and `--bin` are the two
//! arguments that name something the workspace owns, so they are the two that
//! can be checked against it. A value that is a GitHub expression is left
//! alone — its content is not known until the run.
//!
//! **`-p` has to be cargo's `-p`.** The first run of this check reported
//! `mkdir -p corpus` in `ci.yml` as a package named `corpus`, which is the
//! failure that gets a lint switched off rather than fixed: `-p` is also
//! mkdir's, cp's and grep's. So a flag counts only on a line that invokes
//! cargo. The cost is that `cargo run -p a && mkdir -p b` on one line would
//! still be misread; nothing in `.github/` writes that, and a shell parser to
//! rule it out would be a larger thing than the check.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Package,
    Bin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unknown {
    pub kind: Kind,
    pub name: String,
    pub file: PathBuf,
    pub line: usize,
}

/// Every workspace package, with the binaries it produces.
pub fn members(root: &Path) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    Ok(workspace::members(root)?
        .into_iter()
        .map(|member| (member.name.clone(), member.bins()))
        .collect())
}

/// Names in `.github/` that no workspace member answers to.
pub fn audit(root: &Path) -> Result<Vec<Unknown>, String> {
    let members = members(root)?;
    let bins: BTreeSet<String> = members.values().flatten().cloned().collect();

    let directory = root.join(".github/workflows");
    let read =
        std::fs::read_dir(&directory).map_err(|e| format!("{}: {e}", directory.display()))?;
    let mut files: Vec<PathBuf> = read
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "yml" || e == "yaml"))
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(format!("{}: no workflows", directory.display()));
    }

    let mut out = Vec::new();
    for file in files {
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        for (at, line) in text.lines().enumerate() {
            for (flag, kind) in [("-p", Kind::Package), ("--bin", Kind::Bin)] {
                for name in values(line, flag) {
                    let known = match kind {
                        Kind::Package => members.contains_key(name),
                        Kind::Bin => bins.contains(name),
                    };
                    if !known {
                        out.push(Unknown {
                            kind,
                            name: name.to_owned(),
                            file: file.clone(),
                            line: at + 1,
                        });
                    }
                }
            }
        }
    }
    Ok(out)
}

/// The values `flag` is given on one line, when cargo is what is being run.
///
/// A value that is a GitHub expression, another flag, or empty is not a name
/// this can check, and `-p` inside a longer word (`--strip`) is not the flag.
pub fn values<'a>(line: &'a str, flag: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let quotes = |c: char| c == '"' || c == '\'';
    let mut cargo = false;
    let mut words = line.split_whitespace().map(|w| w.trim_matches(quotes));
    while let Some(word) = words.next() {
        // `cargo`, or a path ending in it, which is how rustup's shim appears.
        if word == "cargo" || word.ends_with("/cargo") {
            cargo = true;
            continue;
        }
        if !cargo || word != flag {
            continue;
        }
        let Some(value) = words.next() else { continue };
        if value.is_empty() || value.starts_with('-') || value.contains("${{") {
            continue;
        }
        out.push(value);
    }
    out
}

/// Prints what CI names that the workspace does not have.
pub fn run(root: &Path) -> Result<(), String> {
    let members = members(root)?;
    println!("{} workspace members", members.len());
    let unknown = audit(root)?;
    if unknown.is_empty() {
        println!("workflows: every -p and --bin names something the workspace has");
        return Ok(());
    }
    for entry in &unknown {
        let what = match entry.kind {
            Kind::Package => "package",
            Kind::Bin => "binary",
        };
        println!(
            "  {}:{}: no {what} `{}`",
            entry.file.display(),
            entry.line,
            entry.name
        );
    }
    Err(format!(
        "{} unknown names in .github/workflows",
        unknown.len()
    ))
}
