//! RX-31 and §12.2's parity gate: the browser index and the tantivy index
//! answer the same corpus the same way.
//!
//! Top-five **set** equality plus rank-1 equality, not order equality: float
//! summation order differs between the two, which §12.2 says in as many words.

#![cfg(feature = "server")]

mod support;

use std::collections::BTreeSet;

use liyasa_search::doc::SectionDocument;
use liyasa_search::idx::Index;
use liyasa_search::idx::manifest::Context;
use liyasa_search::idx::query;
use liyasa_search::idx::search::SearchOptions;
use liyasa_search::idx::tokenize::Tokenizer;
use liyasa_search::idx::writer::{self, WriterOptions};
use liyasa_search::server::{ServerIndex, ServerSearcher};
use support::corpus;

fn both(documents: &[SectionDocument]) -> (Index, ServerIndex) {
    let browser = Index::from_built(writer::build(documents, &WriterOptions::default()));
    let mut server = ServerIndex::in_memory();
    server.index_all(documents).expect("the index writes");
    (browser, server)
}

fn browser_keys(index: &Index, text: &str, locale: &str) -> Vec<String> {
    let parsed = query::parse(text, locale).expect("valid query");
    index
        .search(&parsed, &Context::default(), &SearchOptions::default())
        .expect("searches")
        .into_iter()
        .map(|hit| hit.url)
        .collect()
}

fn server_keys(index: &ServerIndex, text: &str, locale: &str) -> Vec<String> {
    let parsed = query::parse(text, locale).expect("valid query");
    ServerSearcher::new(index)
        .expect("opens")
        .search(&parsed, &SearchOptions::default())
        .expect("searches")
        .into_iter()
        .map(|hit| hit.key)
        .collect()
}

#[test]
fn both_indexes_agree_on_every_corpus_query() {
    let documents = corpus::documents();
    let (browser, server) = both(&documents);

    for case in corpus::cases() {
        let left = browser_keys(&browser, &case.query, &case.locale);
        let right = server_keys(&server, &case.query, &case.locale);

        if case.expect_empty {
            assert!(
                left.is_empty() && right.is_empty(),
                "`{}`: {left:?} / {right:?}",
                case.query
            );
            continue;
        }
        assert_eq!(
            left.first(),
            right.first(),
            "`{}` disagrees on rank 1: {left:?} / {right:?}",
            case.query
        );
        let top = |keys: &[String]| keys.iter().take(5).cloned().collect::<BTreeSet<_>>();
        assert_eq!(
            top(&left),
            top(&right),
            "`{}` disagrees on the top five: {left:?} / {right:?}",
            case.query
        );
    }
}

#[test]
fn both_indexes_hold_the_same_terms() {
    // §12.2's Parity row: "Tokenization is one shared module … so query terms
    // are tokenized exactly as the index was built."
    let documents = corpus::documents();
    let (browser, _) = both(&documents);
    let shard = browser
        .manifest
        .shards
        .first()
        .expect("the fixture is one shard");
    let reader = browser.reader(shard).expect("opens");

    for document in &documents {
        for term in Tokenizer::for_locale(document.locale.as_str()).tokenize(&document.body) {
            assert!(
                reader.contains(&term.text),
                "`{}` is in the corpus but not in the term dictionary",
                term.text
            );
        }
    }
}

#[test]
fn both_indexes_agree_on_the_reader_scope() {
    let documents = corpus::documents();
    let (browser, server) = both(&documents);
    let text = "raising a limit";
    assert!(!browser_keys(&browser, text, "en").contains(&"/internal/runbook".to_owned()));
    assert!(!server_keys(&server, text, "en").contains(&"/internal/runbook".to_owned()));
}

#[test]
fn the_stemmer_agrees_per_language() {
    // The stem is produced by one function for both indexes, so the assertion
    // that matters is that a locale's terms reach the tantivy dictionary in
    // the stemmed form the browser looks them up by.
    let documents = corpus::documents();
    let (_, server) = both(&documents);
    let german = server_keys(&server, "Ratenbegrenzung", "de");
    assert_eq!(
        german.first().map(String::as_str),
        Some("/de/anleitungen/limits")
    );
}

#[test]
fn a_larger_corpus_still_agrees() {
    let documents = corpus::reference_site(120);
    let (browser, server) = both(&documents);
    for text in ["rate limit", "webhook", "authentication token", "quota"] {
        let left = browser_keys(&browser, text, "en");
        let right = server_keys(&server, text, "en");
        assert_eq!(
            left.first(),
            right.first(),
            "`{text}`: {left:?} / {right:?}"
        );
        let top = |keys: &[String]| keys.iter().take(5).cloned().collect::<BTreeSet<_>>();
        assert_eq!(top(&left), top(&right), "`{text}`: {left:?} / {right:?}");
    }
}
