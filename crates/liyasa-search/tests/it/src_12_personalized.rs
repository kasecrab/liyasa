//! SRC-12: nothing a reader sees about themselves reaches a shared index.
//!
//! §6.6.4 says every indexing pipeline uses the **anonymous render**, where
//! `reader.*` is undefined. The build is responsible for handing over that
//! render; this package is responsible for never carrying a reader value
//! through even when it is handed one — an unexpanded `{{ reader.plan }}` is
//! markup, not content, and a fixture value that slipped into a prop is not
//! prose either.

use super::support;

use liyasa_core::document::{Inline, Node, PropValue, Props};
use liyasa_core::ids::{Locale, Route};
use liyasa_core::span::{SourceId, Span};
use liyasa_search::doc::{DocKind, PageMeta};
use liyasa_search::idx::writer::{self, WriterOptions};
use liyasa_search::section;
use support::{block, component, document, para, text};

/// What a reader-bearing render would have put on the page.
const FIXTURE_VALUES: [&str; 4] = [
    "Ada Lovelace",
    "Analytical Engine Ltd",
    "plan_enterprise",
    "sk_live_fixture",
];
const TEMPLATE_MARKER: &str = "{{ reader.";

fn personalized_page() -> liyasa_core::document::Document {
    let mut props = Props::default();
    props
        .0
        .insert("title".to_owned(), PropValue::Str("Your plan".to_owned()));
    // An expression the expansion pass left alone, as a Source Document
    // fragment carries it.
    props.0.insert(
        "plan".to_owned(),
        PropValue::Expr("{{ reader.plan }}".to_owned()),
    );

    document(vec![
        para("Every account has a quota."),
        Node::Inline(Inline::TemplateInline {
            expr: "reader.organization".to_owned(),
            origin: Span {
                source: SourceId(0),
                start: 0,
                end: 0,
            },
        }),
        component("Callout", props, vec![para("Contact support to raise it.")]),
        block(
            liyasa_core::document::BlockKind::HtmlBlock {
                html: format!("<span data-reader=\"{}\">hidden</span>", FIXTURE_VALUES[0]),
            },
            vec![text(FIXTURE_VALUES[0])],
        ),
    ])
}

fn meta() -> PageMeta {
    let mut meta = PageMeta::new(Route::new("/account/plan"), "Your plan", Locale::new("en"));
    meta.kind = DocKind::Page;
    meta
}

#[test]
fn a_section_document_carries_no_reader_value() {
    let documents = section::extract(&personalized_page(), &meta());
    let json = serde_json::to_string(&documents).expect("serializes");
    assert!(
        !json.contains(TEMPLATE_MARKER) && !json.contains("reader."),
        "an unexpanded reader expression reached a section document: {json}"
    );
    for value in FIXTURE_VALUES {
        assert!(
            !json.contains(value),
            "`{value}` reached a section document"
        );
    }
    assert!(
        documents[0].body.contains("Every account has a quota."),
        "the anonymous prose is still indexed: {}",
        documents[0].body
    );
    assert!(
        documents[0].body.contains("Your plan"),
        "a literal prop is prose"
    );
}

#[test]
fn no_index_file_holds_a_reader_value() {
    let mut documents = section::extract(&personalized_page(), &meta());
    documents.extend(support::corpus::documents());
    let built = writer::build(&documents, &WriterOptions::default());

    for (name, bytes) in &built.files {
        let text = String::from_utf8_lossy(bytes);
        assert!(
            !text.contains(TEMPLATE_MARKER),
            "`{name}` holds an unexpanded reader expression"
        );
        assert!(
            !text.contains("reader."),
            "`{name}` holds a reader expression"
        );
        for value in FIXTURE_VALUES {
            assert!(!text.contains(value), "`{name}` holds `{value}`");
        }
    }
}

#[test]
fn the_snippet_blob_is_the_one_src_12_names() {
    // SRC-12 asserts against `snippets.bin`; the shard suffix is
    // plan/rfcs/0702-idx-file-layout.md's.
    let documents = section::extract(&personalized_page(), &meta());
    let built = writer::build(&documents, &WriterOptions::default());
    let snippets: Vec<&String> = built
        .files
        .keys()
        .filter(|name| name.starts_with("snippets-") && name.ends_with(".bin"))
        .collect();
    assert!(!snippets.is_empty(), "the assertion has a file to make");
    for name in snippets {
        let text = String::from_utf8_lossy(&built.files[name]);
        for value in FIXTURE_VALUES {
            assert!(!text.contains(value), "`{name}` holds `{value}`");
        }
    }
}

#[cfg(feature = "server")]
#[test]
fn the_tantivy_index_holds_no_reader_value() {
    use liyasa_search::server::ServerIndex;

    let directory = std::env::temp_dir().join(format!(
        "liyasa-src-12-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&directory).expect("a temporary directory");

    let documents = section::extract(&personalized_page(), &meta());
    let mut index = ServerIndex::create_in_dir(&directory).expect("creates");
    index.index_all(&documents).expect("indexes");

    let mut checked = 0;
    for entry in std::fs::read_dir(&directory).expect("reads") {
        let entry = entry.expect("an entry");
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let bytes = std::fs::read(entry.path()).expect("reads");
        let text = String::from_utf8_lossy(&bytes);
        for value in FIXTURE_VALUES {
            assert!(
                !text.contains(value),
                "`{}` holds `{value}`",
                entry.file_name().to_string_lossy()
            );
        }
        assert!(
            !text.contains(TEMPLATE_MARKER),
            "`{}` holds an unexpanded reader expression",
            entry.file_name().to_string_lossy()
        );
        checked += 1;
    }
    assert!(checked > 0, "the index wrote something to assert against");
    let _ = std::fs::remove_dir_all(&directory);
}
