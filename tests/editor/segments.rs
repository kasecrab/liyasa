//! The editor's TypeScript tests run against segmentations the real scanner
//! produced.
//!
//! `web/editor/` maps a `SourceDocument` to ED-01's node tree, and it cannot
//! call `liyasa_markdown::source::scan` — it runs in a browser, and its unit
//! suite runs under `node --test`. A test that wrote its own segmentation
//! would assert the mapping against a model the product does not use, which is
//! the shape of three defects this project has already recorded.
//!
//! So the scanner writes the fixture. This test scans every page under
//! `web/editor/test/fixtures/pages/` and holds the checked-in
//! `fixtures/segments.json` to what came out, rewriting it on drift the way
//! `liyasa-wasm`'s TypeScript declaration test does. The node suite reads that
//! file and nothing else.

use std::path::{Path, PathBuf};

use liyasa_core::span::SourceId;
use liyasa_markdown::scan;

fn editor() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/editor/test/fixtures")
}

fn pages(root: &Path) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = std::fs::read_dir(root.join("pages"))
        .expect("the fixture pages are readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .map(|path| {
            let name = path
                .file_name()
                .expect("a fixture page has a name")
                .to_string_lossy()
                .into_owned();
            (
                name,
                std::fs::read_to_string(&path).expect("a fixture page is readable"),
            )
        })
        .collect();
    found.sort_by(|left, right| left.0.cmp(&right.0));
    found
}

fn generated(root: &Path) -> String {
    let mut entries = Vec::new();
    for (name, source) in pages(root) {
        let (document, _) = scan(&source, SourceId(0));
        entries.push(serde_json::json!({
            "path": name,
            "source": source,
            "document": document,
        }));
    }
    let mut text = serde_json::to_string_pretty(&serde_json::Value::Array(entries))
        .expect("a source document serializes");
    text.push('\n');
    text
}

#[test]
fn the_checked_in_segmentation_is_what_the_scanner_produces() {
    let root = editor();
    let path = root.join("segments.json");
    blessed(
        &path,
        &generated(&root),
        "`liyasa_markdown::scan`",
        "editor_segments::the_checked_in_segmentation_is_what_the_scanner_produces",
    );
}

#[test]
fn the_fixture_covers_every_segment_kind_the_editor_models() {
    // A fixture that happens to hold only prose would let the mapping tests
    // pass while modelling nothing, and nothing would say so.
    let text = generated(&editor());
    for kind in [
        "markdown",
        "code",
        "template",
        "directiveOpen",
        "directiveClose",
        "directiveLeaf",
    ] {
        assert!(
            text.contains(&format!("\"segment\": \"{kind}\"")),
            "no fixture page produces a `{kind}` segment"
        );
    }
}

/// The fifty pages ED-12's find-and-replace runs over.
///
/// Each one puts the same phrase in four places the scanner segments
/// differently — prose, a directive prop, a fenced command, and a template
/// expression — because the point of ED-12's scope rules is that those four
/// are not the same thing. A fixture whose phrase appears only in prose would
/// let a replace that rewrites fenced commands pass.
fn bulk_pages() -> Vec<(String, String)> {
    (0..50)
        .map(|index| {
            let path = format!("guides/page-{index:02}.md");
            let source = format!(
                "---\ntitle: Page {index}\n---\n\n\
                 The Widget API is described here, page {index}.\n\n\
                 :::note{{title=\"The Widget API\"}}\n\
                 The Widget API changed in 2.0.\n\
                 :::\n\n\
                 ```bash\n\
                 curl https://example.invalid/Widget/API\n\
                 ```\n\n\
                 {{{{ site.name }}}} documents the Widget API.\n"
            );
            (path, source)
        })
        .collect()
}

fn generated_bulk() -> String {
    let mut entries = Vec::new();
    for (path, source) in bulk_pages() {
        let (document, _) = scan(&source, SourceId(0));
        entries.push(serde_json::json!({
            "path": path,
            "source": source,
            "document": document,
        }));
    }
    let mut text = serde_json::to_string_pretty(&serde_json::Value::Array(entries))
        .expect("a source document serializes");
    text.push('\n');
    text
}

#[test]
fn the_checked_in_bulk_corpus_is_what_the_scanner_produces() {
    let path = editor().join("bulk.json");
    blessed(
        &path,
        &generated_bulk(),
        "`liyasa_markdown::scan`",
        "editor_segments::the_checked_in_bulk_corpus_is_what_the_scanner_produces",
    );
}

/// Rewrites a generated file, but only when asked.
///
/// A test that repairs the tree it is checking leaves a modified tracked file
/// behind every gate that finds drift — which produced a false "main is RED"
/// on 2026-09-17 and is why `gate_commit` carries a reset.
/// `plan/rfcs/2433-a-pin-on-a-shared-append-only-file.md` records the rule: a
/// gate never sets `LIYASA_BLESS`, so a gate never writes.
fn blessed(path: &std::path::Path, fresh: &str, source: &str, test: &str) {
    let committed = std::fs::read_to_string(path).unwrap_or_default();
    if committed == fresh {
        return;
    }
    if std::env::var_os("LIYASA_BLESS").is_some() {
        std::fs::write(path, fresh).expect("the generated file is writable");
        panic!("{} was rewritten from {source}; commit it", path.display());
    }
    // The command is the test's own name, not the file's: a filter that names
    // the file matches nothing and sends the reader round again.
    panic!(
        "{} no longer matches {source}. It is generated; do not edit it by hand. Run:\n\n    \
         LIYASA_BLESS=1 cargo test -p liyasa-tests --test it -- {test}\n\n\
         and commit what it writes.",
        path.display(),
    );
}
