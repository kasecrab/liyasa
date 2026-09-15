//! Running a [`Query`] against a [`ShardReader`] (§12.2's Query row).
//!
//! Terms are expanded over the FST — exact, then prefix, then one edit — and
//! each expansion is damped so an exact match always outranks a guess. Scoring
//! is [`score`], the same function the server calls
//! (plan/rfcs/0703-server-ranks-with-idx.md).

use std::collections::BTreeMap;

use super::field::{ByField, Field};
use super::manifest::IdfTable;
use super::query::{Query, ReaderScope, Term};
use super::reader::ShardReader;
use super::score::{self, Ranked, Stats};
use super::tokenize::Tokenizer;
use crate::doc::DocKind;
use crate::error::SearchError;

/// What a prefix or fuzzy expansion is worth next to the term as typed.
const EXACT: f32 = 1.0;
const PREFIX: f32 = 0.9;
const FUZZY: f32 = 0.6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchOptions {
    /// `search.maxResults` (CFG-53).
    pub max_results: usize,
    /// `search.snippets` (CFG-53).
    pub snippets: bool,
    /// Characters of context around the first match.
    pub snippet_chars: usize,
    pub reader: ReaderScope,
    /// Caps on FST expansion, so a one-letter prefix cannot blow SRC-05's
    /// 50 ms budget.
    pub prefix_expansions: usize,
    pub fuzzy_expansions: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            max_results: 20,
            snippets: true,
            snippet_chars: 180,
            reader: ReaderScope::default(),
            prefix_expansions: 32,
            fuzzy_expansions: 16,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Highlight {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    pub text: String,
    /// Byte ranges within `text`, ascending and non-overlapping.
    pub highlights: Vec<Highlight>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub url: String,
    pub route: String,
    pub anchor: String,
    pub title: String,
    pub section: String,
    pub breadcrumb: Vec<String>,
    pub kind: DocKind,
    pub tab: Option<String>,
    pub version: Option<String>,
    pub locale: Option<String>,
    pub score: f32,
    pub updated: Option<u64>,
    /// How many of the query's terms this document matched.
    pub matched: usize,
    pub snippet: Option<Snippet>,
}

impl Ranked for Hit {
    fn score(&self) -> f32 {
        self.score
    }

    fn updated(&self) -> Option<u64> {
        self.updated
    }

    fn tiebreak_key(&self) -> &str {
        &self.url
    }
}

pub fn search(
    reader: &ShardReader<'_>,
    query: &Query,
    stats: &Stats,
    idf: &IdfTable,
    options: &SearchOptions,
) -> Result<Vec<Hit>, SearchError> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let mut docs: BTreeMap<u32, Accumulator> = BTreeMap::new();
    // Positions are needed only to prove a phrase is adjacent, and keeping
    // them for every term of every hit dominates the query budget on a large
    // shard. A query with no phrase keeps none.
    let phrase_terms: Vec<&str> = query.phrases.iter().flatten().map(String::as_str).collect();

    for (at, term) in query.terms.iter().enumerate() {
        for (expansion, damp) in expansions(reader, term, options) {
            let Some(block) = reader.block(&expansion)? else {
                continue;
            };
            let idf = idf
                .get(&expansion)
                .unwrap_or_else(|| score::idf(stats.documents, block.global_df));
            for posting in block.postings {
                let Some(lengths) = reader.stats(posting.doc).map(|s| s.lengths) else {
                    continue;
                };
                let mut frequencies = ByField([0u32; 6]);
                for field in Field::ALL {
                    frequencies[field] = posting.term_frequency(field);
                }
                let contribution = damp * score::term_score(stats, &lengths, idf, &frequencies);
                let wants_positions = phrase_terms.contains(&expansion.as_str());
                let accumulator = docs.entry(posting.doc).or_default();
                accumulator.record(
                    at,
                    query.terms.len(),
                    contribution,
                    &expansion,
                    &posting,
                    wants_positions,
                );
            }
        }
    }

    // A document that has every term is a better answer than one that has some,
    // whatever the arithmetic says; only fall back to partial matches when
    // nothing has them all.
    let complete = docs.values().any(|a| a.matched() == query.terms.len());
    let mut candidates = Vec::with_capacity(docs.len().min(options.max_results * 4));

    for (doc, accumulator) in docs {
        if complete && accumulator.matched() < query.terms.len() {
            continue;
        }
        let (Some(stats_of), Some(meta)) = (reader.stats(doc), reader.meta(doc)) else {
            continue;
        };
        let facets = reader.facets();
        let names = |ids: &[u32], from: &[String]| -> Vec<String> {
            ids.iter()
                .filter_map(|&id| from.get(id as usize).cloned())
                .collect()
        };
        if !options.reader.admits(
            &names(&stats_of.groups, &facets.groups),
            &names(&stats_of.regions, &facets.regions),
        ) {
            continue;
        }
        let tab = stats_of
            .tab
            .and_then(|id| facets.tabs.get(id as usize).cloned());
        let version = stats_of
            .version
            .and_then(|id| facets.versions.get(id as usize).cloned());
        let locale = stats_of
            .locale
            .and_then(|id| facets.locales.get(id as usize).cloned());
        if !passes(
            query,
            stats_of.kind,
            tab.as_deref(),
            version.as_deref(),
            locale.as_deref(),
        ) {
            continue;
        }

        let mut total = accumulator.score();
        for phrase in &query.phrases {
            for field in Field::ALL {
                total += score::phrase_bonus(stats, field, accumulator.adjacent(phrase, field));
            }
        }

        candidates.push(Candidate {
            doc,
            url: meta.url(),
            score: score::with_boost(total, stats_of.boost),
            updated: stats_of.updated,
            matched: accumulator.matched(),
            terms: accumulator.matched_terms,
            kind: stats_of.kind,
            tab,
            version,
            locale,
        });
    }

    // Ranking first, rendering second: cutting a shard's worth of candidates
    // down to `max_results` before tokenizing any snippet is the difference
    // between SRC-05's 50 ms budget and missing it by an order of magnitude.
    score::rank(&mut candidates);
    candidates.truncate(options.max_results);

    let mut hits = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let Some(meta) = reader.meta(candidate.doc) else {
            continue;
        };
        let snippet = options
            .snippets
            .then(|| {
                reader.snippet(candidate.doc).map(|text| {
                    snippet(
                        text,
                        &candidate.terms,
                        candidate.locale.as_deref().unwrap_or("en"),
                        options.snippet_chars,
                    )
                })
            })
            .flatten();
        hits.push(Hit {
            url: candidate.url,
            route: meta.route.clone(),
            anchor: meta.anchor.clone(),
            title: meta.title.clone(),
            section: meta.section.clone(),
            breadcrumb: meta.breadcrumb.clone(),
            kind: candidate.kind,
            tab: candidate.tab,
            version: candidate.version,
            locale: candidate.locale,
            score: candidate.score,
            updated: candidate.updated,
            matched: candidate.matched,
            snippet,
        });
    }
    Ok(hits)
}

