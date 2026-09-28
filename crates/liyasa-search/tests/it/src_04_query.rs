//! SRC-04: prefixes, fuzziness, phrases, field filters, and suggestions.

use super::support;

use liyasa_search::idx::Index;
use liyasa_search::idx::manifest::Context;
use liyasa_search::idx::query;
use liyasa_search::idx::search::{Hit, SearchOptions};
use liyasa_search::idx::writer::{self, WriterOptions};
use support::corpus;

fn index() -> Index {
    Index::from_built(writer::build(
        &corpus::documents(),
        &WriterOptions::default(),
    ))
}

fn run(index: &Index, text: &str) -> Vec<Hit> {
    let query = query::parse(text, "en").expect("valid query");
    index
        .search(&query, &Context::default(), &SearchOptions::default())
        .expect("searches")
}

fn urls(hits: &[Hit]) -> Vec<String> {
    hits.iter().map(|hit| hit.url.clone()).collect()
}

#[test]
fn a_half_typed_word_matches_by_prefix() {
    let index = index();
    assert!(
        urls(&run(&index, "rotat")).contains(&"/guides/auth#rotating".to_owned()),
        "{:?}",
        urls(&run(&index, "rotat"))
    );
}

#[test]
fn only_the_last_word_is_a_prefix() {
    let index = index();
    let found = |hits: &[Hit], url: &str| {
        hits.iter()
            .find(|hit| hit.url == url)
            .map(|hit| hit.matched)
    };

    // Still typing: `dashbo` reaches `dashboard`.
    let typing = run(&index, "dashbo");
    assert_eq!(
        typing[0].url,
        "/guides/auth#api-keys",
        "{:?}",
        urls(&typing)
    );

    // Settled: the same fragment is now a whole word, and no document has it,
    // so the section matches `key` and only `key`.
    assert_eq!(
        found(&run(&index, "dashbo key"), "/guides/auth#api-keys"),
        Some(1)
    );
    assert_eq!(
        found(&run(&index, "dashboard key"), "/guides/auth#api-keys"),
        Some(2)
    );
}

#[test]
fn one_typo_is_forgiven_above_four_characters() {
    let index = index();
    assert!(!run(&index, "limts").is_empty(), "a five-letter typo");
    assert!(
        !run(&index, "requsts").is_empty(),
        "a missing letter in a longer word"
    );
}

#[test]
fn a_short_word_is_not_guessed_at() {
    let index = index();
    // `kay` is one edit from `key`, but three letters is too short for SRC-04
    // to tolerate, so the query must not quietly answer a different question.
    assert!(
        run(&index, "\"kay\"").is_empty(),
        "{:?}",
        urls(&run(&index, "\"kay\""))
    );
}

#[test]
fn an_exact_match_outranks_a_guess() {
    let index = index();
    let hits = run(&index, "limits");
    assert_eq!(hits[0].route, "/guides/limits");
}

#[test]
fn a_phrase_must_be_adjacent() {
    let index = index();
    let loose = urls(&run(&index, "ninety days"));
    assert!(loose.contains(&"/guides/auth#rotating".to_owned()));

    // The corpus writes "ninety days" but never "days ninety".
    let reversed = run(&index, "\"days ninety\"");
    let forward = run(&index, "\"ninety days\"");
    let score = |hits: &[Hit]| {
        hits.iter()
            .find(|hit| hit.url == "/guides/auth#rotating")
            .map_or(0.0, |hit| hit.score)
    };
    assert!(score(&forward) > score(&reversed), "a phrase is ordered");
}

#[test]
fn a_field_filter_narrows_without_changing_the_terms() {
    let index = index();
    let all = urls(&run(&index, "rate limits"));
    let v1 = urls(&run(&index, "version:v1 rate limits"));
    assert!(all.len() > v1.len());
    assert_eq!(v1, ["/v1/guides/limits"]);
}

#[test]
fn every_facet_filters() {
    let index = index();
    for (query, expected) in [
        ("tab:api user", "/api-reference/users/"),
        ("type:changelog September", "/changelog/"),
        ("locale:de Ratenbegrenzung", "/de/"),
    ] {
        let hits = urls(&run(&index, query));
        assert!(!hits.is_empty(), "`{query}` found nothing");
        assert!(
            hits.iter().all(|url| url.starts_with(expected)),
            "`{query}` returned {hits:?}"
        );
    }
}

#[test]
fn a_filter_that_matches_nothing_returns_nothing() {
    let index = index();
    assert!(run(&index, "version:v9 rate limits").is_empty());
}

#[test]
fn a_filter_on_its_own_is_not_a_search() {
    let index = index();
    assert!(
        run(&index, "version:v1").is_empty(),
        "a filter narrows a query; it is not one"
    );
}

#[test]
fn all_the_terms_win_over_some_of_them() {
    let index = index();
    let hits = run(&index, "burst rate limit");
    assert_eq!(
        hits[0].url,
        "/guides/limits#burst",
        "the only section with all three: {:?}",
        urls(&hits)
    );
}

#[test]
fn a_query_no_document_has_every_term_of_still_answers() {
    let index = index();
    let hits = run(&index, "rate limits quinoa");
    assert!(
        !hits.is_empty(),
        "one unknown word must not empty the result list"
    );
    assert_eq!(hits[0].url, "/guides/limits");
}

#[test]
fn a_code_identifier_is_found_by_its_parts() {
    let index = index();
    assert!(
        urls(&run(&index, "getUserById")).contains(&"/guides/sdk#getting-a-user".to_owned()),
        "{:?}",
        urls(&run(&index, "getUserById"))
    );
    assert!(
        urls(&run(&index, "client getUserById")).contains(&"/guides/sdk#getting-a-user".to_owned())
    );
}

