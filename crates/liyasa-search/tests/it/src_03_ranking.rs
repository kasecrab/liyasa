//! SRC-03: BM25 with field weights, config boosts, recency, and phrases.

use super::support;

use liyasa_search::idx::Index;
use liyasa_search::idx::manifest::Context;
use liyasa_search::idx::query::{self, ReaderScope};
use liyasa_search::idx::search::{Hit, SearchOptions};
use liyasa_search::idx::writer::{self, WriterOptions};
use support::corpus;

fn index() -> Index {
    Index::from_built(writer::build(
        &corpus::documents(),
        &WriterOptions::default(),
    ))
}

fn run(index: &Index, text: &str, locale: &str) -> Vec<Hit> {
    let query = query::parse(text, locale).expect("the corpus holds only valid queries");
    index
        .search(&query, &Context::default(), &SearchOptions::default())
        .expect("the index this test just built is not corrupt")
}

fn urls(hits: &[Hit]) -> Vec<String> {
    hits.iter().map(|hit| hit.url.clone()).collect()
}

#[test]
fn every_corpus_query_holds() {
    let index = index();
    for case in corpus::cases() {
        let hits = run(&index, &case.query, &case.locale);
        let found = urls(&hits);
        let top_five: Vec<&String> = found.iter().take(5).collect();

        if case.expect_empty {
            assert!(hits.is_empty(), "`{}` found {found:?}", case.query);
            continue;
        }
        assert!(!hits.is_empty(), "`{}` found nothing", case.query);

        if let Some(top) = &case.top {
            assert_eq!(
                found.first().map(String::as_str),
                Some(top.as_str()),
                "`{}` ranked {found:?}",
                case.query
            );
        }
        for wanted in &case.contains {
            assert!(
                top_five.contains(&wanted),
                "`{}` did not put {wanted} in the top five: {top_five:?}",
                case.query
            );
        }
        for unwanted in &case.absent {
            assert!(
                !found.iter().any(|url| url == unwanted),
                "`{}` returned {unwanted}",
                case.query
            );
        }
    }
}

#[test]
fn a_title_match_outranks_a_body_match() {
    let index = index();
    let hits = run(&index, "authentication", "en");
    assert_eq!(hits[0].route, "/guides/auth");
}

#[test]
fn a_config_boost_reorders() {
    let mut documents = corpus::documents();
    let plain = Index::from_built(writer::build(&documents, &WriterOptions::default()));
    let before = urls(&run(&plain, "rate limits", "en"));
    assert_eq!(before.first().map(String::as_str), Some("/guides/limits"));

    // `search.boost` with `{ match: "/guides/limits#burst", factor: 3 }`.
    for document in &mut documents {
        if document.route.as_str() == "/guides/limits" && document.anchor == "burst" {
            document.boost = 3.0;
        }
    }
    let boosted = Index::from_built(writer::build(&documents, &WriterOptions::default()));
    let after = urls(&run(&boosted, "rate limits", "en"));

    assert_ne!(before, after, "a threefold boost must change the order");
    assert_eq!(
        after.first().map(String::as_str),
        Some("/guides/limits#burst"),
        "{after:?}"
    );
}

#[test]
fn a_factor_under_one_deprioritizes() {
    let mut documents = corpus::documents();
    for document in &mut documents {
        if document.route.as_str() == "/guides/limits" && document.anchor.is_empty() {
            document.boost = 0.1;
        }
    }
    let index = Index::from_built(writer::build(&documents, &WriterOptions::default()));
    let hits = urls(&run(&index, "rate limits", "en"));
    assert_ne!(
        hits.first().map(String::as_str),
        Some("/guides/limits"),
        "a tenth of the weight must not stay first: {hits:?}"
    );
}

#[test]
fn an_exact_phrase_beats_the_same_words_apart() {
    let index = index();
    let loose = run(&index, "rate limit", "en");
    let exact = run(&index, "\"rate limit\"", "en");
    let of = |hits: &[Hit], url: &str| {
        hits.iter()
            .find(|hit| hit.url == url)
            .map_or(0.0, |hit| hit.score)
    };
    // `/guides/limits#burst` writes "sustained rate limit"; the changelog
    // writes "Rate limits are now per key", which is the same two stems but
    // not adjacent.
    let phrase_gain = of(&exact, "/guides/limits#burst") - of(&loose, "/guides/limits#burst");
    let apart_gain = of(&exact, "/changelog/2026-09") - of(&loose, "/changelog/2026-09");
    assert!(
        phrase_gain > apart_gain,
        "phrase {phrase_gain}, apart {apart_gain}"
    );
}

#[test]
fn recency_breaks_a_tie_between_identical_pages() {
    let index = index();
    let hits = urls(&run(&index, "version:v1 rate limits", "en"));
    assert_eq!(hits.first().map(String::as_str), Some("/v1/guides/limits"));

    // The two versioned pages differ only in their text and date; with the
    // filter lifted the newer one must come first among them.
    let both = urls(&run(&index, "rate limit smoothed", "en"));
    let v1 = both.iter().position(|url| url == "/v1/guides/limits");
    let v2 = both.iter().position(|url| url == "/v2/guides/limits");
    assert!(v2 < v1 || v1.is_none(), "{both:?}");
}

#[test]
fn a_gated_section_is_invisible_to_a_reader_outside_its_group() {
    let index = index();
    let anonymous = run(&index, "raising a limit", "en");
    assert!(
        !anonymous.iter().any(|hit| hit.url == "/internal/runbook"),
        "{:?}",
        urls(&anonymous)
    );

    let query = query::parse("raising a limit", "en").expect("parses");
    let staff = index
        .search(
            &query,
            &Context::default(),
            &SearchOptions {
                reader: ReaderScope {
                    groups: vec!["staff".to_owned()],
                    region: None,
                },
                ..SearchOptions::default()
            },
        )
        .expect("searches");
    assert!(staff.iter().any(|hit| hit.url == "/internal/runbook"));
}

#[test]
fn max_results_is_honoured() {
    let index = index();
    let query = query::parse("rate limits", "en").expect("parses");
    let hits = index
        .search(
            &query,
            &Context::default(),
            &SearchOptions {
                max_results: 2,
                ..SearchOptions::default()
            },
        )
        .expect("searches");
    assert_eq!(hits.len(), 2);
}

#[test]
fn a_hit_carries_a_highlighted_snippet() {
    let index = index();
    let hits = run(&index, "burst", "en");
    let snippet = hits[0]
        .snippet
        .as_ref()
        .expect("snippets are on by default");
    assert!(snippet.text.contains("Burst"), "{}", snippet.text);
    let first = snippet.highlights.first().expect("the match is marked");
    let marked = &snippet.text[first.start as usize..first.end as usize];
    assert_eq!(marked.to_lowercase(), "burst");
}

#[test]
fn snippets_can_be_turned_off() {
    let documents = corpus::documents();
    let index = Index::from_built(writer::build(
        &documents,
        &WriterOptions {
            snippets: false,
            ..WriterOptions::default()
        },
    ));
    let query = query::parse("burst", "en").expect("parses");
    let hits = index
        .search(
            &query,
            &Context::default(),
            &SearchOptions {
                snippets: false,
                ..SearchOptions::default()
            },
        )
        .expect("searches");
    assert!(hits[0].snippet.is_none());
}

#[test]
fn the_same_query_ranks_the_same_way_twice() {
    let first = urls(&run(&index(), "rate limits", "en"));
    let second = urls(&run(&index(), "rate limits", "en"));
    assert_eq!(first, second, "ranking must be deterministic (§6.6.2)");
}
