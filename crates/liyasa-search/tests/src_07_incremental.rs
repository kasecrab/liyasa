//! SRC-07: one page changes, one page is re-indexed, and the rest come back
//! from the artifact cache. SRC-11's index lifecycle rides along.

#![cfg(feature = "server")]

mod support;

use liyasa_core::conformance::fixtures::MemoryCache;
use liyasa_search::doc::SectionDocument;
use liyasa_search::idx::Index;
use liyasa_search::idx::manifest::Context;
use liyasa_search::idx::query;
use liyasa_search::idx::search::SearchOptions;
use liyasa_search::idx::writer::{self, WriterOptions};
use liyasa_search::incremental::{self, SectionFingerprints};
use liyasa_search::server::{Merges, ServerIndex, ServerSearcher};
use support::corpus;

fn edit(documents: &mut [SectionDocument], route: &str, body: &str) {
    for document in documents.iter_mut() {
        if document.route.as_str() == route && document.anchor.is_empty() {
            document.body = body.to_owned();
        }
    }
}

#[test]
fn only_the_changed_sections_reach_the_writer() {
    let mut documents = corpus::documents();
    let mut index = ServerIndex::in_memory();
    index.index_all(&documents).expect("first build");
    let before = SectionFingerprints::of(&documents);

    edit(
        &mut documents,
        "/guides/limits",
        "Every API key now has two rate limits.",
    );
    let plan = incremental::plan(&before, &documents);
    assert_eq!(plan.changed, ["/guides/limits"]);
    assert_eq!(plan.unchanged.len(), documents.len() - 1);

    let to_write = plan.to_write();
    let written = index
        .index_changed(&documents, &|document| to_write.contains(&document.key()))
        .expect("second build");
    assert_eq!(written, ["/guides/limits"], "the writer log");
}

#[test]
fn a_re_indexed_page_is_replaced_rather_than_duplicated() {
    let mut documents = corpus::documents();
    let mut index = ServerIndex::in_memory();
    index.index_all(&documents).expect("first build");
    let keys_before = index.keys().count();

    edit(
        &mut documents,
        "/guides/limits",
        "Bursts are now smoothed per key.",
    );
    index
        .index_changed(&documents, &|_| true)
        .expect("second build");
    assert_eq!(index.keys().count(), keys_before);

    let parsed = query::parse("smoothed per key", "en").expect("parses");
    let hits = ServerSearcher::new(&index)
        .expect("opens")
        .search(&parsed, &SearchOptions::default())
        .expect("searches");
    assert_eq!(hits.len(), 1, "the old text must not still match");
    assert_eq!(hits[0].key, "/guides/limits");
}

#[test]
fn a_deleted_page_leaves_the_index() {
    let mut documents = corpus::documents();
    let mut index = ServerIndex::in_memory();
    index.index_all(&documents).expect("first build");

    documents.retain(|document| document.route.as_str() != "/guides/limits");
    index
        .index_changed(&documents, &|_| false)
        .expect("second build");
    assert!(!index.contains("/guides/limits"));

    let parsed = query::parse("rate limits", "en").expect("parses");
    let hits = ServerSearcher::new(&index)
        .expect("opens")
        .search(&parsed, &SearchOptions::default())
        .expect("searches");
    assert!(
        !hits.iter().any(|hit| hit.key == "/guides/limits"),
        "{:?}",
        hits.iter().map(|h| &h.key).collect::<Vec<_>>()
    );
}

#[test]
fn removing_a_page_updates_the_corpus_statistics() {
    let mut documents = corpus::documents();
    let mut index = ServerIndex::in_memory();
    index.index_all(&documents).expect("first build");
    let before = index.stats().documents;

    documents.retain(|document| document.route.as_str() != "/guides/limits");
    let removed = before - documents.len() as u64;
    index
        .index_changed(&documents, &|_| false)
        .expect("second build");
    assert_eq!(index.stats().documents, before - removed);
    assert_eq!(index.stats().documents, documents.len() as u64);
}

fn shard_files(built: &liyasa_search::idx::writer::BuiltIndex) -> usize {
    built.manifest.shards.len() * 4
}

#[test]
fn unchanged_shards_come_back_from_the_cache() {
    let documents = corpus::multi_context(30, &["en", "de"], &["v1"]);
    let options = WriterOptions {
        shard_min_bytes: 4 * 1024,
        shard_max_bytes: 96 * 1024,
        ..WriterOptions::default()
    };
    let first = writer::build(&documents, &options);
    assert!(first.manifest.shards.len() > 1, "the fixture must shard");

    let cache = MemoryCache::new();
    let stored = incremental::store(&cache, &first);
    assert!(stored.misses > 0, "a cold cache had to be filled");
    assert_eq!(stored.total(), shard_files(&first));
    // Two shards whose bytes are identical — the fixture repeats one body per
    // locale — share one cache entry, which is what content addressing is for.

    // Rebuilding the same corpus writes the same bytes, so every shard hits.
    let again = writer::build(&documents, &options);
    let report = incremental::report(&cache, &again);
    assert_eq!(report.misses, 0, "{report:?}");
    assert_eq!(report.hits, report.total());
}

#[test]
fn editing_one_locale_leaves_the_other_locales_shards_cached() {
    let mut documents = corpus::multi_context(30, &["en", "de"], &["v1"]);
    let options = WriterOptions {
        shard_min_bytes: 4 * 1024,
        shard_max_bytes: 96 * 1024,
        ..WriterOptions::default()
    };
    let cache = MemoryCache::new();
    incremental::store(&cache, &writer::build(&documents, &options));

    for document in &mut documents {
        if document.locale.as_str() == "de" && document.anchor.is_empty() {
            document.body.push_str(" Eine neue Zeile.");
        }
    }
    let rebuilt = writer::build(&documents, &options);
    let report = incremental::report(&cache, &rebuilt);
    assert!(
        report.hits > 0,
        "the untouched locale's shards must still be cached: {report:?}"
    );
    assert!(report.misses > 0, "the edited locale's shards must not be");
}

#[test]
fn the_browser_index_agrees_after_an_incremental_rebuild() {
    let mut documents = corpus::documents();
    edit(
        &mut documents,
        "/guides/limits",
        "Quota ceilings are per key.",
    );
    let index = Index::from_built(writer::build(&documents, &WriterOptions::default()));
    let parsed = query::parse("quota ceilings", "en").expect("parses");
    let hits = index
        .search(&parsed, &Context::default(), &SearchOptions::default())
        .expect("searches");
    assert_eq!(hits[0].url, "/guides/limits");
}

#[test]
fn dev_defers_its_merges_and_production_never_merges() {
    let documents = corpus::documents();
    let mut production = ServerIndex::in_memory();
    assert_eq!(production.merges(), Merges::Never);
    production.index_all(&documents).expect("builds");
    production.merge_now().expect("a no-op in production");

    let mut dev = ServerIndex::in_memory().with_merges(Merges::Deferred);
    dev.index_all(&documents).expect("builds");
    dev.index_changed(&documents, &|_| true).expect("rebuilds");
    dev.merge_now().expect("merges when idle");

    let parsed = query::parse("rate limits", "en").expect("parses");
    let hits = ServerSearcher::new(&dev)
        .expect("opens")
        .search(&parsed, &SearchOptions::default())
        .expect("searches");
    assert_eq!(hits[0].key, "/guides/limits");
}
