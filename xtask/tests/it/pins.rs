//! The two pinned sets are current (RFC 0008).
//!
//! Both should only shrink. The data lives in `tests/pins/`, which
//! `bin/path-guard` lets every package write, so whoever fixes a flag or makes
//! a code emit lowers the pin in the same commit rather than leaving it for
//! WP-00 to chase.

use std::path::{Path, PathBuf};

use xtask::pins;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn the_pinned_sets_match_the_tree() {
    if let Err(report) = pins::run(&root(), false) {
        panic!("{report}");
    }
}

#[test]
fn the_pin_files_carry_their_own_instructions() {
    // A bare list of codes teaches nobody what to do when the test fails.
    for file in [pins::UNRAISED_CODES, pins::PHANTOM_FLAGS] {
        let text = std::fs::read_to_string(root().join(file)).expect("the pin file is readable");
        assert!(
            text.contains("pins --update"),
            "{file} does not say how to regenerate itself"
        );
        assert!(
            text.contains("DELETE"),
            "{file} does not say that entries come out when the defect is fixed"
        );
    }
}

#[test]
fn a_pin_file_holds_only_entries() {
    for file in [pins::UNRAISED_CODES, pins::PHANTOM_FLAGS] {
        for entry in pins::read(&root(), file).expect("the pin file parses") {
            assert!(
                !entry.contains(char::is_whitespace),
                "{file}: {entry:?} is not a bare entry"
            );
        }
    }
}
