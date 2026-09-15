//! Section documents into a `search-index/` directory (§12.2).
//!
//! The writer performs no I/O: it returns the bytes of every file, keyed by
//! the path they belong at, and the caller writes them through `Vfs`. That is
//! what lets the same code run in the build, in a test, and in a browser.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::ids::Fingerprint;

use super::docs::{self, DocEntry, DocMeta, DocStats, Facets};
use super::field::{ByField, Field};
use super::manifest::{Manifest, Shard, ShardFiles, ShardKey, ShardSelector, TokenizerConfig};
use super::postings::{Block, FieldPosting, Posting};
use super::score;
use super::snippets::SnippetWriter;
use super::tokenize::{self, Tokenizer};
use crate::doc::SectionDocument;

/// The directory every path in [`BuiltIndex::files`] is relative to.
pub const DIRECTORY: &str = "search-index";
pub const MANIFEST: &str = "manifest.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterOptions {
    /// `search.shardSize.min` (§8.6). Shards below it are merged.
    pub shard_min_bytes: u64,
    /// `search.shardSize.max`. A key that cannot hold every shard under it is
    /// escalated to the next one in [`ShardKey::ESCALATION`].
    pub shard_max_bytes: u64,
    /// `search.snippets`. Off writes an empty `snippets-<n>.bin` and results
    /// carry no highlighted text.
    pub snippets: bool,
    /// How much of a section's prose is kept for highlighting. A snippet is
    /// two lines in a dialog; keeping the whole of a long page would dominate
    /// the shard for text no reader sees.
    pub snippet_bytes: usize,
}

impl Default for WriterOptions {
    fn default() -> Self {
        Self {
            shard_min_bytes: 200 * 1024,
            shard_max_bytes: 2 * 1024 * 1024,
            snippets: true,
            snippet_bytes: 1200,
        }
    }
}

/// Everything the build writes, plus the manifest it wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltIndex {
    pub manifest: Manifest,
    /// Relative to [`DIRECTORY`].
    pub files: BTreeMap<String, Vec<u8>>,
}

impl BuiltIndex {
    pub fn total_bytes(&self) -> u64 {
        self.files.values().map(|f| f.len() as u64).sum()
    }
}

pub fn build(documents: &[SectionDocument], options: &WriterOptions) -> BuiltIndex {
    let analyzed: Vec<Analyzed> = documents.iter().map(Analyzed::of).collect();
    let stats = corpus_stats(&analyzed);

    let (key, groups) = partition(&analyzed, options);
    let groups = chunk_large(&analyzed, merge_small(&analyzed, groups, options), options);

    let mut files = BTreeMap::new();
    let mut shards = Vec::with_capacity(groups.len());
    let mut shard_terms: Vec<BTreeSet<&str>> = Vec::with_capacity(groups.len());

    for (id, group) in groups.into_iter().enumerate() {
        let id = id as u32;
        let members: Vec<&Analyzed> = group.members.iter().map(|&at| &analyzed[at]).collect();
        let bytes = encode_shard(&members, &stats.document_frequency, options);
        let names = ShardFiles::of(id);
        let hash = Fingerprint::of_parts([
            bytes.terms.as_slice(),
            bytes.postings.as_slice(),
            bytes.docs.bytes.as_slice(),
            bytes.snippets.as_slice(),
        ]);
        let sized = (bytes.terms.len() + bytes.postings.len()) as u64;
        let total = sized + (bytes.docs.bytes.len() + bytes.snippets.len()) as u64;

        shard_terms.push(
            members
                .iter()
                .flat_map(|m| m.terms.keys().map(String::as_str))
                .collect(),
        );
        files.insert(names.terms.clone(), bytes.terms);
        files.insert(names.postings.clone(), bytes.postings);
        files.insert(names.docs.clone(), bytes.docs.bytes);
        files.insert(names.snippets.clone(), bytes.snippets);
        shards.push(Shard {
            id,
            selector: group.selector,
            documents: members.len() as u64,
            files: names,
            bytes: sized,
            total_bytes: total,
            stats_bytes: bytes.docs.stats_bytes,
            hash: hash.to_string(),
        });
    }

    let mut manifest = Manifest {
        documents: stats.documents,
        tokenizer: tokenizer_config(documents),
        shard_key: key,
        shards,
        ..Manifest::default()
    };
    manifest.set_average_length(stats.average_length);
    manifest.idf = cross_shard_idf(&stats, &shard_terms);

    let json = serde_json::to_vec_pretty(&manifest).unwrap_or_default();
    files.insert(MANIFEST.to_owned(), json);
    BuiltIndex { manifest, files }
}

