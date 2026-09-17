//! The browser search worker (§12.2, SRC-05).
//!
//! The worker fetches `manifest.json` first, then a shard's term dictionary and
//! the ranking prefix of its `docs-<n>.bin`, then postings as the query needs
//! them. [`Searcher`] holds whatever has arrived and answers over that, so a
//! first result does not wait for the whole index.
//!
//! Scoring statistics come from the manifest, never from a shard, which is what
//! makes scores comparable across shards.

use std::collections::BTreeMap;

use liyasa_core::diagnostics::Diagnostics;
use liyasa_search::error::SearchError;
use liyasa_search::idx::manifest::{Context, Manifest, Shard};
use liyasa_search::idx::query::{self, ReaderScope};
use liyasa_search::idx::reader::{ShardBytes, ShardReader};
use liyasa_search::idx::score::{self, Stats};
use liyasa_search::idx::search::{self, Hit, SearchOptions};
use liyasa_search::idx::writer::MANIFEST;

use crate::api::{Highlight, SearchHit, SearchRequest, SearchResponse, Snippet};

pub struct Searcher {
    manifest: Manifest,
    files: BTreeMap<String, Vec<u8>>,
}

impl std::fmt::Debug for Searcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Searcher")
            .field("documents", &self.manifest.documents)
            .field("shards", &self.manifest.shards.len())
            .field("files", &self.files.len())
            .finish()
    }
}

impl Searcher {
    /// Opens over what the worker has fetched so far. `manifest.json` must be
    /// among it; everything else arrives later.
    pub fn open(files: BTreeMap<String, Vec<u8>>) -> Result<Self, Diagnostics> {
        let refused = |error: SearchError| {
            let mut diagnostics = Diagnostics::new();
            diagnostics.push(error.diagnostic());
            diagnostics
        };
        let json = files
            .get(MANIFEST)
            .ok_or_else(|| refused(SearchError::corrupt(MANIFEST)))?;
        let manifest: Manifest =
            serde_json::from_slice(json).map_err(|_| refused(SearchError::corrupt(MANIFEST)))?;
        ShardReader::check_version(manifest.version).map_err(refused)?;
        Ok(Self { manifest, files })
    }

    /// Adds a file the worker fetched after the session opened.
    pub fn add_file(&mut self, name: impl Into<String>, bytes: Vec<u8>) {
        self.files.insert(name.into(), bytes);
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn search(&self, request: &SearchRequest) -> SearchResponse {
        let locale = request.locale.as_deref().unwrap_or("en");
        let query = match query::parse(&request.query, locale) {
            Ok(query) => query,
            Err(error) => {
                return SearchResponse {
                    hits: Vec::new(),
                    diagnostics: vec![error.diagnostic()],
                };
            }
        };
        let context = Context {
            locale: request.locale.clone(),
            version: request.version.clone(),
            tab: request.tab.clone(),
        };
        let mut options = SearchOptions {
            reader: ReaderScope {
                groups: request.reader_groups.clone(),
                region: request.reader_region.clone(),
            },
            ..SearchOptions::default()
        };
        if let Some(max) = request.max_results {
            options.max_results = max as usize;
        }
        if let Some(snippets) = request.snippets {
            options.snippets = snippets;
        }

        let stats = Stats {
            documents: self.manifest.documents,
            average_length: self.manifest.average_length(),
            weights: self.manifest.weights(),
        };
        let chosen: Vec<&Shard> = if context == Context::default() {
            self.manifest.all_shards().iter().collect()
        } else {
            self.manifest.shards_for(&context)
        };

        let mut hits = Vec::new();
        let mut diagnostics = Vec::new();
        for shard in chosen {
            // A shard whose files have not arrived is skipped, not an error:
            // the worker is still fetching and the next keystroke asks again.
            let Some(bytes) = self.bytes(shard) else {
                continue;
            };
            let reader = match ShardReader::open(bytes) {
                Ok(reader) => reader,
                Err(error) => {
                    diagnostics.push(error.diagnostic());
                    continue;
                }
            };
            match search::search(&reader, &query, &stats, &self.manifest.idf, &options) {
                Ok(found) => hits.extend(found),
                Err(error) => diagnostics.push(error.diagnostic()),
            }
        }
        score::rank(&mut hits);
        hits.truncate(options.max_results);

        SearchResponse {
            hits: hits.iter().map(hit).collect(),
            diagnostics,
        }
    }

    fn bytes(&self, shard: &Shard) -> Option<ShardBytes<'_>> {
        let of = |name: &str| self.files.get(name).map(Vec::as_slice);
        Some(ShardBytes {
            terms: of(&shard.files.terms)?,
            postings: of(&shard.files.postings)?,
            docs: of(&shard.files.docs)?,
            snippets: of(&shard.files.snippets)?,
        })
    }
}

fn hit(hit: &Hit) -> SearchHit {
    SearchHit {
        url: hit.url.clone(),
        route: hit.route.clone(),
        anchor: hit.anchor.clone(),
        title: hit.title.clone(),
        section: hit.section.clone(),
        breadcrumb: hit.breadcrumb.clone(),
        kind: hit.kind.as_str().to_owned(),
        tab: hit.tab.clone(),
        version: hit.version.clone(),
        locale: hit.locale.clone(),
        score: hit.score,
        matched: hit.matched as u32,
        snippet: hit.snippet.as_ref().map(|snippet| Snippet {
            text: snippet.text.clone(),
            highlights: snippet
                .highlights
                .iter()
                .map(|range| Highlight {
                    start: range.start,
                    end: range.end,
                })
                .collect(),
        }),
    }
}
