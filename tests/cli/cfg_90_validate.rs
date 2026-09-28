//! CFG-90: `liyasa validate --format json` reports the semantic rules with
//! their codes and their spans.
//!
//! Two halves, and both are here. The command's own body is
//! `printer.emit(&diagnostics, &sources)`, which is `render` and a `println!`,
//! so asserting `Printer::render` with `Format::Json` asserts the bytes the
//! command prints. The exit code a shell sees belongs to
//! `crates/liyasa-cli/tests/it/`, which can spawn a process.

use liyasa_cli::cli::Format;
use liyasa_cli::diag::Printer;
use liyasa_config::vfs::MemVfs;
use liyasa_config::{Mode, Options, check};
use liyasa_core::source_map::SourceMap;
use serde_json::Value;

/// One project breaking a rule of each kind that can coexist: a page named
/// twice, a primary no label clears, two versions and no default, a subtree
/// bound to a version nobody declared, a colour that is not a colour, and a
/// page no navigation reaches.
const BROKEN: &str = r##"{
  "name": "Acme docs",
  "seo": { "canonicalOrigin": "https://docs.acme.com" },
  "theme": { "colors": { "primary": "#818CF8", "accent": "#ggg" } },
  "versions": [{ "name": "v2" }, { "name": "v1" }],
  "navigation": ["index", "index", { "version": "v9", "pages": ["guides/install"] }]
}"##;

const PAGES: &[(&str, &str)] = &[
    ("index.md", "---\ntitle: Home\n---\n# Home\n"),
    ("guides/install.md", "---\ntitle: Install\n---\n# Install\n"),
    ("guides/orphan.md", "---\ntitle: Orphan\n---\n# Orphan\n"),
];

/// What `liyasa validate` computes, and the sources it renders against.
fn validated() -> (liyasa_config::Checked, SourceMap) {
    let mut files: Vec<(&str, Vec<u8>)> = vec![("liyasa.json", BROKEN.as_bytes().to_vec())];
    files.extend(
        PAGES
            .iter()
            .map(|(path, text)| (*path, text.as_bytes().to_vec())),
    );
    let vfs: MemVfs = files.into_iter().collect();
    let mut sources = SourceMap::new();
    let checked = check(&vfs, &mut sources, &Options::default(), Mode::Build);
    (checked, sources)
}

fn json_document() -> Value {
    let (checked, sources) = validated();
    let rendered = Printer::new(Format::Json, false).render(&checked.diagnostics, &sources);
    serde_json::from_str(&rendered).expect("`--format json` prints one JSON document")
}

fn diagnostics_of(document: &Value) -> Vec<&Value> {
    document
        .get("diagnostics")
        .and_then(Value::as_array)
        .map(|entries| entries.iter().collect())
        .unwrap_or_default()
}

#[test]
fn every_semantic_rule_reaches_the_json_document() {
    let document = json_document();
    let codes: Vec<&str> = diagnostics_of(&document)
        .iter()
        .filter_map(|entry| entry.get("code").and_then(Value::as_str))
        .collect();
    for code in ["E0105", "E0107", "E0108", "E0132", "E0133", "W0130"] {
        assert!(codes.contains(&code), "{code} is not among {codes:?}");
    }
}

#[test]
fn each_one_carries_a_span_into_the_file_it_came_from() {
    let document = json_document();
    for entry in diagnostics_of(&document) {
        let code = entry.get("code").and_then(Value::as_str).unwrap_or("?");
        if code == "W0130" {
            // The page nothing navigates to is not written anywhere in
            // `liyasa.json`, so there is no span to carry.
            continue;
        }
        assert!(
            entry
                .get("file")
                .and_then(Value::as_str)
                .is_some_and(|file| file.ends_with("liyasa.json")),
            "{code} points somewhere other than the config: {entry}"
        );
        let span = entry
            .get("span")
            .unwrap_or_else(|| panic!("{code} has no span: {entry}"));
        for key in ["line", "column", "endLine", "endColumn"] {
            assert!(
                span.get(key).and_then(Value::as_u64).is_some(),
                "{code}'s span has no {key}: {span}"
            );
        }
        assert!(
            entry
                .get("url")
                .and_then(Value::as_str)
                .is_some_and(|url| url.ends_with(code)),
            "{code} does not link to its own page"
        );
    }
}

#[test]
fn a_clean_project_renders_an_empty_document_rather_than_nothing() {
    let vfs: MemVfs = [
        (
            "liyasa.json",
            br#"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.dev" },
                 "navigation": ["index"] }"#
                .to_vec(),
        ),
        ("index.md", b"---\ntitle: Home\n---\n# Home\n".to_vec()),
    ]
    .into_iter()
    .collect();
    let mut sources = SourceMap::new();
    let checked = check(&vfs, &mut sources, &Options::default(), Mode::Build);
    assert!(!checked.has_errors());

    let rendered = Printer::new(Format::Json, false).render(&checked.diagnostics, &sources);
    let document: Value = serde_json::from_str(&rendered).expect("still one JSON document");
    assert_eq!(diagnostics_of(&document).len(), 0);
}