// ---- analysis ----

/// One document's term postings and field lengths. Computed once; the
/// partition search below re-encodes shards but never re-tokenizes.
struct Analyzed<'a> {
    document: &'a SectionDocument,
    terms: BTreeMap<String, ByField<Vec<u32>>>,
    lengths: ByField<u32>,
}

impl<'a> Analyzed<'a> {
    fn of(document: &'a SectionDocument) -> Self {
        let prose = Tokenizer::for_locale(document.locale.as_str());
        let mut terms: BTreeMap<String, ByField<Vec<u32>>> = BTreeMap::new();
        let mut lengths = ByField([0u32; 6]);

        let breadcrumb = document.breadcrumb.join(" ");
        let keywords = document.keywords.join(" ");
        let fields = [
            (Field::Title, document.title.as_str()),
            (Field::Section, document.section.as_str()),
            (Field::Keywords, keywords.as_str()),
            (Field::Breadcrumb, breadcrumb.as_str()),
            (Field::Body, document.body.as_str()),
        ];
        for (field, text) in fields {
            let tokens = prose.tokenize(text);
            lengths[field] = tokens.len() as u32;
            for token in tokens {
                terms
                    .entry(token.text)
                    .or_insert_with(|| ByField(std::array::from_fn(|_| Vec::new())))[field]
                    .push(token.position);
            }
        }
        let tokens = tokenize::code(&document.code);
        lengths[Field::Code] = tokens.len() as u32;
        for token in tokens {
            terms
                .entry(token.text)
                .or_insert_with(|| ByField(std::array::from_fn(|_| Vec::new())))[Field::Code]
                .push(token.position);
        }
        Self {
            document,
            terms,
            lengths,
        }
    }
}

struct CorpusStats {
    documents: u64,
    average_length: ByField<f32>,
    document_frequency: BTreeMap<String, u32>,
}

fn corpus_stats(analyzed: &[Analyzed]) -> CorpusStats {
    let mut totals = ByField([0u64; 6]);
    let mut document_frequency: BTreeMap<String, u32> = BTreeMap::new();
    for entry in analyzed {
        for field in Field::ALL {
            totals[field] += u64::from(entry.lengths[field]);
        }
        for term in entry.terms.keys() {
            *document_frequency.entry(term.clone()).or_default() += 1;
        }
    }
    let count = analyzed.len().max(1) as f32;
    let mut average_length = ByField([0.0f32; 6]);
    for field in Field::ALL {
        average_length[field] = totals[field] as f32 / count;
    }
    CorpusStats {
        documents: analyzed.len() as u64,
        average_length,
        document_frequency,
    }
}

fn tokenizer_config(documents: &[SectionDocument]) -> TokenizerConfig {
    let mut config = TokenizerConfig::default();
    for document in documents {
        let locale = document.locale.as_str();
        if config.locales.contains_key(locale) {
            continue;
        }
        let algorithm = match tokenize::algorithm_for(locale) {
            // The `Debug` name is the Snowball language, which is exactly what
            // belongs in a file an operator reads.
            Some(algorithm) => format!("{algorithm:?}").to_lowercase(),
            None => "none".to_owned(),
        };
        config.locales.insert(locale.to_owned(), algorithm);
    }
    config
}

/// The idf table of §12.2: only terms that span shards, because a term in one
/// shard already has the corpus frequency there.
fn cross_shard_idf(
    stats: &CorpusStats,
    shard_terms: &[BTreeSet<&str>],
) -> super::manifest::IdfTable {
    let mut table = super::manifest::IdfTable::default();
    if shard_terms.len() < 2 {
        return table;
    }
    for (term, &frequency) in &stats.document_frequency {
        let shards = shard_terms
            .iter()
            .filter(|terms| terms.contains(term.as_str()))
            .count();
        if shards > 1 {
            table.insert(term, score::idf(stats.documents, frequency));
        }
    }
    table
}

// ---- sharding ----

struct Group {
    selector: ShardSelector,
    members: Vec<usize>,
}

