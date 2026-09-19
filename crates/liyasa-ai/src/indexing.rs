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

/// The job the deployment enqueues.
///
/// **This must equal `liyasa_server::deploy::queue::EMBED_JOB`**, which is what
/// `queue_embedding` actually enqueues after a deploy. The two constants are
/// duplicated across a crate boundary that cannot be closed in either direction
/// — `liyasa-server` depends on this crate, so the name cannot live only there,
/// and `queue_embedding` is WP-16's — so the equality is pinned by a test in
/// `tests/` rather than by the type system. A rename on either side orphans the
/// job: it is warned about at startup and in readiness, and never fails.
pub const JOB_NAME: &str = "assistant.embed";

/// What the deploy queue puts on the row.
///
/// The field names are `queue_embedding`'s, not this crate's preference: it
/// sends `{ "buildId": ..., "project": ... }` and nothing else. `deployment`
/// reads that `buildId`, and `routes` defaults to empty because a fresh build
/// does not know which routes changed — and empty already means "the whole
/// site", which is the correct pass for a deploy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobPayload {
    pub project: String,
    #[serde(rename = "buildId", alias = "deployment")]
    pub deployment: String,
    #[serde(default)]
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
