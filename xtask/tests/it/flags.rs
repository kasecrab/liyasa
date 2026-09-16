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
        "prose names {fresh:?}, which the CLI does not define.\n\
         Build the flag, fix the text, or — if it belongs to another tool — add it to \
         FOREIGN in xtask/src/flags.rs with a note saying whose."
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