/// The coarsest key under which no shard exceeds `shard_max_bytes`.
///
/// When no key fits — a large site in one locale, one version, and one tab has
/// no context to cut along — the key that comes closest wins and
/// [`chunk_large`] splits what is left. Escalating past that would label the
/// index `localeVersionTab` while producing exactly the groups `single` did.
fn partition(analyzed: &[Analyzed], options: &WriterOptions) -> (ShardKey, Vec<Group>) {
    let mut best: Option<(ShardKey, Vec<Group>, u64)> = None;
    for key in ShardKey::ESCALATION {
        let groups = group_by(analyzed, key);
        let largest = groups
            .iter()
            .map(|group| estimate(analyzed, group))
            .max()
            .unwrap_or(0);
        if largest <= options.shard_max_bytes {
            return (key, groups);
        }
        if best
            .as_ref()
            .is_none_or(|(_, _, previous)| largest < *previous)
        {
            best = Some((key, groups, largest));
        }
    }
    match best {
        Some((key, groups, _)) => (key, groups),
        None => (ShardKey::Single, group_by(analyzed, ShardKey::Single)),
    }
}

/// Splits a group that is still over the cap into equal chunks that share its
/// selector, so §12.2's "capped at 2 MB per shard by the build, which splits
/// larger shards" holds even where there is no context to shard by. The
/// reader fetches every chunk its context matches.
fn chunk_large(analyzed: &[Analyzed], groups: Vec<Group>, options: &WriterOptions) -> Vec<Group> {
    let cap = options.shard_max_bytes.max(1);
    let mut out = Vec::with_capacity(groups.len());
    for group in groups {
        let size = estimate(analyzed, &group);
        if size <= cap || group.members.len() < 2 {
            out.push(group);
            continue;
        }
        let chunks = size.div_ceil(cap).max(2) as usize;
        let per = group.members.len().div_ceil(chunks);
        for members in group.members.chunks(per) {
            out.push(Group {
                selector: group.selector.clone(),
                members: members.to_vec(),
            });
        }
    }
    out
}

fn group_by(analyzed: &[Analyzed], key: ShardKey) -> Vec<Group> {
    let mut groups: BTreeMap<(String, String, String), Vec<usize>> = BTreeMap::new();
    for (at, entry) in analyzed.iter().enumerate() {
        groups.entry(shard_key_of(entry, key)).or_default().push(at);
    }
    groups
        .into_iter()
        .map(|((locale, version, tab), members)| Group {
            selector: ShardSelector {
                locales: non_empty(locale),
                versions: non_empty(version),
                tabs: non_empty(tab),
            },
            members,
        })
        .collect()
}

fn shard_key_of(entry: &Analyzed, key: ShardKey) -> (String, String, String) {
    let document = entry.document;
    let locale = document.locale.as_str().to_owned();
    let version = document
        .version
        .as_ref()
        .map(|v| v.as_str().to_owned())
        .unwrap_or_default();
    let tab = document.tab.clone().unwrap_or_default();
    match key {
        ShardKey::Single => (String::new(), String::new(), String::new()),
        ShardKey::Locale => (locale, String::new(), String::new()),
        ShardKey::LocaleVersion => (locale, version, String::new()),
        ShardKey::LocaleVersionTab => (locale, version, tab),
    }
}

fn non_empty(value: String) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        vec![value]
    }
}

/// Merges groups under `shard_min_bytes` into their neighbour, so a
/// 12-locale, 4-version, 3-tab site does not produce 144 tiny shards (§12.2).
fn merge_small(analyzed: &[Analyzed], groups: Vec<Group>, options: &WriterOptions) -> Vec<Group> {
    if groups.len() < 2 {
        return groups;
    }
    let mut out: Vec<Group> = Vec::with_capacity(groups.len());
    for group in groups {
        let size = estimate(analyzed, &group);
        match out.last_mut() {
            Some(previous)
                if size < options.shard_min_bytes
                    && estimate(analyzed, previous) < options.shard_min_bytes
                    && estimate(analyzed, previous) + size <= options.shard_max_bytes =>
            {
                previous.members.extend(group.members);
                merge_selectors(&mut previous.selector, group.selector);
            }
            _ => out.push(group),
        }
    }
    out
}

fn merge_selectors(into: &mut ShardSelector, other: ShardSelector) {
    // A dimension one side leaves open stays open: the merged shard serves
    // every context either side served.
    for (target, source) in [
        (&mut into.locales, other.locales),
        (&mut into.versions, other.versions),
        (&mut into.tabs, other.tabs),
    ] {
        if target.is_empty() || source.is_empty() {
            target.clear();
            continue;
        }
        for value in source {
            if !target.contains(&value) {
                target.push(value);
            }
        }
        target.sort();
    }
}

