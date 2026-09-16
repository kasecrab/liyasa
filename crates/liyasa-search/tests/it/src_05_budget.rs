//! SRC-05 and §12.2's budgets, measured on the 1,000-page reference site.
//!
//! The e2e half of SRC-05 (`web/e2e/src_05_budget.spec.ts`) needs a browser
//! and a `web/` workspace that does not exist yet; what a browser would
//! measure is the byte counts and the query time asserted here, over the same
//! reader.

use super::support;

use std::time::Instant;

use liyasa_search::idx::Index;
use liyasa_search::idx::manifest::{Context, ShardKey};
use liyasa_search::idx::query;
use liyasa_search::idx::search::SearchOptions;
use liyasa_search::idx::writer::{self, WriterOptions};
use support::corpus;

const PAGES: usize = 1_000;
/// §12.2: "manifest plus first shard under 200 KB for the 1,000-page
/// reference site". Asserted uncompressed, which is the stricter reading.
const FIRST_RESULT_BUDGET: u64 = 200 * 1024;

fn reference() -> Index {
    Index::from_built(writer::build(
        &corpus::reference_site(PAGES),
        &WriterOptions::default(),
    ))
}

#[test]
fn the_bytes_before_a_first_result_are_under_the_budget() {
    let index = reference();
    // What the worker fetches for the reader's own context: the manifest, and
    // every shard that context matches.
    let bytes: u64 = index
        .manifest
        .shards_for(&Context::default())
        .into_iter()
        .map(|shard| index.first_result_bytes(shard))
        .sum();
    assert!(
        bytes < FIRST_RESULT_BUDGET,
        "{bytes} bytes before the first result over {} shards, budget \
         {FIRST_RESULT_BUDGET}",
        index.manifest.shards.len()
    );
}

#[test]
fn a_query_answers_within_the_latency_budget() {
    let index = reference();
    // Warm the reader the way the worker's first query does.
    let warm = query::parse("rate limit", "en").expect("parses");
    let _ = index.search(&warm, &Context::default(), &SearchOptions::default());

    let queries = [
        "rate limit",
        "authentication token",
        "webhook retry",
        "quota",
    ];
    for text in queries {
        let parsed = query::parse(text, "en").expect("parses");
        let started = Instant::now();
        let hits = index
            .search(&parsed, &Context::default(), &SearchOptions::default())
            .expect("searches");
        let elapsed = started.elapsed();
        assert!(!hits.is_empty(), "`{text}` found nothing");
        // §12.2 budgets a release build; a debug build is several times
        // slower and asserting 50 ms there would only produce a flaky test.
        let budget = if cfg!(debug_assertions) { 400 } else { 50 };
        assert!(
            elapsed.as_millis() < budget,
            "`{text}` took {elapsed:?}, budget {budget} ms"
        );
    }
}

#[test]
fn a_thousand_pages_needs_no_context_sharding() {
    let index = reference();
    // One locale, one version, one tab: there is no context to cut along, so
    // the key stays `single` however many chunks the 2 MB cap produces.
    assert_eq!(index.manifest.shard_key, ShardKey::Single);
    assert_eq!(index.manifest.documents, PAGES as u64 * 3);
    assert!(
        index
            .manifest
            .shards
            .iter()
            .all(|shard| shard.selector.specificity() == 0),
        "every shard serves every context"
    );
}

#[test]
fn a_shard_never_exceeds_the_configured_maximum() {
    let options = WriterOptions {
        shard_min_bytes: 8 * 1024,
        shard_max_bytes: 256 * 1024,
        ..WriterOptions::default()
    };
    let built = writer::build(&corpus::reference_site(400), &options);
    assert!(built.manifest.shards.len() > 1, "the site must have split");
    for shard in &built.manifest.shards {
        assert!(
            shard.bytes <= options.shard_max_bytes,
            "shard {} is {} bytes",
            shard.id,
            shard.bytes
        );
    }
}

#[test]
fn a_many_context_site_shards_by_locale_rather_than_by_every_dimension() {
    let documents = corpus::multi_context(40, &["en", "de", "fr", "ja"], &["v1", "v2"]);
    let built = writer::build(
        &documents,
        &WriterOptions {
            shard_min_bytes: 16 * 1024,
            shard_max_bytes: 192 * 1024,
            ..WriterOptions::default()
        },
    );
    assert_ne!(built.manifest.shard_key, ShardKey::Single);
    assert!(
        built.manifest.shards.len() <= 8,
        "{} shards for eight contexts",
        built.manifest.shards.len()
    );
    let index = Index::from_built(built);
    for locale in ["en", "de", "fr", "ja"] {
        let context = Context {
            locale: Some(locale.to_owned()),
            ..Context::default()
        };
        assert!(
            index.manifest.shard_for(&context).is_some(),
            "no shard serves {locale}"
        );
    }
}

#[test]
fn a_sharded_index_carries_the_cross_shard_idf_table() {
    let documents = corpus::multi_context(40, &["en", "de"], &["v1"]);
    let built = writer::build(
        &documents,
        &WriterOptions {
            shard_min_bytes: 8 * 1024,
            shard_max_bytes: 128 * 1024,
            ..WriterOptions::default()
        },
    );
    assert!(built.manifest.shards.len() > 1);
    assert!(
        !built.manifest.idf.terms.is_empty(),
        "terms that span shards must carry the corpus-wide value (§12.2)"
    );
}

#[test]
fn every_shard_is_content_addressed_and_immutable() {
    let documents = corpus::reference_site(200);
    let first = writer::build(&documents, &WriterOptions::default());
    let second = writer::build(&documents, &WriterOptions::default());
    let hashes = |built: &writer::BuiltIndex| {
        built
            .manifest
            .shards
            .iter()
            .map(|s| s.hash.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(hashes(&first), hashes(&second), "the build is reproducible");
    for shard in &first.manifest.shards {
        assert!(shard.hash.starts_with("blake3:"), "{}", shard.hash);
    }
}

#[test]
fn the_index_builds_within_its_budget() {
    let documents = corpus::reference_site(PAGES);
    let started = Instant::now();
    let built = writer::build(&documents, &WriterOptions::default());
    let elapsed = started.elapsed();
    assert_eq!(built.manifest.documents, PAGES as u64 * 3);
    // §12.2: "index build under 2 s for 1,000 pages", in a release build.
    let budget = if cfg!(debug_assertions) {
        30_000
    } else {
        2_000
    };
    assert!(
        elapsed.as_millis() < budget,
        "the build took {elapsed:?}, budget {budget} ms"
    );
}
