//! Reading one shard out of bytes the worker fetched (§12.2).
//!
//! Nothing here fetches anything: the caller hands over four byte slices,
//! which is what lets the same reader run over `fetch` in a Web Worker, over
//! `Vfs` in the CLI, and over a `Vec` in a test.

use fst::{Automaton, IntoStreamer, Streamer};

use super::docs::{self, DocMeta, DocStats, Facets};
use super::manifest::FORMAT_VERSION;
use super::postings::Block;
use super::snippets::Snippets;
use crate::error::SearchError;

/// The four files of one shard. `docs` may be just the ranking prefix
/// (`Shard::stats_bytes`), in which case [`ShardReader::meta`] returns `None`
/// until the rest arrives.
#[derive(Debug, Clone, Copy)]
pub struct ShardBytes<'a> {
    pub terms: &'a [u8],
    pub postings: &'a [u8],
    pub docs: &'a [u8],
    pub snippets: &'a [u8],
}

pub struct ShardReader<'a> {
    terms: fst::Map<&'a [u8]>,
    postings: &'a [u8],
    facets: Facets,
    stats: Vec<DocStats>,
    meta: Option<Vec<DocMeta>>,
    snippets: Snippets<'a>,
}

impl<'a> ShardReader<'a> {
    pub fn open(bytes: ShardBytes<'a>) -> Result<Self, SearchError> {
        let terms =
            fst::Map::new(bytes.terms).map_err(|_| SearchError::corrupt("terms-<n>.fst"))?;
        let (facets, stats) =
            docs::decode_stats(bytes.docs).ok_or_else(|| SearchError::corrupt("docs-<n>.bin"))?;
        Ok(Self {
            terms,
            postings: bytes.postings,
            facets,
            stats,
            meta: docs::decode_meta(bytes.docs),
            snippets: Snippets(bytes.snippets),
        })
    }

    /// Refuses a format this build does not understand, rather than decoding
    /// it into nonsense.
    pub fn check_version(version: u32) -> Result<(), SearchError> {
        if version > FORMAT_VERSION {
            return Err(SearchError::FormatVersion {
                found: version,
                supported: FORMAT_VERSION,
            });
        }
        Ok(())
    }

    pub fn documents(&self) -> u32 {
        self.stats.len() as u32
    }

    pub fn facets(&self) -> &Facets {
        &self.facets
    }

    pub fn stats(&self, doc: u32) -> Option<&DocStats> {
        self.stats.get(doc as usize)
    }

    pub fn meta(&self, doc: u32) -> Option<&DocMeta> {
        self.meta.as_ref()?.get(doc as usize)
    }

    pub fn snippet(&self, doc: u32) -> Option<&'a str> {
        let meta = self.meta(doc)?;
        self.snippets.get(meta.snippet_at, meta.snippet_len)
    }

    /// One term's postings, or `None` when the term is not in this shard.
    pub fn block(&self, term: &str) -> Result<Option<Block>, SearchError> {
        let Some(at) = self.terms.get(term) else {
            return Ok(None);
        };
        let mut at = at as usize;
        Block::decode(self.postings, &mut at)
            .map(Some)
            .ok_or_else(|| SearchError::corrupt("postings-<n>.bin"))
    }

    /// Every indexed term starting with `prefix`, capped: a one-letter prefix
    /// on a large shard matches thousands of terms and scoring them all would
    /// blow SRC-05's 50 ms budget for no change in the top results.
    pub fn terms_with_prefix(&self, prefix: &str, limit: usize) -> Vec<String> {
        let automaton = fst::automaton::Str::new(prefix).starts_with();
        let mut stream = self.terms.search(automaton).into_stream();
        let mut out = Vec::new();
        while let Some((term, _)) = stream.next() {
            if out.len() >= limit {
                break;
            }
            if let Ok(term) = std::str::from_utf8(term) {
                out.push(term.to_owned());
            }
        }
        out
    }

    /// Every indexed term within `distance` edits of `term` (SRC-04's typo
    /// tolerance). An automaton the crate refuses to build — the term is too
    /// long for the distance — yields nothing rather than failing the query.
    pub fn terms_within(&self, term: &str, distance: u32, limit: usize) -> Vec<String> {
        let Ok(automaton) = fst::automaton::Levenshtein::new(term, distance) else {
            return Vec::new();
        };
        let mut stream = self.terms.search(automaton).into_stream();
        let mut out = Vec::new();
        while let Some((candidate, _)) = stream.next() {
            if out.len() >= limit {
                break;
            }
            if let Ok(candidate) = std::str::from_utf8(candidate) {
                out.push(candidate.to_owned());
            }
        }
        out
    }

    pub fn contains(&self, term: &str) -> bool {
        self.terms.contains_key(term)
    }
}