/// Bytes the term dictionary and postings of a group would occupy — the part
/// `search.shardSize` caps. Close enough to choose a key by: a posting is a
/// handful of varints and the estimate counts them.
fn estimate(analyzed: &[Analyzed], group: &Group) -> u64 {
    group
        .members
        .iter()
        .map(|&at| {
            let entry = &analyzed[at];
            entry
                .terms
                .iter()
                .map(|(term, positions)| {
                    let occurrences: usize = positions.0.iter().map(Vec::len).sum();
                    // Term text in the FST, the block header, and two bytes
                    // for each delta-encoded position.
                    (term.len() + 6 + occurrences * 2) as u64
                })
                .sum::<u64>()
        })
        .sum()
}

// ---- shard encoding ----

struct ShardBytes {
    terms: Vec<u8>,
    postings: Vec<u8>,
    docs: docs::Encoded,
    snippets: Vec<u8>,
}

fn encode_shard(
    members: &[&Analyzed],
    document_frequency: &BTreeMap<String, u32>,
    options: &WriterOptions,
) -> ShardBytes {
    let mut snippets = SnippetWriter::default();
    let mut facets = Facets::default();
    let mut entries = Vec::with_capacity(members.len());
    let mut terms: BTreeMap<&str, Vec<Posting>> = BTreeMap::new();

    for (ordinal, member) in members.iter().enumerate() {
        let document = member.document;
        let (snippet_at, snippet_len) = if options.snippets {
            snippets.push(clip(&document.body, options.snippet_bytes))
        } else {
            (0, 0)
        };
        entries.push(DocEntry {
            stats: DocStats {
                kind: document.kind,
                boost: document.boost,
                updated: document.updated,
                lengths: member.lengths,
                tab: document
                    .tab
                    .as_deref()
                    .map(|tab| Facets::intern(&mut facets.tabs, tab)),
                version: document
                    .version
                    .as_ref()
                    .map(|v| Facets::intern(&mut facets.versions, v.as_str())),
                locale: Some(Facets::intern(
                    &mut facets.locales,
                    document.locale.as_str(),
                )),
                groups: document
                    .groups
                    .iter()
                    .map(|g| Facets::intern(&mut facets.groups, g))
                    .collect(),
                regions: document
                    .regions
                    .iter()
                    .map(|r| Facets::intern(&mut facets.regions, r))
                    .collect(),
            },
            meta: DocMeta {
                route: document.route.as_str().to_owned(),
                anchor: document.anchor.clone(),
                title: document.title.clone(),
                section: document.section.clone(),
                breadcrumb: document.breadcrumb.clone(),
                snippet_at,
                snippet_len,
            },
        });

        for (term, positions) in &member.terms {
            let fields = Field::ALL
                .into_iter()
                .filter(|&field| !positions[field].is_empty())
                .map(|field| FieldPosting {
                    field,
                    positions: positions[field].clone(),
                })
                .collect();
            terms.entry(term).or_default().push(Posting {
                doc: ordinal as u32,
                fields,
            });
        }
    }

    let mut postings = Vec::new();
    let mut builder = fst::MapBuilder::memory();
    for (term, list) in terms {
        let at = postings.len() as u64;
        Block {
            global_df: document_frequency.get(term).copied().unwrap_or(1),
            postings: list,
        }
        .encode(&mut postings);
        builder
            .insert(term, at)
            .expect("a BTreeMap yields terms in lexicographic order, without duplicates");
    }

    ShardBytes {
        terms: builder.into_inner().unwrap_or_default(),
        postings,
        docs: docs::encode(&facets, &entries),
        snippets: snippets.finish(),
    }
}

/// Truncates on a character boundary, never inside one.
fn clip(text: &str, bytes: usize) -> &str {
    if text.len() <= bytes {
        return text;
    }
    let mut end = bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_stops_on_a_character_boundary() {
        assert_eq!(clip("abcdef", 3), "abc");
        assert_eq!(clip("abc", 10), "abc");
        // Each of these is three bytes, so a cut at 4 lands mid-character.
        assert_eq!(clip("検索エンジン", 4), "検");
    }
}