#[test]
fn an_unbalanced_quote_is_e1004_rather_than_no_results() {
    let error = query::parse("\"rate limit", "en").expect_err("must not parse");
    assert_eq!(error.diagnostic().code.as_str(), "E1004");
}

#[test]
fn an_empty_query_is_not_an_error() {
    let index = index();
    assert!(run(&index, "").is_empty());
    assert!(run(&index, "   ").is_empty());
}

/// SRC-04's last clause: typo-tolerant suggestions.
///
/// Fuzzy matching and suggesting are different jobs. Fuzzy silently widens the
/// query, so it has to be conservative or a search for one word answers with
/// another. A suggestion is shown to the reader and applied only if they take
/// it, so it can reach further — and it is the only thing that helps when the
/// typo matched nothing at all.
mod suggestions {
    use liyasa_core::ids::{Locale, Route};
    use liyasa_search::doc::{DocKind, SectionDocument};
    use liyasa_search::idx::suggest::Suggestion;
    use liyasa_search::idx::writer::{self, WriterOptions};

    use super::*;

    fn suggest(index: &Index, text: &str) -> Vec<Suggestion> {
        let query = query::parse(text, "en").expect("valid query");
        index
            .suggest(&query, &Context::default(), &SearchOptions::default())
            .expect("suggests")
    }

    /// The suggestion for one query term, looked up by the term as the parsed
    /// query carries it — the stem. `limis` reaches the index as `limi`.
    fn suggested_for<'a>(list: &'a [Suggestion], term: &str) -> Option<&'a str> {
        list.iter()
            .find(|s| s.term == term)
            .map(|s| s.suggested.as_str())
    }

    #[test]
    fn a_typo_the_index_nearly_holds_is_corrected() {
        let index = index();
        let list = suggest(&index, "limis");
        assert_eq!(
            suggested_for(&list, "limi"),
            Some("limit"),
            "one edit from a term the corpus holds: {list:?}"
        );
    }

    #[test]
    fn a_term_the_index_holds_is_not_corrected() {
        let index = index();
        assert!(
            suggest(&index, "limits").is_empty(),
            "nothing to suggest for a word that is already in the index"
        );
    }

    #[test]
    fn only_the_unknown_term_of_a_query_is_corrected() {
        let index = index();
        let list = suggest(&index, "rate limis");
        assert!(suggested_for(&list, "rate").is_none(), "{list:?}");
        assert_eq!(suggested_for(&list, "limi"), Some("limit"), "{list:?}");
    }

    #[test]
    fn a_word_nothing_resembles_gets_no_guess() {
        let index = index();
        assert!(
            suggest(&index, "quinoa").is_empty(),
            "a miss with no near neighbour is a miss, not a wrong guess"
        );
    }

    #[test]
    fn a_short_typo_is_left_alone() {
        let index = index();
        // Below the fuzzy threshold a typo has too many neighbours for any of
        // them to be the answer, which is why fuzzy matching stops there too.
        assert!(
            suggest(&index, "lim").is_empty(),
            "three letters is a prefix, not a typo"
        );
    }

    /// Two candidates one edit away, with the frequencies controlled by the
    /// fixture rather than read off the reference corpus — so this asserts the
    /// ranking rule and not a memorised winner. `stash` is on three pages and
    /// `stasi` on one, so `stash` wins; swapping which is rarer swaps the
    /// answer, which is what makes the assertion falsifiable.
    #[test]
    fn the_commonest_candidate_wins() {
        fn page(route: &str, body: &str) -> SectionDocument {
            SectionDocument {
                route: Route::new(route),
                anchor: String::new(),
                title: "Cache".to_owned(),
                section: "Cache".to_owned(),
                breadcrumb: Vec::new(),
                body: body.to_owned(),
                code: String::new(),
                keywords: Vec::new(),
                tab: None,
                version: None,
                locale: Locale::new("en"),
                kind: DocKind::Page,
                boost: 1.0,
                groups: Vec::new(),
                regions: Vec::new(),
                updated: None,
            }
        }

        let common = vec![
            page("/a", "stash"),
            page("/b", "stash"),
            page("/c", "stash"),
            page("/d", "stasi"),
        ];
        let index = Index::from_built(writer::build(&common, &WriterOptions::default()));
        assert_eq!(
            suggested_for(&suggest(&index, "stasj"), "stasj"),
            Some("stash"),
            "three documents beat one"
        );

        // The mirror image: make `stasi` the common one and the answer follows.
        let flipped = vec![
            page("/a", "stasi"),
            page("/b", "stasi"),
            page("/c", "stasi"),
            page("/d", "stash"),
        ];
        let index = Index::from_built(writer::build(&flipped, &WriterOptions::default()));
        assert_eq!(
            suggested_for(&suggest(&index, "stasj"), "stasj"),
            Some("stasi"),
            "the ranking follows the frequency, not the spelling"
        );
    }

    #[test]
    fn a_suggestion_names_the_term_it_corrects() {
        let index = index();
        // `burts` stems to `burt`, one insertion from the indexed `burst`.
        let list = suggest(&index, "burts");
        let first = list.first().expect("a suggestion");
        assert_eq!(first.term, "burt", "the term the parsed query carried");
        assert_eq!(first.suggested, "burst");
    }

    #[test]
    fn suggesting_is_capped_so_a_long_query_cannot_stall_it() {
        let index = index();
        let words = vec!["limis"; 40].join(" ");
        let query = query::parse(&words, "en").expect("valid query");
        let list = index
            .suggest(&query, &Context::default(), &SearchOptions::default())
            .expect("suggests");
        assert_eq!(list.len(), 1, "one suggestion per distinct term: {list:?}");
    }
}
