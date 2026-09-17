//! Index on publish (AST-01): one deployment's pages into chunk records.
//!
//! The job itself is enqueued and leased by the server's job store, which is
//! what makes it at-most-once per cluster; this module is the work the job
//! does, so it can be tested without one. [`JOB_NAME`] and [`JobPayload`] are
//! the contract between the two.

use liyasa_core::document::Document;
use liyasa_core::ids::{ChunkId, Route};
use serde::{Deserialize, Serialize};

use crate::chunk::{Chunk, ChunkOptions, PageContext, chunk};
use crate::config::AiConfig;
use crate::exclude::{Environment, Excluded, PageFacts, exclusion};
use crate::index::{ChunkKind, ChunkRecord, IndexError, VectorStore};
use crate::reindex::{Delta, delta};

/// The job the deployment enqueues. Named here so the server and this crate
/// cannot disagree about it.
pub const JOB_NAME: &str = "assistant.index";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobPayload {
    pub project: String,
    pub deployment: String,
    /// The routes this deployment changed. Empty means the whole site, which is
    /// what a first publish and a re-index both send.
    pub routes: Vec<Route>,
}

/// One page, as the indexer receives it.
pub struct PageInput<'a> {
    pub facts: PageFacts<'a>,
    pub context: PageContext,
    pub document: &'a Document,
}

/// The records a page produces, or why it produced none.
pub fn records_for(
    page: &PageInput<'_>,
    env: Environment,
    config: &AiConfig,
    options: &ChunkOptions,
) -> Result<Vec<ChunkRecord>, Excluded> {
    if let Some(reason) = exclusion(&page.facts, env, config) {
        return Err(reason);
    }
    Ok(chunk(page.document, &page.context.title, options)
        .iter()
        .map(|c| ChunkRecord::from_chunk(c, &page.context, ChunkKind::Prose))
        .collect())
}

/// Records from chunks another producer made — an OpenAPI operation, or a
/// caller that chunked elsewhere.
pub fn records_from(chunks: &[Chunk], context: &PageContext, kind: ChunkKind) -> Vec<ChunkRecord> {
    chunks
        .iter()
        .map(|c| ChunkRecord::from_chunk(c, context, kind))
        .collect()
}

/// What one page's pass must embed and delete, against what the index holds.
pub async fn page_delta(
    store: &dyn VectorStore,
    index: &liyasa_core::ids::IndexId,
    route: &Route,
    produced: Vec<ChunkRecord>,
) -> Result<Delta, IndexError> {
    let held = store.hashes(index, route).await?;
    Ok(delta(&held, produced))
}

/// The rows of a page that is no longer indexable: everything the index holds
/// for it.
///
/// A page that turns `hidden: true`, or gains an `ai: false`, has to LOSE its
/// rows. Leaving them is the failure this function exists for: the page
/// disappears from the site and keeps answering questions.
pub async fn withdraw(
    store: &dyn VectorStore,
    index: &liyasa_core::ids::IndexId,
    route: &Route,
) -> Result<Vec<ChunkId>, IndexError> {
    Ok(store
        .hashes(index, route)
        .await?
        .into_iter()
        .map(|(id, _)| id)
        .collect())
}
