//! Retrieval over tantivy, ranking through `idx::score`.
//!
//! Reading the inverted index directly rather than through tantivy's query
//! layer is the whole point: it yields `(document, field, term frequency,
//! positions)`, which is exactly what the browser reader yields, so one
//! scorer serves both and §12.2's parity gate is a statement about retrieval
//! rather than about two people implementing BM25 the same way
//! (plan/rfcs/0703-server-ranks-with-idx.md).

use std::collections::BTreeMap;

use tantivy::postings::Postings;
use tantivy::schema::IndexRecordOption;
use tantivy::termdict::TermDictionary;
use tantivy::{DocSet, Searcher, TantivyDocument, Term};

use super::writer::ServerIndex;
use crate::doc::SectionDocument;
use crate::error::SearchError;
use crate::idx::field::{ByField, Field};
use crate::idx::query::Query;
use crate::idx::score::{self, Ranked, Stats};
use crate::idx::search::{self, Hit, SearchOptions, Snippet};

/// One server-side hit. The section document is the stored payload, so a
/// caller renders it exactly as the build wrote it.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerHit {
    pub key: String,
    pub document: SectionDocument,
    pub score: f32,
    pub matched: usize,
    /// Cut from the stored section text, by the same function the browser
    /// reader uses, so a result reads the same either way.
    pub snippet: Option<Snippet>,
}

impl From<ServerHit> for Hit {
    fn from(hit: ServerHit) -> Self {
        let document = hit.document;
        Self {
            url: hit.key,
            route: document.route.as_str().to_owned(),
            anchor: document.anchor,
            title: document.title,
            section: document.section,
            breadcrumb: document.breadcrumb,
            kind: document.kind,
            tab: document.tab,
            version: document.version.map(|v| v.as_str().to_owned()),
            locale: Some(document.locale.as_str().to_owned()),
            score: hit.score,
            updated: document.updated,
            matched: hit.matched,
            snippet: hit.snippet,
        }
    }
}

impl Ranked for ServerHit {
    fn score(&self) -> f32 {
        self.score
    }

    fn updated(&self) -> Option<u64> {
        self.document.updated
    }

    fn tiebreak_key(&self) -> &str {
        &self.key
    }
}

pub struct ServerSearcher<'a> {
    index: &'a ServerIndex,
    searcher: Searcher,
    stats: Stats,
}

