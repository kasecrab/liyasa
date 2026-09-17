//! Chunk records and the vector index (AST-04).
//!
//! `liyasa_core::store::{ChunkMeta, ChunkFilter}` are fieldless marker structs,
//! so the frozen `VectorIndex` cannot carry what AST-01 stores or what AST-04
//! filters on. The typed records live here and the frozen trait is implemented
//! as a facade over them, the arrangement RFC 1400 reached for the store
//! entities; RFC 1801 records it for these two.
//!
//! Retrieval is two stages, and the split is the reason `k` is over-fetched.
//! The vector stage is nearest-neighbour over the whole active index; the
//! metadata stage — groups, regions, version, locale — is a second pass over
//! the candidates. A reader in one group would otherwise get `k` results of
//! which most are invisible to them and a short answer built from what is left,
//! so the first stage asks for [`OVERFETCH`] times as many.

pub mod sql;

use liyasa_core::ids::{ChunkId, IndexId, Locale, Route, Version};
use liyasa_core::net::BoxFut;
use serde::{Deserialize, Serialize};

use crate::config::ModelRef;

/// How many times `k` the vector stage fetches before the metadata stage runs
/// (AST-04).
pub const OVERFETCH: usize = 4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChunkKind {
    #[default]
    Prose,
    /// One OpenAPI operation as a structured document (AST-03).
    Operation,
}

impl ChunkKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prose => "prose",
            Self::Operation => "operation",
        }
    }
}

/// One row of the index. Proposed verbatim for `liyasa_core::store::ChunkMeta`
/// (RFC 1801).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkRecord {
    pub id: ChunkId,
    pub route: Route,
    /// The heading's anchor, `""` for the lead section. A citation is
    /// `route#anchor` (AST-12).
    pub anchor: String,
    pub title: String,
    pub breadcrumb: Vec<String>,
    pub version: Option<Version>,
    pub locale: Locale,
    /// Empty means "every reader"; a non-empty list means a reader needs one of
    /// them (AST-11).
    pub groups: Vec<String>,
    pub regions: Vec<String>,
    pub product: Option<String>,
    /// Milliseconds since the epoch; `None` when the page carries no
    /// verification (AST-15 down-weights on this).
    pub last_verified: Option<u64>,
    pub kind: ChunkKind,
    pub ordinal: u32,
    pub tokens: u32,
    /// `blake3:…` of the chunk text. What makes "only changed chunks are
    /// re-embedded" (AST-01) a comparison rather than a guess.
    pub content_hash: String,
    pub text: String,
}

impl ChunkRecord {
    /// A record with an id and nothing else, which is all the frozen
    /// `ChunkMeta` can supply through [`CoreFacade`] (RFC 1801).
    pub fn bare(id: ChunkId) -> Self {
        Self {
            id,
            route: Route::new(""),
            anchor: String::new(),
            title: String::new(),
            breadcrumb: Vec::new(),
            version: None,
            locale: Locale::new(""),
            groups: Vec::new(),
            regions: Vec::new(),
            product: None,
            last_verified: None,
            kind: ChunkKind::Prose,
            ordinal: 0,
            tokens: 0,
            content_hash: String::new(),
            text: String::new(),
        }
    }

    /// `<route>#<anchor>#<ordinal>`: stable across builds, so an unchanged
    /// chunk keeps its row and its vector.
    pub fn id_for(route: &Route, anchor: &str, ordinal: u32) -> ChunkId {
        ChunkId::new(format!("{}#{anchor}#{ordinal}", route.as_str()))
    }
}

/// The metadata stage (AST-04, AST-11). Every field is optional and `None`
/// means "do not filter on it".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChunkQuery {
    /// The reader's groups. A chunk with no groups is visible to everyone; a
    /// chunk with groups needs an intersection.
    pub groups: Vec<String>,
    /// The reader's region, if the deployment resolves one.
    pub region: Option<String>,
    pub version: Option<Version>,
    pub locale: Option<Locale>,
    pub kind: Option<ChunkKind>,
    /// Only these routes. Used by the `get_page` tool, not by retrieval.
    pub routes: Vec<Route>,
}

