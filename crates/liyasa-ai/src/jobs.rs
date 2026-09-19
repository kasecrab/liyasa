//! The two jobs the server registers for this package (RFC 1404, defect 130).
//!
//! RFC 1404 splits a job in two: the owning crate exports the work as a plain
//! function over its own types, and `liyasa-server` holds a thin `run` that
//! builds those types from `AppState` and calls it. `run` cannot live here —
//! its signature names `AppState` and `JobRecord`, and `liyasa-server` depends
//! on this crate rather than the other way round.
//!
//! So this module is the half that can live here, and it is deliberately the
//! larger half. Without it a server-side `run_index` would orchestrate
//! chunking, the changed-chunk delta, embedding, withdrawal and the atomic
//! swap itself — which is neither thin nor in a file this package could later
//! fix.

use liyasa_core::document::Document;
use liyasa_core::frontmatter::FrontmatterFields;
use liyasa_core::ids::{IndexId, Route};
use liyasa_core::net::BoxFut;

use crate::chunk::{ChunkOptions, PageContext};
use crate::config::AiConfig;
use crate::exclude::{Environment, Excluded, PageFacts};
use crate::index::{ChunkRecord, IndexError, VectorStore};
use crate::indexing::{JobPayload, records_for};
use crate::privacy::{PrivacyConfig, StoredExchange};
use crate::reindex::{Embedder, ReindexError};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum JobError {
    #[error("{0}")]
    Index(#[from] IndexError),
    #[error("{0}")]
    Reindex(#[from] ReindexError),
    /// The seam the server supplies failed. Carried as a string because what
    /// can go wrong there belongs to the server's types, not to this crate's.
    #[error("{0}")]
    Source(String),
}

// ---- index on publish (AST-01) ----

/// One page as the indexer sees it, with everything AST-02's rules need.
pub struct IndexablePage {
    pub context: PageContext,
    pub front: FrontmatterFields,
    pub draft: bool,
    pub ignored: bool,
    pub ai_ignored: bool,
    pub document: Document,
}

/// Where the indexer gets pages. The build owns them; this is the seam.
pub trait PageSource: Send + Sync {
    /// The pages for `routes`, or every indexable page when `routes` is empty.
    ///
    /// A route that was ASKED FOR and is not returned is one the deployment
    /// removed, and its rows are withdrawn. That is why this returns the pages
    /// rather than taking a callback: the absence is information.
    fn pages<'a>(
        &'a self,
        routes: &'a [Route],
    ) -> BoxFut<'a, Result<Vec<IndexablePage>, JobError>>;
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct IndexReport {
    pub embedded: usize,
    pub unchanged: usize,
    pub deleted: usize,
    /// Pages that produced nothing, and why (AST-02). Carried rather than
    /// counted so the dashboard can answer "why is this page not answerable".
    pub excluded: Vec<(Route, Excluded)>,
    /// Routes asked for that the deployment no longer has.
    pub withdrawn: Vec<Route>,
}

pub struct IndexJob<'a> {
    pub store: &'a dyn VectorStore,
    pub index: &'a IndexId,
    pub pages: &'a dyn PageSource,
    pub config: &'a AiConfig,
    pub env: Environment,
    pub options: ChunkOptions,
    pub embedder: Embedder<'a>,
}

impl IndexJob<'_> {
    /// Runs one index pass.
    ///
    /// Writes into `index`, which during a full re-index is not the active one:
    /// the old index answers every query until the caller swaps (AST-05).
    pub async fn run(&self, payload: &JobPayload) -> Result<IndexReport, JobError> {
        let pages = self.pages.pages(&payload.routes).await?;
        let mut report = IndexReport::default();
        let mut produced_routes = Vec::with_capacity(pages.len());

        for page in &pages {
            produced_routes.push(page.context.route.clone());
            let facts = PageFacts {
                front: &page.front,
                draft: page.draft,
                ignored: page.ignored,
                ai_ignored: page.ai_ignored,
            };
            let input = crate::indexing::PageInput {
                facts,
                context: page.context.clone(),
                document: &page.document,
            };
            match records_for(&input, self.env, self.config, &self.options) {
                Ok(records) => {
                    self.apply(&page.context.route, records, &mut report).await?;
                }
                Err(reason) => {
                    // A page that STOPS being indexable has to lose its rows.
                    // Leaving them is the failure this branch exists for: the
                    // page disappears from the site and keeps answering.
                    self.apply(&page.context.route, Vec::new(), &mut report).await?;
                    report.excluded.push((page.context.route.clone(), reason));
                }
            }
        }

        // A route asked for and not returned is one the deployment removed.
        // Only meaningful for a targeted pass: an empty `routes` means "every
        // page", and this crate cannot enumerate what the index holds to
        // diff against it. See `run`'s note on WHOLE-SITE DELETIONS below.
        for route in &payload.routes {
            if !produced_routes.contains(route) {
                let ids = crate::indexing::withdraw(self.store, self.index, route).await?;
                if !ids.is_empty() {
                    self.store.delete(self.index, &ids).await?;
                    report.deleted += ids.len();
                }
                report.withdrawn.push(route.clone());
            }
        }

        Ok(report)
    }

    /// One page's delta, embedded and applied.
    async fn apply(
        &self,
        route: &Route,
        produced: Vec<ChunkRecord>,
        report: &mut IndexReport,
    ) -> Result<(), JobError> {
        let delta = crate::indexing::page_delta(self.store, self.index, route, produced).await?;
        report.unchanged += delta.unchanged;
        if !delta.delete.is_empty() {
            self.store.delete(self.index, &delta.delete).await?;
            report.deleted += delta.delete.len();
        }
        if !delta.embed.is_empty() {
            let embedded = self.embedder.run(&delta.embed, 0).await?;
            report.embedded += embedded;
        }
        Ok(())
    }
}

// ---- retention sweep (AST-22) ----

/// Where stored transcripts live. The server owns the table; this is the seam.
pub trait TranscriptStore: Send + Sync {
    /// Every stored exchange at or before `cutoff`. The filter is the store's
    /// so a retention pass does not load the whole table to find the tail.
    fn expired_before<'a>(
        &'a self,
        cutoff: u64,
    ) -> BoxFut<'a, Result<Vec<StoredExchange>, JobError>>;

    /// Deletes those rows. Returns how many went.
    fn delete<'a>(&'a self, rows: &'a [StoredExchange]) -> BoxFut<'a, Result<u64, JobError>>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetentionReport {
    pub deleted: u64,
    /// What the store was asked for, recorded so an operator can tell a sweep
    /// that found nothing from one that was never configured to find anything.
    pub cutoff: u64,
}

pub struct RetentionJob<'a> {
    pub transcripts: &'a dyn TranscriptStore,
    pub config: &'a PrivacyConfig,
}

impl RetentionJob<'_> {
    pub async fn run(&self, now_ms: u64) -> Result<RetentionReport, JobError> {
        let cutoff = crate::privacy::cutoff(now_ms, self.config);
        let rows = self.transcripts.expired_before(cutoff).await?;
        let deleted = if rows.is_empty() {
            0
        } else {
            self.transcripts.delete(&rows).await?
        };
        Ok(RetentionReport { deleted, cutoff })
    }
}