/// A hit before it is rendered: everything ranking needs and nothing it does
/// not.
struct Candidate {
    doc: u32,
    url: String,
    score: f32,
    updated: Option<u64>,
    matched: usize,
    terms: Vec<String>,
    kind: DocKind,
    tab: Option<String>,
    version: Option<String>,
    locale: Option<String>,
}

impl Ranked for Candidate {
    fn score(&self) -> f32 {
        self.score
    }

    fn updated(&self) -> Option<u64> {
        self.updated
    }

    fn tiebreak_key(&self) -> &str {
        &self.url
    }
}

/// The index terms one query term reaches, best first.
fn expansions(
    reader: &ShardReader<'_>,
    term: &Term,
    options: &SearchOptions,
) -> Vec<(String, f32)> {
    let mut out = Vec::new();
    let mut push = |text: String, damp: f32| {
        if !out
            .iter()
            .any(|(existing, _): &(String, f32)| existing == &text)
        {
            out.push((text, damp));
        }
    };
    for form in term.forms() {
        if reader.contains(form) {
            push(form.to_owned(), EXACT);
        }
    }
    if term.prefix {
        for form in term.forms() {
            for candidate in reader.terms_with_prefix(form, options.prefix_expansions) {
                push(candidate, PREFIX);
            }
        }
    }
    if term.fuzzy {
        for candidate in reader.terms_within(&term.text, 1, options.fuzzy_expansions) {
            push(candidate, FUZZY);
        }
    }
    out
}