impl ChunkQuery {
    /// Whether a chunk survives the metadata stage.
    ///
    /// The group rule is the one that matters for AST-11: a reader never
    /// receives content they could not browse, so an entitled chunk needs an
    /// intersection with the reader's groups and an empty reader set never
    /// matches one.
    pub fn admits(&self, record: &ChunkRecord) -> bool {
        if !record.groups.is_empty() && !record.groups.iter().any(|g| self.groups.contains(g)) {
            return false;
        }
        if !record.regions.is_empty()
            && !self
                .region
                .as_ref()
                .is_some_and(|r| record.regions.contains(r))
        {
            return false;
        }
        if let Some(version) = &self.version
            && record.version.as_ref() != Some(version)
        {
            return false;
        }
        if let Some(locale) = &self.locale
            && &record.locale != locale
        {
            return false;
        }
        if let Some(kind) = self.kind
            && record.kind != kind
        {
            return false;
        }
        if !self.routes.is_empty() && !self.routes.contains(&record.route) {
            return false;
        }
        true
    }
}

/// Which storage an index lives in (AST-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// `vectors.db`, single node.
    SqliteVec,
    /// Postgres, required for replicas.
    PgVector,
}

/// What is recorded per index so a model or dimension change can be detected
/// (AST-04).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDescriptor {
    pub id: IndexId,
    pub model: ModelRef,
    pub dims: usize,
    pub backend: Backend,
    /// The extension version the rows were written with. `None` for
    /// [`Backend::PgVector`], where `pgvector`'s own version is the server's.
    pub extension_version: Option<String>,
    /// Milliseconds since the epoch.
    pub created_at: u64,
}

impl IndexDescriptor {
    /// Whether a new configuration needs a re-index rather than an incremental
    /// update (AST-04, AST-05).
    ///
    /// A dimension change needs a NEW TABLE, not a migration of rows in place,
    /// which is why the two are distinguished.
    pub fn change_from(&self, model: &ModelRef, dims: usize) -> Option<IndexChange> {
        if self.dims != dims {
            return Some(IndexChange::Dimension {
                from: self.dims,
                to: dims,
            });
        }
        if &self.model != model {
            return Some(IndexChange::Model {
                from: self.model.clone(),
                to: model.clone(),
            });
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexChange {
    Model { from: ModelRef, to: ModelRef },
    Dimension { from: usize, to: usize },
}

impl IndexChange {
    /// A dimension change cannot reuse the table; a model change at the same
    /// dimension still re-embeds every chunk but may write into the same shape.
    pub fn needs_new_table(&self) -> bool {
        matches!(self, Self::Dimension { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IndexError {
    #[error("this index embeds with {expected} dimensions; {found} were supplied")]
    Dimension { expected: usize, found: usize },
    #[error("no index is active")]
    NoActiveIndex,
    #[error("index `{0}` does not exist")]
    Unknown(IndexId),
    #[error("vector storage: {0}")]
    Storage(String),
}

/// The typed index the assistant calls (RFC 1801).
pub trait VectorStore: Send + Sync {
    fn active<'a>(&'a self) -> BoxFut<'a, Result<Option<IndexDescriptor>, IndexError>>;

    /// Creates the table for a new index and returns its descriptor.
    fn create<'a>(
        &'a self,
        model: &'a ModelRef,
        dims: usize,
    ) -> BoxFut<'a, Result<IndexDescriptor, IndexError>>;

    /// Writes chunks into `index`, which need not be the active one: AST-05
    /// fills the new index while the old one still answers.
    fn upsert<'a>(
        &'a self,
        index: &'a IndexId,
        chunks: &'a [(ChunkRecord, Vec<f32>)],
    ) -> BoxFut<'a, Result<(), IndexError>>;

    /// `(id, content_hash)` for every chunk of `route` in `index`, so a
    /// re-index embeds only what changed (AST-01).
    fn hashes<'a>(
        &'a self,
        index: &'a IndexId,
        route: &'a Route,
    ) -> BoxFut<'a, Result<Vec<(ChunkId, String)>, IndexError>>;

    fn delete<'a>(
        &'a self,
        index: &'a IndexId,
        ids: &'a [ChunkId],
    ) -> BoxFut<'a, Result<(), IndexError>>;

    /// Nearest neighbours from the ACTIVE index, over-fetched and then filtered.
    fn query<'a>(
        &'a self,
        v: &'a [f32],
        k: usize,
        filter: &'a ChunkQuery,
    ) -> BoxFut<'a, Result<Vec<Hit>, IndexError>>;

    /// One transaction: point at `index`, drop what it replaced (AST-05).
    fn swap_active<'a>(&'a self, index: &'a IndexId) -> BoxFut<'a, Result<(), IndexError>>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub record: ChunkRecord,
    /// Cosine similarity in `0.0..=1.0`, higher is nearer.
    pub score: f32,
}

/// Cosine similarity. Returns 0 for a zero vector rather than a NaN, because a
/// NaN would sort unpredictably and silently reorder results.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

pub mod memory;

pub use memory::MemoryStore;

/// The frozen [`liyasa_core::store::VectorIndex`] over a [`VectorStore`]
/// (RFC 1801).
///
/// `ChunkMeta` and `ChunkFilter` carry no fields, so going through this facade
/// stores a chunk with an EMPTY record and queries with NO filter. That is
/// written down rather than hidden: a consumer holding a `&dyn Store` keeps
/// compiling, and anything that needs the metadata calls [`VectorStore`]
/// directly. The assistant never uses the facade.
pub struct CoreFacade<S> {
    inner: S,
}

impl<S: VectorStore> CoreFacade<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &S {
        &self.inner
    }
}

