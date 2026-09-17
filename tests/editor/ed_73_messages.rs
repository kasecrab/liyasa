//! ED-73's premise, asserted against the registry itself.
//!
//! > Given each validation code that can reach the editor; when shown; then
//! > its message is the plain-language variant from `codes.toml` and a fix
//! > action exists where the registry marks one.
//!
//! **There is no plain-language variant in `codes.toml` and no fix marker.** A
//! row is `{ severity, crate, title }` and nothing else, across all of them,
//! and the file is append-only for every package — adding a field to an
//! existing row is editing a line somebody else owns.
//! `plan/rfcs/2432-plain-language-messages-live-in-the-editor.md` records the
//! reading that follows: the plain-language text is the editor's, keyed by
//! code, in `web/editor/src/messages.ts`.
//!
//! This file used to *generate* `web/editor/src/code-list.ts` and pin it.
//! `plan/rfcs/2433-a-pin-on-a-shared-append-only-file.md` records why that was
//! wrong: `codes.toml` is shared and append-only, so the pin went stale
//! whenever any other package merged a code — it took this branch red in
//! `bin/integrate` on eight rows nobody here wrote, and in a chain the bisect
//! would have convicted the branch that added them. The editor's own suite now
//! reads `codes.toml` directly and nothing is generated.
//!
//! What is left here are the assertions that are about the registry rather
//! than about a file.

/// The crates whose diagnostics the WebAssembly module can raise in a browser.
///
/// `liyasa-core` is deliberately absent: it owns the registry and raises
/// nothing itself, which `every_editor_crate_actually_raises_codes` caught
/// when an earlier version of this list included it.
const EDITOR_CRATES: &[&str] = &[
    "liyasa-config",
    "liyasa-markdown",
    "liyasa-components",
    "liyasa-search",
    "liyasa-wasm",
];

const REGISTRY: &str = include_str!("../../crates/liyasa-core/src/diagnostics/codes.toml");
const SUITE: &str = include_str!("../../web/editor/test/messages.test.ts");

#[test]
fn the_registry_still_has_no_plain_language_field() {
    // RFC 2432's premise. If a `plain` or `fix` key is ever added to
    // `codes.toml`, ED-73's own mechanism exists and the editor's table should
    // move into it — so this fails, loudly, rather than leaving a duplicate
    // table nobody notices.
    for field in ["plain =", "plain=", "fix =", "fix=", "friendly ="] {
        assert!(
            !REGISTRY.contains(field),
            "`codes.toml` now has a `{field}` field; ED-73's plain-language text belongs there, \
             and `web/editor/src/messages.ts` should be moved into it (RFC 2432)"
        );
    }
}

#[test]
fn every_editor_crate_actually_raises_codes() {
    // A list naming a crate with no codes would silently shrink what ED-73
    // covers while every assertion about coverage still passed.
    for krate in EDITOR_CRATES {
        assert!(
            liyasa_core::diagnostics::registry()
                .iter()
                .any(|entry| entry.krate == *krate),
            "`{krate}` is in EDITOR_CRATES and owns no code"
        );
    }
}

#[test]
fn the_editor_suite_names_the_same_crates() {
    // The list exists twice — here, and in TypeScript, which cannot read this
    // file. Two copies with nothing holding them together is how a crate
    // quietly drops out of ED-73's coverage on one side only.
    let start = SUITE
        .find("const EDITOR_CRATES = [")
        .expect("the editor's suite declares EDITOR_CRATES");
    let end = SUITE[start..].find("];").expect("the list is closed") + start;
    let declared = &SUITE[start..end];

    for krate in EDITOR_CRATES {
        assert!(
            declared.contains(&format!("\"{krate}\"")),
            "`{krate}` is missing from the editor suite's EDITOR_CRATES"
        );
    }
    assert_eq!(
        declared.matches('"').count() / 2,
        EDITOR_CRATES.len(),
        "the editor suite lists a different number of crates from this file"
    );
}

#[test]
fn the_editor_suite_reads_the_registry_rather_than_a_copy_of_it() {
    // The repair RFC 2433 describes, asserted rather than trusted: if somebody
    // reintroduces a generated code list, this says why not.
    assert!(
        SUITE.contains("diagnostics/codes.toml"),
        "the editor's suite should read `codes.toml` itself (RFC 2433)"
    );
    assert!(
        !SUITE.contains("code-list.ts"),
        "a generated code list is a pin on a shared append-only file (RFC 2433)"
    );
}

/// The codes `web/editor/test/messages.test.ts` would find in `text`.
///
/// This mirrors that file's `ROW` regular expression —
/// `^([EW]\d{4})\s*=\s*\{([^}]*)\}\s*$` — by hand, because the agreement
/// between the two parsers is the thing being asserted and a *looser* check
/// here would happily pass a file the TypeScript silently misses.
///
/// The first version of this check compared only the text before the first
/// ` = ` on each line. That passes a row split across two lines, which the
/// TypeScript cannot read at all — one such row would have dropped a code out
/// of ED-73's coverage with nothing to say so. The coordinator asked whether
/// the check had been measured or only reasoned about; it had only been
/// reasoned about, and it was wrong.
fn codes_a_line_parser_finds(text: &str) -> Vec<&str> {
    text.lines().filter_map(row_code).collect()
}

fn row_code(line: &str) -> Option<&str> {
    let code = line.get(..5)?;
    let mut characters = code.chars();
    if !matches!(characters.next()?, 'E' | 'W') {
        return None;
    }
    if !characters.all(|character| character.is_ascii_digit()) {
        return None;
    }
    let rest = line[5..].trim_start().strip_prefix('=')?.trim_start();
    // `[^}]*` cannot span a brace, so the row ends at the first one.
    let (_, after) = rest.strip_prefix('{')?.split_once('}')?;
    after.trim().is_empty().then_some(code)
}

#[test]
fn every_registered_code_is_on_a_line_the_editor_suite_can_parse() {
    // The editor's suite parses `codes.toml` with a regular expression, which
    // is only safe because the file is one row per line — its own header says
    // so, because `merge=union` requires it. This is what makes that safe
    // rather than lucky.
    let found = codes_a_line_parser_finds(REGISTRY);
    let registered = liyasa_core::diagnostics::registry().len();
    assert_eq!(
        found.len(),
        registered,
        "{} of {registered} registered codes are on a line `web/editor/test/messages.test.ts` \
         can parse; the rest would drop out of ED-73's coverage silently",
        found.len(),
    );
}

#[test]
fn the_line_parser_misses_exactly_the_shapes_the_typescript_misses() {
    // The other half of the assertion above, and the half that was missing:
    // a check over a format nobody has broken proves nothing until it is shown
    // to fail on a format that does break it.
    assert_eq!(
        codes_a_line_parser_finds("E0001 = { severity = \"error\", crate = \"liyasa-cli\" }\n"),
        vec!["E0001"],
        "the positive control: a well-formed row is found, so a parser that found \
         nothing at all could not pass the cases below"
    );

    for (what, text) in [
        (
            "a row split across lines",
            "E0001 = {\n  severity = \"error\",\n}\n",
        ),
        ("an indented row", "  E0001 = { severity = \"error\" }\n"),
        (
            "two rows on one line",
            "E0001 = { a = 1 } E0002 = { b = 2 }\n",
        ),
        ("a trailing comment", "E0001 = { a = 1 } # a note\n"),
        ("a row with no braces", "E0001 = \"error\"\n"),
    ] {
        assert!(
            codes_a_line_parser_finds(text).is_empty(),
            "{what} was parsed as a row, so the check above would pass a `codes.toml` \
             the editor's suite cannot read"
        );
    }
}
