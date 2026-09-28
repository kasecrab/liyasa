//! A whole `search-index/` directory, opened and queried.
//!
//! The browser worker holds one of these and fills it lazily as it fetches;
//! the CLI and the tests hold one with every file already in it. Either way
//! the scoring statistics come from the manifest, never from a shard.

use std::collections::BTreeMap;

use super::manifest::{Context, Manifest, Shard};
use super::query::Query;
use super::reader::{ShardBytes, ShardReader};
use super::score::{self, Stats};
use super::search::{self, Hit, SearchOptions};
use super::suggest::{self, Suggestion};
use super::writer::{BuiltIndex, MANIFEST};
use crate::error::SearchError;

pub struct Index {
    pub manifest: Manifest,
    files: BTreeMap<String, Vec<u8>>,
}

impl Index {
    /// Opens what the writer produced, without a round trip through bytes.
    pub fn from_built(built: BuiltIndex) -> Self {
        Self {
            manifest: built.manifest,
            files: built.files,
        }
    }

    /// Opens a directory the caller read, from `manifest.json` outward.
    pub fn open(files: BTreeMap<String, Vec<u8>>) -> Result<Self, SearchError> {
        let json = files
            .get(MANIFEST)
            .ok_or_else(|| SearchError::corrupt(MANIFEST))?;
        let manifest: Manifest =
            serde_json::from_slice(json).map_err(|_| SearchError::corrupt(MANIFEST))?;
        ShardReader::check_version(manifest.version)?;
        Ok(Self { manifest, files })
    }

    pub fn file(&self, name: &str) -> Option<&[u8]> {
        self.files.get(name).map(Vec::as_slice)
    }

    /// What a reader downloads before it can show a first result: the
    /// manifest, the shard's term dictionary, and the ranking prefix of its
    /// `docs-<n>.bin`. Postings arrive afterwards, as one range per query
    /// term, and the display half and snippets later still (§12.2, SRC-05).
    pub fn first_result_bytes(&self, shard: &Shard) -> u64 {
        let of = |name: &str| self.files.get(name).map_or(0, |f| f.len() as u64);
        of(MANIFEST) + of(&shard.files.terms) + shard.stats_bytes
    }

    pub fn total_bytes(&self) -> u64 {
        self.files.values().map(|f| f.len() as u64).sum()
    }

    pub fn stats(&self) -> Stats {
        Stats {
            documents: self.manifest.documents,
            average_length: self.manifest.average_length(),
            weights: self.manifest.weights(),
        }
    }

    pub fn reader(&self, shard: &Shard) -> Result<ShardReader<'_>, SearchError> {
        let of = |name: &str| {
            self.files
                .get(name)
                .map(Vec::as_slice)
                .ok_or_else(|| SearchError::corrupt(name.to_owned()))
        };
        ShardReader::open(ShardBytes {
            terms: of(&shard.files.terms)?,
            postings: of(&shard.files.postings)?,
            docs: of(&shard.files.docs)?,
            snippets: of(&shard.files.snippets)?,
        })
    }

    /// Searches the shard the context selects, or every shard when nothing
    /// pins one — which is what the CLI, the REST endpoint, and the MCP tool
    /// do. Scores are comparable across shards because the statistics are
    /// global (§12.2).
    pub fn search(
        &self,
        query: &Query,
        context: &Context,
        options: &SearchOptions,
    ) -> Result<Vec<Hit>, SearchError> {
        let stats = self.stats();
        let chosen: Vec<&Shard> = if context == &Context::default() {
            self.manifest.all_shards().iter().collect()
        } else {
            self.manifest.shards_for(context)
        };
        let mut hits = Vec::new();
        for shard in chosen {
            let reader = self.reader(shard)?;
            hits.extend(search::search(
                &reader,
                query,
                &stats,
                &self.manifest.idf,
                options,
            )?);
        }
        score::rank(&mut hits);
        hits.truncate(options.max_results);
        Ok(hits)
    }

    /// What the reader probably meant, for the terms this index does not hold
    /// (SRC-04). Deliberately not folded into `search`: a caller shows a
    /// suggestion when the results are thin, and paying for one on every query
    /// that already answered well would spend SRC-05's budget on nothing.
    pub fn suggest(
        &self,
        query: &Query,
        context: &Context,
        _options: &SearchOptions,
    ) -> Result<Vec<Suggestion>, SearchError> {
        let chosen: Vec<&Shard> = if context == &Context::default() {
            self.manifest.all_shards().iter().collect()
        } else {
            self.manifest.shards_for(context)
        };

        let mut out: Vec<Suggestion> = Vec::new();
        for shard in chosen {
            let reader = self.reader(shard)?;
            for suggestion in suggest::for_query(&reader, query) {
                // A term unknown to one shard may be held by another, and a
                // suggestion for a term some shard has is a correction of a
                // word that exists.
                if !out.iter().any(|s| s.term == suggestion.term) {
                    out.push(suggestion);
                }
            }
        }
        out.retain(|s| !self.holds(&s.term));
        Ok(out)
    }

    /// Whether any shard holds this term.
    fn holds(&self, term: &str) -> bool {
        self.manifest
            .all_shards()
            .iter()
            .any(|shard| self.reader(shard).is_ok_and(|r| r.contains(term)))
    }
}