fn passes(
    query: &Query,
    kind: DocKind,
    tab: Option<&str>,
    version: Option<&str>,
    locale: Option<&str>,
) -> bool {
    let matches = |wanted: &Option<String>, value: Option<&str>| {
        wanted
            .as_deref()
            .is_none_or(|wanted| value.is_some_and(|value| value == wanted))
    };
    query.filters.kind.is_none_or(|wanted| wanted == kind)
        && matches(&query.filters.tab, tab)
        && matches(&query.filters.version, version)
        && matches(&query.filters.locale, locale)
}

/// One document's running score. Each query term contributes its best
/// expansion rather than the sum of them, so a broad prefix widens what is
/// found without inflating what is ranked.
#[derive(Debug, Default)]
struct Accumulator {
    best: Vec<f32>,
    positions: BTreeMap<(String, u8), Vec<u32>>,
    matched_terms: Vec<String>,
}

impl Accumulator {
    fn record(
        &mut self,
        term: usize,
        total: usize,
        contribution: f32,
        expansion: &str,
        posting: &super::postings::Posting,
        wants_positions: bool,
    ) {
        if self.best.len() < total {
            self.best.resize(total, 0.0);
        }
        if let Some(slot) = self.best.get_mut(term)
            && contribution > *slot
        {
            *slot = contribution;
        }
        if contribution > 0.0 && !self.matched_terms.iter().any(|t| t == expansion) {
            self.matched_terms.push(expansion.to_owned());
        }
        if wants_positions {
            for field in Field::ALL {
                let found = posting.positions(field);
                if !found.is_empty() {
                    self.positions
                        .entry((expansion.to_owned(), field.id()))
                        .or_default()
                        .extend_from_slice(found);
                }
            }
        }
    }

    fn score(&self) -> f32 {
        self.best.iter().sum()
    }

    fn matched(&self) -> usize {
        self.best.iter().filter(|&&s| s > 0.0).count()
    }

    /// How many times `phrase`'s terms appear consecutively in `field`.
    fn adjacent(&self, phrase: &[String], field: Field) -> u32 {
        let Some(first) = phrase.first() else {
            return 0;
        };
        let Some(starts) = self.positions.get(&(first.clone(), field.id())) else {
            return 0;
        };
        starts
            .iter()
            .filter(|&&start| {
                phrase.iter().enumerate().skip(1).all(|(step, term)| {
                    self.positions
                        .get(&(term.clone(), field.id()))
                        .is_some_and(|found| found.contains(&(start + step as u32)))
                })
            })
            .count() as u32
    }
}

/// A window of section text around the first matched term, with the matches
/// marked. Tokenizing the snippet is exact by construction: it is the same
/// function that produced the terms in the index.
fn snippet(text: &str, matched: &[String], locale: &str, chars: usize) -> Snippet {
    let tokens = Tokenizer::for_locale(locale).tokenize(text);
    let first = tokens
        .iter()
        .find(|token| matched.iter().any(|m| m == &token.text))
        .map_or(0, |token| token.start as usize);

    let start = floor_boundary(text, first.saturating_sub(chars / 3));
    let end = ceil_boundary(text, (start + chars).min(text.len()));
    let window = &text[start..end];

    let highlights = tokens
        .iter()
        .filter(|token| matched.iter().any(|m| m == &token.text))
        .filter(|token| token.start as usize >= start && token.end as usize <= end)
        .map(|token| Highlight {
            start: token.start - start as u32,
            end: token.end - start as u32,
        })
        .collect();

    Snippet {
        text: window.to_owned(),
        highlights,
    }
}

fn floor_boundary(text: &str, mut at: usize) -> usize {
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn ceil_boundary(text: &str, mut at: usize) -> usize {
    while at < text.len() && !text.is_char_boundary(at) {
        at += 1;
    }
    at.min(text.len())
}