impl<S: VectorStore> liyasa_core::store::VectorIndex for CoreFacade<S> {
    fn upsert<'a>(
        &'a self,
        chunks: &'a [(ChunkId, Vec<f32>, liyasa_core::store::ChunkMeta)],
    ) -> BoxFut<'a, Result<(), liyasa_core::store::StoreError>> {
        Box::pin(async move {
            let Some(active) = self
                .inner
                .active()
                .await
                .map_err(|e| liyasa_core::store::StoreError::Io(e.to_string()))?
            else {
                return Err(liyasa_core::store::StoreError::Io(
                    "no index is active".to_owned(),
                ));
            };
            let rows: Vec<(ChunkRecord, Vec<f32>)> = chunks
                .iter()
                .map(|(id, vector, _)| (ChunkRecord::bare(id.clone()), vector.clone()))
                .collect();
            self.inner
                .upsert(&active.id, &rows)
                .await
                .map_err(|e| liyasa_core::store::StoreError::Io(e.to_string()))
        })
    }

    fn query<'a>(
        &'a self,
        v: &'a [f32],
        k: usize,
        _filter: &'a liyasa_core::store::ChunkFilter,
    ) -> BoxFut<'a, Result<Vec<(ChunkId, f32)>, liyasa_core::store::StoreError>> {
        Box::pin(async move {
            let hits = self
                .inner
                .query(v, k, &ChunkQuery::default())
                .await
                .map_err(|e| liyasa_core::store::StoreError::Io(e.to_string()))?;
            Ok(hits
                .into_iter()
                .map(|hit| (hit.record.id, hit.score))
                .collect())
        })
    }

    fn swap_active<'a>(
        &'a self,
        index: IndexId,
    ) -> BoxFut<'a, Result<(), liyasa_core::store::StoreError>> {
        Box::pin(async move {
            self.inner
                .swap_active(&index)
                .await
                .map_err(|e| liyasa_core::store::StoreError::Io(e.to_string()))
        })
    }
}
