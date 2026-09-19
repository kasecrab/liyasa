//! The entry point a build calls to produce the search index.
//!
//! Everything else in this crate is tested from section documents a test wrote
//! by hand. This file starts one step earlier, from the Rendered AST and the
//! page facets a build actually has, because that is the seam where the index
//! went missing: the crate could always build an index and nothing ever asked
//! it to. See `plan/rfcs/0705-who-builds-the-search-index.md`.

use liyasa_core::document::{Block, BlockKind, Deps, Document};
use liyasa_core::ids::{Locale, Route};
use liyasa_core::{BlockId, Diagnostics};
use liyasa_search::build::{self, IndexPage};
use liyasa_search::config::{BoostRule, SearchSettings};
use liyasa_search::doc::PageMeta;
use liyasa_search::idx::Index;
use liyasa_search::idx::manifest::Context;
use liyasa_search::idx::query;
use liyasa_search::idx::search::SearchOptions;
use liyasa_search::idx::writer;

use super::support::{document, heading, para};

fn page(title: &str) -> Document {
    document(vec![
        heading(1, "", title),
        para("Every API key has a rate limit."),
        heading(2, "burst", "Burst limits"),
        para("Burst traffic is allowed up to twice the sustained rate."),
    ])
}

fn meta(route: &str, title: &str) -> PageMeta {
    PageMeta::new(Route::new(route), title, Locale::new("en"))
}

fn empty_document() -> Document {
    Document {
        root: Block {
            id: BlockId::implicit("fixture", "empty", "", 0),
            explicit_id: None,
            kind: BlockKind::Document,
            origin: liyasa_core::document::Origin {
                span: None,
                frames: Vec::new(),
            },
            children: Vec::new(),
        },
        deps: Deps::default(),
        diagnostics: Diagnostics::new(),
    }
}

fn site() -> Vec<(Document, PageMeta)> {
    vec![
        (page("Rate limits"), meta("/guides/limits", "Rate limits")),
        (
            page("Authentication"),
            meta("/guides/auth", "Authentication"),
        ),
    ]
}

fn pages(owned: &[(Document, PageMeta)]) -> Vec<IndexPage<'_>> {
    owned
        .iter()
        .map(|(document, meta)| IndexPage {
            document,
            meta: meta.clone(),
            indexed: true,
        })
        .collect()
}

#[test]
fn a_site_produces_an_index_a_reader_can_query() {
    let owned = site();
    let built = build::index_site(&pages(&owned), &SearchSettings::default());

    assert!(built.sections >= 4, "one document per section: {built:?}");
    assert_eq!(built.pages_indexed, 2);

    // The whole point: what comes back opens and answers.
    let index = Index::from_built(built.index);
    let parsed = query::parse("rate limit", "en").expect("valid query");
    let hits = index
        .search(&parsed, &Context::default(), &SearchOptions::default())
        .expect("searches");
    assert!(
        hits.iter().any(|hit| hit.url.starts_with("/guides/limits")),
        "{:?}",
        hits.iter().map(|h| &h.url).collect::<Vec<_>>()
    );
}

#[test]
fn the_files_are_named_for_the_directory_the_cli_reads() {
    let owned = site();
    let built = build::index_site(&pages(&owned), &SearchSettings::default());

    let paths: Vec<String> = built.output_files().map(|(path, _)| path).collect();
    assert!(
        paths
            .iter()
            .all(|path| path.starts_with(&format!("{}/", writer::DIRECTORY))),
        "{paths:?}"
    );
    assert!(
        paths.contains(&format!("{}/{}", writer::DIRECTORY, writer::MANIFEST)),
        "the manifest is what a reader opens first: {paths:?}"
    );
}

#[test]
fn a_page_the_build_excluded_from_search_is_not_in_the_index() {
    let owned = site();
    let mut offered = pages(&owned);
    offered[1].indexed = false;

    let built = build::index_site(&offered, &SearchSettings::default());
    assert_eq!(built.pages_indexed, 1);
    assert_eq!(built.pages_skipped, 1);

    let index = Index::from_built(built.index);
    let parsed = query::parse("authentication", "en").expect("valid query");
    let hits = index
        .search(&parsed, &Context::default(), &SearchOptions::default())
        .expect("searches");
    assert!(
        !hits.iter().any(|hit| hit.url.starts_with("/guides/auth")),
        "`indexing.search = false` keeps a page out: {:?}",
        hits.iter().map(|h| &h.url).collect::<Vec<_>>()
    );
}

#[test]
fn a_route_search_exclude_names_is_kept_out_too() {
    let owned = site();
    let settings = SearchSettings {
        exclude: vec!["/guides/auth**".to_owned()],
        ..SearchSettings::default()
    };
    let built = build::index_site(&pages(&owned), &settings);

    assert_eq!(built.pages_indexed, 1);
    assert_eq!(built.pages_skipped, 1);
}

