//! ED-73's acceptance test.
//!
//! > Given each validation code that can reach the editor; when shown; then
//! > its message is the plain-language variant from `codes.toml` and a fix
//! > action exists where the registry marks one.
//!
//! **There is no plain-language variant in `codes.toml` and no fix marker.** A
//! row is `{ severity, crate, title }` and nothing else, across all 198 of
//! them, and the file is append-only for every package — adding a field to an
//! existing row is editing a line somebody else owns.
//! `plan/rfcs/2432-plain-language-messages-live-in-the-editor.md` records the
//! reading that follows: the plain-language text is the editor's, keyed by
//! code, and this test is what keeps it from drifting.
//!
//! It writes `web/editor/src/code-list.ts` — every code, its severity, its
//! crate and its title — and fails when the checked-in copy is stale. The
//! editor's own suite then asserts that every editor-reachable code has a
//! message and that no message names a code the registry does not have.

use std::collections::BTreeMap;
use std::path::PathBuf;

use liyasa_core::diagnostics::registry;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/editor/src/code-list.ts")
}

/// The crates whose diagnostics the WebAssembly module can raise in a browser.
///
/// WP-24a's `state/wp-24a/NOTES.md` lists what `liyasa-wasm` carries: the
/// scanner, expansion, the directive parser, the components, the sanitizer,
/// config and front matter validation and the `liyasa-idx` reader. A code from
/// `liyasa-build` or `liyasa-server` reaches an author through a *build*, not
/// through the editor's own validator, and is shown with the build's own
/// wording.
/// `liyasa-core` is not here: it owns the registry and raises nothing itself,
/// and `every_editor_crate_actually_raises_codes` caught it in this list.
const EDITOR_CRATES: &[&str] = &[
    "liyasa-config",
    "liyasa-markdown",
    "liyasa-components",
    "liyasa-search",
    "liyasa-wasm",
];

fn generated() -> String {
    let mut rows: BTreeMap<String, (String, String, String)> = BTreeMap::new();
    for entry in registry() {
        rows.insert(
            entry.code.to_string(),
            (
                format!("{:?}", entry.severity).to_lowercase(),
                entry.krate.to_owned(),
                entry.title.to_owned(),
            ),
        );
    }

    let mut out = String::new();
    out.push_str(
        "// Generated from `crates/liyasa-core/src/diagnostics/codes.toml` by\n\
         // `tests/editor/ed_73_messages.rs`. Do not edit: that test rewrites it and\n\
         // fails when this file and the registry disagree.\n\n",
    );
    out.push_str("export interface CodeEntry {\n  code: string;\n  severity: string;\n  crate: string;\n  title: string;\n}\n\n");
    out.push_str("/** The crates whose codes the editor's own validator can raise. */\nexport const EDITOR_CRATES: string[] = [\n");
    for krate in EDITOR_CRATES {
        out.push_str(&format!("  \"{krate}\",\n"));
    }
    out.push_str("];\n\nexport const CODES: CodeEntry[] = [\n");
    for (code, (severity, krate, title)) in &rows {
        out.push_str(&format!(
            "  {{ code: \"{code}\", severity: \"{severity}\", crate: \"{krate}\", title: {} }},\n",
            json_string(title)
        ));
    }
    out.push_str("];\n");
    out
}

/// A TypeScript string literal for `text`, with nothing in it that could end it.
fn json_string(text: &str) -> String {
    serde_json::to_string(text).expect("a title is a string")
}

#[test]
fn the_checked_in_code_list_is_the_registry() {
    let path = fixture();
    let fresh = generated();
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    if committed != fresh {
        let _ = std::fs::write(&path, &fresh);
        panic!(
            "{} did not match the code registry; it has been rewritten, commit it \
             with the change that moved it",
            path.display()
        );
    }
}

#[test]
fn the_registry_still_has_no_plain_language_field() {
    // The reading in RFC 2432 rests on this. If a `plain` or `fix` key is ever
    // added to `codes.toml`, ED-73's own mechanism exists and the editor's
    // table should move into it — so this fails, loudly, rather than leaving a
    // duplicate table nobody notices.
    let source = include_str!("../../crates/liyasa-core/src/diagnostics/codes.toml");
    for field in ["plain =", "plain=", "fix =", "fix=", "friendly ="] {
        assert!(
            !source.contains(field),
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
            registry().iter().any(|entry| entry.krate == *krate),
            "`{krate}` is in EDITOR_CRATES and owns no code"
        );
    }
}