impl<'a> ServerSearcher<'a> {
    pub fn new(index: &'a ServerIndex) -> Result<Self, SearchError> {
        let reader = index
            .index
            .reader()
            .map_err(|e| SearchError::corrupt(e.to_string()))?;
        Ok(Self {
            searcher: reader.searcher(),
            stats: index.stats().stats(),
            index,
        })
    }

    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    pub fn search(
        &self,
        query: &Query,
        options: &SearchOptions,
    ) -> Result<Vec<ServerHit>, SearchError> {
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let schema = &self.index.schema;
        let mut accumulated: BTreeMap<(usize, u32), Accumulator> = BTreeMap::new();

        for (segment, reader) in self.searcher.segment_readers().iter().enumerate() {
            let alive = reader.alive_bitset();
            for (at, term) in query.terms.iter().enumerate() {
                for field in Field::ALL {
                    let inverted = reader
                        .inverted_index(schema.text[field])
                        .map_err(|e| SearchError::corrupt(e.to_string()))?;
                    for (text, damp) in expansions(inverted.terms(), term, options) {
                        let tantivy_term = Term::from_field_text(schema.text[field], &text);
                        let Some(mut postings) = inverted
                            .read_postings(&tantivy_term, IndexRecordOption::WithFreqsAndPositions)
                            .map_err(|e| SearchError::corrupt(e.to_string()))?
                        else {
                            continue;
                        };
                        // The corpus-wide frequency, not tantivy's per-field
                        // one: the browser scores a term by how many documents
                        // hold it anywhere (§12.2's global statistics).
                        let document_frequency = self
                            .index
                            .stats()
                            .document_frequency(&text)
                            .unwrap_or_else(|| inverted.doc_freq(&tantivy_term).unwrap_or(1));
                        let idf = score::idf(self.stats.documents, document_frequency);
                        let mut positions = Vec::new();
                        while postings.doc() != tantivy::TERMINATED {
                            let doc = postings.doc();
                            if alive.is_none_or(|bitset| bitset.is_alive(doc)) {
                                positions.clear();
                                postings.positions(&mut positions);
                                accumulated
                                    .entry((segment, doc))
                                    .or_insert_with(|| Accumulator::new(query.terms.len()))
                                    .record(
                                        at,
                                        field,
                                        postings.term_freq(),
                                        idf,
                                        damp,
                                        &text,
                                        &positions,
                                    );
                            }
                            postings.advance();
                        }
                    }
                }
            }
        }

        let complete = accumulated
            .values()
            .any(|a| a.matched() == query.terms.len());
        let mut hits = Vec::new();

        for ((segment, doc), mut accumulator) in accumulated {
            if complete && accumulator.matched() < query.terms.len() {
                continue;
            }
            let Some(reader) = self.searcher.segment_readers().get(segment) else {
                continue;
            };
            let mut lengths = ByField([0u32; 6]);
            for field in Field::ALL {
                lengths[field] = reader
                    .fast_fields()
                    .u64(&super::schema::length_field(field))
                    .ok()
                    .and_then(|column| column.first(doc))
                    .unwrap_or(0) as u32;
            }
            let address = tantivy::DocAddress {
                segment_ord: segment as u32,
                doc_id: doc,
            };
            let Ok(stored) = self.searcher.doc::<TantivyDocument>(address) else {
                continue;
            };
            let Some(document) = self.index.payload(&stored) else {
                continue;
            };
            if !options.reader.admits(&document.groups, &document.regions)
                || !passes(query, &document)
            {
                continue;
            }

            let mut total = accumulator.score(&self.stats, &lengths);
            for phrase in &query.phrases {
                for field in Field::ALL {
                    total += score::phrase_bonus(
                        &self.stats,
                        field,
                        accumulator.adjacent(phrase, field),
                    );
                }
            }
            let snippet = options.snippets.then(|| {
                search::snippet(
                    &document.body,
                    &accumulator.matched_terms(),
                    document.locale.as_str(),
                    options.snippet_chars,
                )
            });
            hits.push(ServerHit {
                key: document.key(),
                score: score::with_boost(total, document.boost),
                matched: accumulator.matched(),
                snippet,
                document,
            });
        }

        score::rank(&mut hits);
        hits.truncate(options.max_results);
        Ok(hits)
    }
}

fn passes(query: &Query, document: &SectionDocument) -> bool {
    let matches = |wanted: &Option<String>, value: Option<&str>| {
        wanted
            .as_deref()
            .is_none_or(|wanted| value.is_some_and(|value| value == wanted))
    };
    query
        .filters
        .kind
        .is_none_or(|wanted| wanted == document.kind)
        && matches(&query.filters.tab, document.tab.as_deref())
        && matches(
            &query.filters.version,
            document.version.as_ref().map(|v| v.as_str()),
        )
        && matches(&query.filters.locale, Some(document.locale.as_str()))
}

/// The same expansion rules as the browser reader, over tantivy's term
/// dictionary instead of the shard's FST.
fn expansions(
    terms: &TermDictionary,
    term: &crate::idx::query::Term,
    options: &SearchOptions,
) -> Vec<(String, f32)> {
    use tantivy::termdict::TermStreamer;

    let mut out: Vec<(String, f32)> = Vec::new();
    let mut push = |text: String, damp: f32| {
        if !out.iter().any(|(existing, _)| existing == &text) {
            out.push((text, damp));
        }
    };
    let collect = |streamer: &mut TermStreamer<'_>, limit: usize| {
        let mut found = Vec::new();
        while streamer.advance() {
            if found.len() >= limit {
                break;
            }
            if let Ok(text) = std::str::from_utf8(streamer.key()) {
                found.push(text.to_owned());
            }
        }
        found
    };

    for form in term.forms() {
        if terms.get(form).ok().flatten().is_some() {
            push(form.to_owned(), crate::idx::search::EXACT);
        }
    }
    if term.prefix {
        for form in term.forms() {
            let Ok(mut streamer) = terms.range().ge(form.as_bytes()).into_stream() else {
                continue;
            };
            for candidate in collect(&mut streamer, options.prefix_expansions) {
                if !candidate.starts_with(form) {
                    break;
                }
                push(candidate, crate::idx::search::PREFIX);
            }
        }
    }
    if term.fuzzy {
        // tantivy's dictionary has no Levenshtein automaton of its own, so the
        // candidates come from the neighbourhood of the term and are filtered
        // by the same distance the browser applies.
        let Ok(mut streamer) = terms.range().into_stream() else {
            return out;
        };
        let mut seen = 0usize;
        while streamer.advance() && seen < options.fuzzy_expansions * 64 {
            seen += 1;
            let Ok(candidate) = std::str::from_utf8(streamer.key()) else {
                continue;
            };
            if edit_distance_at_most_one(candidate, &term.text) {
                push(candidate.to_owned(), crate::idx::search::FUZZY);
            }
        }
    }
    out
}

