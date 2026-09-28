//! Every registration in `routes/work.rs` is one line that union can merge.
//!
//! On 2026-09-28 a union merge interleaved two packages' multi-line `JobKind`
//! literals in that file and swallowed a `},`. The file had 55 `{` against
//! 54 `}`, so every branch chained under it died at the gate's FORMAT step in
//! two seconds — seven packages looked red for one missing brace (defect 192).
//!
//! The rule that followed — an appended entry occupies exactly one line — is
//! not self-enforcing, and a literal reading of it does not even work: a
//! three-field `JobKind` literal is 116 columns against `max_width = 100`, so
//! rustfmt puts the forbidden shape back. What works is a `pub const` in the
//! registering package's own module, named here by one short line. This test
//! is what keeps that true after everyone who remembers has moved on.

use std::path::PathBuf;

fn work_rs() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/liyasa-server/src/routes/work.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// The body of `kinds()`, which is the union-merged region that matters.
fn registry_body(source: &str) -> String {
    let start = source
        .find("pub fn kinds()")
        .expect("`kinds()` is not in work.rs any more; this test measures nothing");
    let open = source[start..]
        .find("&[")
        .expect("`kinds()` no longer returns a slice literal");
    let from = start + open + 2;
    let close = source[from..]
        .find("    ]")
        .expect("the slice literal is not closed where this test can see it");
    source[from..from + close].to_owned()
}

#[test]
fn every_registration_is_one_line() {
    let source = work_rs();
    let body = registry_body(&source);

    let entries: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .collect();
    assert!(
        entries.len() >= 4,
        "only {} entries parsed out of `kinds()`; the parser has stopped matching and an \
         empty result would read as a clean bill of health",
        entries.len()
    );

    for entry in entries {
        assert!(
            entry.ends_with(','),
            "`{entry}` does not end a registration. An entry that spans lines is what a \
             union merge interleaves into something `cargo fmt` cannot parse (defect 192): \
             write a `pub const JobKind` in your own module and name it here."
        );
        assert!(
            !entry.contains('{'),
            "`{entry}` opens a brace. A struct literal in this list is the exact shape that \
             cost eight branches their merge: put it in your own module as a `pub const` and \
             leave one short name here."
        );
    }
}

/// The rule is "one line", and rustfmt decides what one line may hold. An
/// entry over the limit is rewrapped into the multi-line form on the next
/// `cargo fmt`, so the two constraints have to be checked together or the
/// first is satisfied only until somebody formats.
#[test]
fn no_line_in_the_registry_exceeds_the_formatter_width() {
    let source = work_rs();
    let width: usize =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../rustfmt.toml"))
            .ok()
            .and_then(|text| {
                text.lines()
                    .find_map(|line| line.strip_prefix("max_width = ")?.trim().parse().ok())
            })
            .expect(
                "rustfmt.toml sets max_width; without it this test asserts a number it invented",
            );

    for (number, line) in source.lines().enumerate() {
        assert!(
            line.chars().count() <= width,
            "work.rs:{} is {} columns against max_width {width}. rustfmt will rewrap it, and \
             in this file a rewrap means a multi-line entry.",
            number + 1,
            line.chars().count()
        );
    }
}
