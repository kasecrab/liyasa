//! SRC-06's hybrid mode: tantivy's list fused with the assistant index's by
//! reciprocal rank fusion, and only when `search.mode` says so (CFG-54).
//!
//! The embeddings themselves belong to the assistant package, which is why
//! what is tested here is the fusion and the switch, not the similarity: the
//! caller runs its own embedding query and hands the ranking over.

#![cfg(feature = "server")]

use super::support;

use liyasa_search::api::{self, Engine};
use liyasa_search::config::{SearchMode, SearchSettings};
use liyasa_search::idx::manifest::Context;
use liyasa_search::idx::query::{self, ReaderScope};
use liyasa_search::idx::search::SearchOptions;
use liyasa_search::server::{Hybrid, ServerIndex, ServerSearcher};
use serde_json::Value;
use support::corpus;

fn server() -> ServerIndex {
    let mut index = ServerIndex::in_memory();
    index.index_all(&corpus::documents()).expect("indexes");
    index
}

fn settings(mode: SearchMode) -> SearchSettings {
    SearchSettings {
        mode,
        ..SearchSettings::default()
    }
}

fn keys<E: Engine + ?Sized>(engine: &E, text: &str) -> Vec<String> {
    let parsed = query::parse(text, "en").expect("valid query");
    engine
        .run(&parsed, &Context::default(), &SearchOptions::default())
        .expect("searches")
        .into_iter()
        .map(|hit| hit.url)
        .collect()
}

fn rank(list: &[String], key: &str) -> usize {
    list.iter()
        .position(|found| found == key)
        .unwrap_or(usize::MAX)
}

#[test]
fn keyword_mode_ignores_the_semantic_ranking_entirely() {
    let index = server();
    let searcher = ServerSearcher::new(&index).expect("opens");
    let plain = keys(&searcher, "rate limits");

    let semantic = vec!["/guides/sdk#getting-a-user".to_owned()];
    let fused = Hybrid::for_settings(&settings(SearchMode::Keyword), &searcher, &semantic);

    assert_eq!(
        keys(&fused, "rate limits"),
        plain,
        "CFG-54's default mode is tantivy alone"
    );
}

#[test]
fn hybrid_mode_promotes_what_both_halves_agree_on() {
    let index = server();
    let searcher = ServerSearcher::new(&index).expect("opens");
    let plain = keys(&searcher, "rate limits");
    assert!(
        plain.len() > 2,
        "the query has a list worth fusing: {plain:?}"
    );

    // The embeddings put what tantivy ranked last at the top.
    let last = plain.last().expect("a last result").clone();
    let semantic = vec![last.clone()];
    let fused = Hybrid::for_settings(&settings(SearchMode::Hybrid), &searcher, &semantic);
    let fused = keys(&fused, "rate limits");

    assert!(
        rank(&fused, &last) < rank(&plain, &last),
        "the embeddings' first choice rises: {plain:?} -> {fused:?}"
    );
    assert_eq!(
        fused.len(),
        plain.len(),
        "fusion reorders the list rather than shortening it"
    );
}

#[test]
fn a_section_only_the_embeddings_found_is_not_invented() {
    let index = server();
    let searcher = ServerSearcher::new(&index).expect("opens");
    let semantic = vec!["/guides/nothing-like-this".to_owned()];
    let fused = Hybrid::for_settings(&settings(SearchMode::Hybrid), &searcher, &semantic);

    let results = keys(&fused, "rate limits");
    assert!(
        !results.iter().any(|key| key == "/guides/nothing-like-this"),
        "a hit is only ever a document the index actually holds: {results:?}"
    );
}

#[test]
fn hybrid_with_nothing_to_fuse_is_the_keyword_ranking() {
    let index = server();
    let searcher = ServerSearcher::new(&index).expect("opens");
    let plain = keys(&searcher, "api key");
    let fused = Hybrid::for_settings(&settings(SearchMode::Hybrid), &searcher, &[]);

    assert_eq!(keys(&fused, "api key"), plain);
}

#[test]
fn fusion_respects_the_result_limit() {
    let index = server();
    let searcher = ServerSearcher::new(&index).expect("opens");
    let semantic = vec!["/changelog/2026-09".to_owned()];
    let fused = Hybrid::for_settings(&settings(SearchMode::Hybrid), &searcher, &semantic);

    let parsed = query::parse("rate limits", "en").expect("valid query");
    let options = SearchOptions {
        max_results: 2,
        ..SearchOptions::default()
    };
    let hits = fused
        .run(&parsed, &Context::default(), &options)
        .expect("searches");
    assert_eq!(hits.len(), 2);
}

#[test]
fn the_agent_surfaces_reach_hybrid_mode_through_the_same_engine() {
    let index = server();
    let searcher = ServerSearcher::new(&index).expect("opens");
    let semantic = vec!["/changelog/2026-09".to_owned()];
    let settings = settings(SearchMode::Hybrid);
    let fused = Hybrid::for_settings(&settings, &searcher, &semantic);

    let response = api::rest(&fused, "q=rate+limits", &settings, &ReaderScope::default()).0;
    assert_eq!(response.status, 200);

    let urls: Vec<&str> = response.body["results"]
        .as_array()
        .expect("results")
        .iter()
        .filter_map(|result: &Value| result["url"].as_str())
        .collect();
    assert!(urls.contains(&"/changelog/2026-09"), "{urls:?}");
}

#[test]
fn a_grouped_section_stays_hidden_however_the_embeddings_rank_it() {
    let index = server();
    let searcher = ServerSearcher::new(&index).expect("opens");
    let semantic = vec!["/internal/runbook".to_owned()];
    let settings = settings(SearchMode::Hybrid);
    let fused = Hybrid::for_settings(&settings, &searcher, &semantic);

    let parsed = query::parse("raising a limit", "en").expect("valid query");
    let hits = fused
        .run(&parsed, &Context::default(), &SearchOptions::default())
        .expect("searches");

    assert!(
        !hits.iter().any(|hit| hit.url.starts_with("/internal/")),
        "fusion reorders what the reader may see; it does not widen it"
    );
}
