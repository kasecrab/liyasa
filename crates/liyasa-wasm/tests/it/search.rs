//! The browser search worker over `liyasa-idx` shard bytes (SRC-05).
//!
//! The corpus is built by parsing pages through [`Session`] and indexing the
//! Rendered AST, so the two halves of this crate are tested against each other
//! rather than against a fixture that could drift from either.

use std::collections::BTreeMap;

use liyasa_core::ids::{Locale, Route};
use liyasa_search::doc::{DocKind, PageMeta};
use liyasa_search::idx::writer::{self, MANIFEST, WriterOptions};
use liyasa_search::section;
use liyasa_wasm::api::{OpenRequest, ParseRequest, SearchRequest, SiteMeta};
use liyasa_wasm::search::Searcher;
use liyasa_wasm::session::Session;

const NONCE: &str = "0123456789abcdef0123456789abcdef";

const PAGES: &[(&str, &str, &str)] = &[
    (
        "/install",
        "Install",
        "# Install\n\nRun the installer and restart the daemon.\n",
    ),
    (
        "/upgrade",
        "Upgrade",
        "# Upgrade\n\nBack up the database before you upgrade.\n",
    ),
];

fn session() -> Session {
    Session::sealed(&OpenRequest {
        nonce: NONCE.to_owned(),
        site: SiteMeta {
            name: "Acme docs".to_owned(),
            canonical_origin: "https://docs.acme.com".to_owned(),
            llms_txt: "https://docs.acme.com/llms.txt".to_owned(),
            version: None,
            locale: "en".to_owned(),
        },
        seed: Vec::new(),
    })
    .expect("the request is complete")
}

fn files() -> BTreeMap<String, Vec<u8>> {
    let session = session();
    let mut documents = Vec::new();
    for (route, title, source) in PAGES {
        let parsed = session.parse(&ParseRequest {
            path: format!("{}.md", route.trim_start_matches('/')),
            source: (*source).to_owned(),
            ..ParseRequest::default()
        });
        let document = parsed.document.expect("the page parses");
        let mut meta = PageMeta::new(Route::new(*route), *title, Locale::new("en"));
        meta.kind = DocKind::Page;
        documents.extend(section::extract(&document, &meta));
    }
    writer::build(&documents, &WriterOptions::default()).files
}

fn query(searcher: &Searcher, text: &str) -> Vec<String> {
    let response = searcher.search(&SearchRequest {
        query: text.to_owned(),
        ..SearchRequest::default()
    });
    assert!(
        response.diagnostics.is_empty(),
        "{:?}",
        response.diagnostics
    );
    response.hits.into_iter().map(|hit| hit.route).collect()
}

#[test]
fn a_query_finds_the_page_that_holds_the_word() {
    let searcher = Searcher::open(files()).expect("the index opens");
    assert_eq!(query(&searcher, "installer"), vec!["/install".to_owned()]);
    assert_eq!(query(&searcher, "database"), vec!["/upgrade".to_owned()]);
}

#[test]
fn a_hit_carries_what_the_dialog_shows() {
    let searcher = Searcher::open(files()).expect("the index opens");
    let response = searcher.search(&SearchRequest {
        query: "installer".to_owned(),
        ..SearchRequest::default()
    });
    let hit = response.hits.first().expect("one hit");
    assert_eq!(hit.title, "Install");
    assert_eq!(hit.kind, "page");
    assert!(hit.score > 0.0);
    assert!(hit.matched >= 1);
    let snippet = hit.snippet.as_ref().expect("snippets are on by default");
    assert!(snippet.text.contains("installer"), "{}", snippet.text);
}

#[test]
fn max_results_is_the_caller_s() {
    let searcher = Searcher::open(files()).expect("the index opens");
    let response = searcher.search(&SearchRequest {
        query: "the".to_owned(),
        max_results: Some(1),
        ..SearchRequest::default()
    });
    assert!(response.hits.len() <= 1);
}

#[test]
fn an_index_without_a_manifest_is_refused() {
    let mut files = files();
    files.remove(MANIFEST);
    let refused = Searcher::open(files).expect_err("there is nothing to read");
    assert_eq!(refused.as_slice()[0].code.as_str(), "E1003", "{refused:?}");
}

#[test]
fn an_index_this_reader_cannot_read_is_refused() {
    let mut files = files();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&files[MANIFEST]).expect("the manifest is json");
    manifest["version"] = serde_json::json!(u32::MAX);
    files.insert(
        MANIFEST.to_owned(),
        serde_json::to_vec(&manifest).expect("serializes"),
    );
    let refused = Searcher::open(files).expect_err("a newer format is not readable");
    assert_eq!(refused.as_slice()[0].code.as_str(), "E1002", "{refused:?}");
}

/// The worker asks again on the next keystroke; a half-fetched shard is not an
/// error, and it is not a silent empty result either — the manifest still says
/// the shard exists.
#[test]
fn a_shard_whose_files_have_not_arrived_is_skipped_rather_than_reported() {
    let all = files();
    let mut partial: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    partial.insert(MANIFEST.to_owned(), all[MANIFEST].clone());
    let mut searcher = Searcher::open(partial).expect("the manifest is enough to open");
    let response = searcher.search(&SearchRequest {
        query: "installer".to_owned(),
        ..SearchRequest::default()
    });
    assert!(response.hits.is_empty());
    assert!(
        response.diagnostics.is_empty(),
        "{:?}",
        response.diagnostics
    );

    for (name, bytes) in &all {
        searcher.add_file(name.clone(), bytes.clone());
    }
    assert_eq!(query(&searcher, "installer"), vec!["/install".to_owned()]);
}
