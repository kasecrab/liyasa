//! Prose must not name a flag the CLI does not have (WP-29's finding).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use xtask::flags::{self, FOREIGN};
use xtask::pins;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn the_command_tree_is_readable_and_not_empty() {
    let known = flags::known_flags();
    assert!(
        known.len() > 40,
        "only {} flags came out of Cli::command(); the walk is broken, not the CLI",
        known.len()
    );
    for expected in ["--config", "--json", "--drafts", "--locked", "--help"] {
        assert!(
            known.contains(expected),
            "the command tree is missing {expected}"
        );
    }
}

#[test]
fn no_prose_names_a_flag_the_pin_file_does_not_know_about() {
    let found: BTreeSet<String> = flags::audit(&root())
        .expect("the scan runs")
        .into_iter()
        .map(|p| p.flag)
        .collect();
    let pinned = pins::read(&root(), pins::PHANTOM_FLAGS).expect("the pin file parses");

    let fresh: Vec<&String> = found.difference(&pinned).collect();
    assert!(
        fresh.is_empty(),
        "prose names {fresh:?}, which the `liyasa` command tree does not define \
         (that tree is `Cli::command()` in liyasa-cli, and liyasa-server and \
         liyasa-search have their own).\n\
         Build the flag, fix the text, or — if it belongs to another tool or another \
         liyasa binary — add it to FOREIGN in xtask/src/flags.rs with a note saying whose."
    );
}

#[test]
fn every_foreign_entry_says_whose_flag_it_is() {
    for (flag, why) in FOREIGN {
        assert!(flag.starts_with("--"), "{flag} is not flag-shaped");
        assert!(
            why.len() > 15,
            "{flag} is allowed with only {why:?}; an allow-list without a reason rots"
        );
    }
}

/// Defect 234: the message was the defect, not the match. `--acme-domain` is a
/// real flag of `liyasa-server`, and the check told the reader "the CLI does not
/// define it" — sending them to build something that already existed five lines
/// above the prose that named it. A check that reads one command tree has to say
/// which one, or its own diagnosis is the phantom.
#[test]
fn the_failure_message_names_the_tree_it_checked_against() {
    let message = flags::explain(&[flags::Phantom {
        flag: "--acme-domain".to_owned(),
        file: PathBuf::from("crates/liyasa-server/src/main.rs"),
        line: 42,
        text: "/// pass --acme-domain to obtain a certificate".to_owned(),
    }]);

    assert!(
        message.contains("Cli::command()") && message.contains("liyasa-cli"),
        "the message does not say which tree was read:\n{message}"
    );
    for sibling in ["liyasa-server", "liyasa-search"] {
        assert!(
            message.contains(sibling),
            "the message does not mention that {sibling} has its own tree:\n{message}"
        );
    }
    assert!(
        message.contains("FOREIGN"),
        "the message does not name the table that resolves this:\n{message}"
    );
}

/// A FOREIGN note that blames a `liyasa-*` binary is a checkable claim, not a
/// promise. Three rows were owed for the ACME flags and would have gone in on
/// trust; this is what makes them evidence instead. The two notes that name
/// cargo and a CSS custom property are not checkable and are not checked.
#[test]
fn a_foreign_entry_blamed_on_a_liyasa_binary_is_parsed_by_that_binary() {
    let mut checked = 0;
    for (flag, why) in FOREIGN {
        let Some(crate_name) = liyasa_crate_named_in(why) else {
            continue;
        };
        let dir = root().join("crates").join(&crate_name).join("src");
        assert!(
            dir.is_dir(),
            "{flag} is blamed on {crate_name}, which is not a crate in this workspace"
        );
        let needle = format!("\"{flag}\"");
        assert!(
            rust_sources(&dir).iter().any(|file| {
                std::fs::read_to_string(file)
                    .unwrap_or_default()
                    .contains(&needle)
            }),
            "{flag} is allowed because it belongs to {crate_name}, but nothing under \
             {} matches {needle} — either the note is wrong or the flag was removed",
            dir.display()
        );
        checked += 1;
    }
    assert!(
        checked >= 2,
        "no FOREIGN note named a liyasa binary, so this test asserted nothing"
    );
}

/// The `liyasa-<word>` crate a FOREIGN note blames, if it blames one.
fn liyasa_crate_named_in(why: &str) -> Option<String> {
    let at = why.find("liyasa-")?;
    let rest = &why[at..];
    let end = rest
        .find(|c: char| !(c.is_ascii_lowercase() || c == '-'))
        .unwrap_or(rest.len());
    Some(rest[..end].trim_end_matches('-').to_owned())
}

fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    out
}