/// True when `a` and `b` are at most one insertion, deletion, or substitution
/// apart. Cheaper and clearer than a full matrix for the only distance SRC-04
/// asks for.
fn edit_distance_at_most_one(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    let (long, short) = if a.len() >= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    let mut at = 0;
    let mut skipped = false;
    for (n, ch) in long.iter().enumerate() {
        if short.get(at) == Some(ch) {
            at += 1;
            continue;
        }
        if skipped {
            return false;
        }
        skipped = true;
        if long.len() == short.len() {
            at += 1;
        } else if n + 1 == long.len() && at == short.len() {
            return true;
        }
    }
    at == short.len()
}

/// One document's running score, the server's copy of the browser's.
struct Accumulator {
    best: Vec<f32>,
    /// One entry per `(query term, expansion)`; a term's contribution is the
    /// best of its expansions, as in the browser reader.
    frequencies: Vec<Expansion>,
    positions: BTreeMap<(String, u8), Vec<u32>>,
}

struct Expansion {
    term: usize,
    text: String,
    fields: ByField<u32>,
    idf: f32,
    damp: f32,
}

impl Accumulator {
    fn new(terms: usize) -> Self {
        Self {
            best: vec![0.0; terms],
            frequencies: Vec::new(),
            positions: BTreeMap::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn record(
        &mut self,
        term: usize,
        field: Field,
        frequency: u32,
        idf: f32,
        damp: f32,
        text: &str,
        positions: &[u32],
    ) {
        match self
            .frequencies
            .iter_mut()
            .find(|found| found.term == term && found.text == text)
        {
            Some(found) => found.fields[field] = frequency,
            None => {
                let mut fields = ByField([0u32; 6]);
                fields[field] = frequency;
                self.frequencies.push(Expansion {
                    term,
                    text: text.to_owned(),
                    fields,
                    idf,
                    damp,
                });
            }
        }
        if !positions.is_empty() {
            self.positions
                .entry((text.to_owned(), field.id()))
                .or_default()
                .extend_from_slice(positions);
        }
    }

    fn score(&mut self, stats: &Stats, lengths: &ByField<u32>) -> f32 {
        for expansion in &self.frequencies {
            let contribution = expansion.damp
                * score::term_score(stats, lengths, expansion.idf, &expansion.fields);
            if let Some(slot) = self.best.get_mut(expansion.term)
                && contribution > *slot
            {
                *slot = contribution;
            }
        }
        self.best.iter().sum()
    }

    /// The expansions that actually hit, for the snippet's highlights.
    fn matched_terms(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .frequencies
            .iter()
            .map(|found| found.text.clone())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    fn matched(&self) -> usize {
        let mut seen: Vec<usize> = self.frequencies.iter().map(|found| found.term).collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    }

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

/// The server answers the REST endpoint and the MCP tool through the same
/// entry point the browser index does. `Context` picks a shard, and the server
/// index has none, so it is the one argument this engine ignores.
impl crate::api::Engine for ServerSearcher<'_> {
    fn run(
        &self,
        query: &crate::idx::query::Query,
        _context: &crate::idx::manifest::Context,
        options: &SearchOptions,
    ) -> Result<Vec<Hit>, SearchError> {
        Ok(self
            .search(query, options)?
            .into_iter()
            .map(Hit::from)
            .collect())
    }
}

/// §12.1's fusion constant: large enough that a first place is worth a little
/// more than a second, not so much more that one list decides the order.
pub const RRF_K: f32 = 60.0;

/// An engine in hybrid mode (CFG-54): a keyword engine's list reordered by
/// [`hybrid`] against a ranking the caller got from the assistant index.
///
/// The embedding query itself is the assistant package's, and it is async,
/// which is why this takes the ranking rather than running it: the server
/// embeds the query, then hands the keys over.
pub struct Hybrid<'a, E: ?Sized> {
    keyword: &'a E,
    semantic: &'a [String],
    k: f32,
}

impl<'a, E: crate::api::Engine + ?Sized> Hybrid<'a, E> {
    /// Fuses only when `search.mode` is `"hybrid"`; in keyword mode the
    /// semantic ranking is ignored rather than quietly half-applied.
    pub fn for_settings(
        settings: &crate::config::SearchSettings,
        keyword: &'a E,
        semantic: &'a [String],
    ) -> Self {
        let semantic = match settings.mode {
            crate::config::SearchMode::Hybrid => semantic,
            crate::config::SearchMode::Keyword => &[],
        };
        Self {
            keyword,
            semantic,
            k: RRF_K,
        }
    }
}

impl<E: crate::api::Engine + ?Sized> crate::api::Engine for Hybrid<'_, E> {
    fn run(
        &self,
        query: &crate::idx::query::Query,
        context: &crate::idx::manifest::Context,
        options: &SearchOptions,
    ) -> Result<Vec<Hit>, SearchError> {
        if self.semantic.is_empty() {
            return self.keyword.run(query, context, options);
        }

        // A wider pool than the caller asked for, so a section the embeddings
        // rank highly can climb from outside the first page rather than only
        // within it.
        let pool = SearchOptions {
            max_results: options.max_results.saturating_mul(4).max(20),
            ..options.clone()
        };
        let mut hits = self.keyword.run(query, context, &pool)?;

        let keyword: Vec<String> = hits.iter().map(|hit| hit.url.clone()).collect();
        let fused = hybrid(&keyword, self.semantic, self.k);
        // A key only the embeddings returned has no document here, so it is
        // dropped: a result is always something the index holds, and the
        // reader filter has already run over it.
        let place = |url: &str| {
            fused
                .iter()
                .position(|key| key == url)
                .unwrap_or(usize::MAX)
        };
        hits.sort_by_key(|hit| place(&hit.url));
        hits.truncate(options.max_results);
        Ok(hits)
    }
}

/// Reciprocal rank fusion (SRC-06's hybrid mode): two ranked lists of keys
/// into one, without needing their scores to be on the same scale.
pub fn hybrid(keyword: &[String], semantic: &[String], k: f32) -> Vec<String> {
    let mut scores: BTreeMap<&str, f32> = BTreeMap::new();
    for list in [keyword, semantic] {
        for (rank, key) in list.iter().enumerate() {
            *scores.entry(key.as_str()).or_default() += 1.0 / (k + rank as f32 + 1.0);
        }
    }
    let mut fused: Vec<(&str, f32)> = scores.into_iter().collect();
    fused.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });
    fused.into_iter().map(|(key, _)| key.to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_edit_is_one_edit() {
        assert!(edit_distance_at_most_one("limit", "limt"), "deletion");
        assert!(edit_distance_at_most_one("limit", "limits"), "insertion");
        assert!(edit_distance_at_most_one("limit", "limat"), "substitution");
        assert!(edit_distance_at_most_one("limit", "limit"), "identity");
        assert!(!edit_distance_at_most_one("limit", "lmt"), "two deletions");
        assert!(!edit_distance_at_most_one("limit", "quota"), "unrelated");
        assert!(edit_distance_at_most_one("", "a"));
        assert!(!edit_distance_at_most_one("", "ab"));
    }

    #[test]
    fn fusion_rewards_agreement() {
        let keyword = vec!["/a".to_owned(), "/b".to_owned(), "/c".to_owned()];
        let semantic = vec!["/c".to_owned(), "/b".to_owned(), "/d".to_owned()];
        let fused = hybrid(&keyword, &semantic, 60.0);
        let rank = |key: &str| {
            fused
                .iter()
                .position(|found| found == key)
                .unwrap_or(usize::MAX)
        };
        // `/b` and `/c` are in both lists; `/a` and `/d` are in one. Agreement
        // is what reciprocal rank fusion pays for.
        assert!(rank("/b") < rank("/a"), "{fused:?}");
        assert!(rank("/b") < rank("/d"), "{fused:?}");
        assert!(rank("/c") < rank("/a"), "{fused:?}");
        assert!(rank("/c") < rank("/d"), "{fused:?}");
        assert_eq!(fused.len(), 4, "nothing is dropped");
    }

    #[test]
    fn fusion_of_one_list_keeps_its_order() {
        let keyword = vec!["/a".to_owned(), "/b".to_owned()];
        assert_eq!(hybrid(&keyword, &[], 60.0), keyword);
    }
}