#[test]
fn a_boost_rule_reaches_the_section_documents() {
    let owned = site();

    // Both pages answer `limit`: /guides/limits by title and body, /guides/auth
    // because its body says "rate limit" too. Without a boost the titled page
    // wins. The assertion is that a boost REORDERS them — `<=` would have
    // passed on a boost that did nothing at all, which is the whole failure
    // this test exists to catch.
    let order = |settings: &SearchSettings| -> Vec<String> {
        let index = Index::from_built(build::index_site(&pages(&owned), settings).index);
        let parsed = query::parse("limit", "en").expect("valid query");
        index
            .search(&parsed, &Context::default(), &SearchOptions::default())
            .expect("searches")
            .into_iter()
            .map(|hit| hit.url)
            .collect()
    };

    let plain = order(&SearchSettings::default());
    let boosted = order(&SearchSettings {
        boost: vec![BoostRule {
            matches: "/guides/auth**".to_owned(),
            factor: 50.0,
        }],
        ..SearchSettings::default()
    });

    assert!(
        plain
            .first()
            .is_some_and(|url| url.starts_with("/guides/limits")),
        "the titled page wins on its own: {plain:?}"
    );
    assert!(
        boosted
            .first()
            .is_some_and(|url| url.starts_with("/guides/auth")),
        "a boost of 50 must put the boosted page first: {boosted:?}"
    );
}

#[test]
fn a_boost_of_one_changes_nothing() {
    let owned = site();
    let neutral = SearchSettings {
        boost: vec![BoostRule {
            matches: "/guides/auth**".to_owned(),
            factor: 1.0,
        }],
        ..SearchSettings::default()
    };

    let order = |settings: &SearchSettings| -> Vec<String> {
        let index = Index::from_built(build::index_site(&pages(&owned), settings).index);
        let parsed = query::parse("limit", "en").expect("valid query");
        index
            .search(&parsed, &Context::default(), &SearchOptions::default())
            .expect("searches")
            .into_iter()
            .map(|hit| hit.url)
            .collect()
    };

    assert_eq!(order(&SearchSettings::default()), order(&neutral));
}

#[test]
fn a_pattern_that_matched_no_route_is_a_warning_rather_than_silence() {
    let owned = site();
    let settings = SearchSettings {
        exclude: vec!["/intrenal/**".to_owned()],
        ..SearchSettings::default()
    };
    let built = build::index_site(&pages(&owned), &settings);

    let warning = built
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_str() == "W1005")
        .unwrap_or_else(|| panic!("a typo in a glob is reportable: {:?}", built.diagnostics));
    assert!(warning.message.contains("/intrenal/**"), "{warning:?}");
}

#[test]
fn every_glob_matching_something_warns_about_nothing() {
    let owned = site();
    let settings = SearchSettings {
        exclude: vec!["/guides/auth**".to_owned()],
        ..SearchSettings::default()
    };
    let built = build::index_site(&pages(&owned), &settings);
    assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
}

#[test]
fn a_site_with_nothing_to_index_writes_an_empty_index_rather_than_none() {
    let built = build::index_site(&[], &SearchSettings::default());

    assert_eq!(built.sections, 0);
    assert!(built.is_empty());
    assert!(
        built
            .output_files()
            .any(|(path, _)| path.ends_with(writer::MANIFEST)),
        "an empty index is still an index: the CLI's E0016 means `no index`, \
         and a site with no indexable pages is not that"
    );

    // And it opens, rather than being a file the reader has to special-case.
    let index = Index::from_built(built.index);
    let parsed = query::parse("anything", "en").expect("valid query");
    assert!(
        index
            .search(&parsed, &Context::default(), &SearchOptions::default())
            .expect("an empty index still answers")
            .is_empty()
    );
}

#[test]
fn a_page_that_rendered_to_nothing_contributes_nothing_and_does_not_fail() {
    let document = empty_document();
    let offered = [IndexPage {
        document: &document,
        meta: meta("/empty", "Empty"),
        indexed: true,
    }];
    let built = build::index_site(&offered, &SearchSettings::default());
    assert_eq!(built.sections, 0);
    assert!(built.diagnostics.is_empty());
}

#[test]
fn the_search_settings_come_off_the_site_config() {
    let config = serde_json::json!({
        "name": "Docs",
        "search": {
            "maxResults": 5,
            "exclude": ["/internal/**"],
            "shardSize": { "min": "100KB", "max": "1MB" }
        }
    });
    let settings = build::settings_from_config(&config);
    assert_eq!(settings.max_results, 5);
    assert_eq!(settings.exclude, ["/internal/**"]);
    assert_eq!(settings.shard_size.min_bytes(), 100 * 1024);
}

#[test]
fn a_site_config_without_a_search_object_gets_the_defaults() {
    let config = serde_json::json!({ "name": "Docs" });
    assert_eq!(
        build::settings_from_config(&config),
        SearchSettings::default()
    );
    assert_eq!(
        build::settings_from_config(&serde_json::Value::Null),
        SearchSettings::default()
    );
}
