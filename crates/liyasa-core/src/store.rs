//! Persistence contracts (PRD §6.8, §34.9).
//!
//! `liyasa-store` re-exports these and is the only crate that opens a database.
//! Every repository follows one pattern: four methods from [`Repo`] plus the
//! entity-specific queries it needs.

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::diagnostics::Severity;
use crate::ids::{BuildId, ChunkId, FactId, IndexId, JobId, OrgId, PageId, ProjectId};
use crate::net::BoxFut;
pub use crate::verify::StoreError;

/// A page of results. `cursor` is opaque to the caller.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Page {
    pub cursor: Option<String>,
    pub limit: u32,
}

pub trait Entity: Send + Sync + 'static {
    type Id;
    type Query;
}

pub trait Repo<E: Entity>: Send + Sync {
    fn get<'a>(&'a self, id: &'a E::Id) -> BoxFut<'a, Result<Option<E>, StoreError>>;
    /// Insert or update, with an optimistic version check.
    fn put<'a>(&'a self, e: &'a E) -> BoxFut<'a, Result<(), StoreError>>;
    fn delete<'a>(&'a self, id: &'a E::Id) -> BoxFut<'a, Result<(), StoreError>>;
    fn list<'a>(&'a self, q: &'a E::Query, page: Page) -> BoxFut<'a, Result<Vec<E>, StoreError>>;
}

// ---- entities ----

macro_rules! entity {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
        )]
        #[non_exhaustive]
        pub struct $name;
    };
}

entity!(
    /// Fields land here as each server package needs them; the identity and the
    /// repository shape are what M0 freezes.
    Project
);
entity!(Build);
entity!(Deployment);
entity!(BlockRecord);
entity!(BlockLineage);
entity!(Claim);
entity!(Drift);
entity!(Job);
entity!(Event);
entity!(ChunkMeta);
entity!(ChunkFilter);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BuildStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Leased,
    Done,
    Failed,
    Dead,
}

/// The bounded ingest queue is full (ANA-08).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("event queue is full")]
pub struct QueueFull;

// ---- per-entity filters; every field optional, `Default` means "all" ----

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectQuery {
    pub org: Option<OrgId>,
    pub slug: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildQuery {
    pub project: Option<ProjectId>,
    pub env: Option<String>,
    pub status: Option<BuildStatus>,
    pub since: Option<SystemTime>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeploymentQuery {
    pub project: Option<ProjectId>,
    pub env: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockQuery {
    pub page: Option<PageId>,
    pub build: Option<BuildId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaimQuery {
    pub page: Option<PageId>,
    pub fact: Option<FactId>,
    pub confirmed: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DriftQuery {
    pub project: Option<ProjectId>,
    pub open: Option<bool>,
    pub severity: Option<Severity>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobQuery {
    pub name: Option<String>,
    pub state: Option<JobState>,
    pub project: Option<ProjectId>,
}

impl Entity for Project {
    type Id = ProjectId;
    type Query = ProjectQuery;
}
impl Entity for Build {
    type Id = BuildId;
    type Query = BuildQuery;
}
impl Entity for Deployment {
    type Id = BuildId;
    type Query = DeploymentQuery;
}
impl Entity for BlockRecord {
    type Id = crate::ids::BlockId;
    type Query = BlockQuery;
}
impl Entity for Claim {
    type Id = FactId;
    type Query = ClaimQuery;
}
impl Entity for Drift {
    type Id = JobId;
    type Query = DriftQuery;
}
impl Entity for Job {
    type Id = JobId;
    type Query = JobQuery;
}

// ---- repositories ----

pub trait ProjectRepo: Repo<Project> {}

pub trait BuildRepo: Repo<Build> {
    fn latest_for<'a>(
        &'a self,
        project: &'a ProjectId,
        env: &'a str,
    ) -> BoxFut<'a, Result<Option<Build>, StoreError>>;
}

pub trait DeploymentRepo: Repo<Deployment> {
    /// Points an environment at a build; the rollback primitive.
    fn point<'a>(&'a self, env: &'a str, build: &'a BuildId) -> BoxFut<'a, Result<(), StoreError>>;
}

pub trait BlockRepo: Repo<BlockRecord> {
    fn lineage<'a>(
        &'a self,
        from: &'a crate::ids::BlockId,
    ) -> BoxFut<'a, Result<Vec<BlockLineage>, StoreError>>;
}

pub trait ClaimRepo: Repo<Claim> {
    fn for_fact<'a>(&'a self, fact: &'a FactId) -> BoxFut<'a, Result<Vec<Claim>, StoreError>>;
}

pub trait DriftRepo: Repo<Drift> {
    fn open<'a>(&'a self, project: &'a ProjectId) -> BoxFut<'a, Result<Vec<Drift>, StoreError>>;
}

pub trait JobRepo: Repo<Job> {
    /// Leases the next runnable job to `worker` for `lease`.
    fn claim<'a>(
        &'a self,
        worker: &'a str,
        lease: Duration,
    ) -> BoxFut<'a, Result<Option<Job>, StoreError>>;
    fn heartbeat<'a>(&'a self, id: &'a JobId) -> BoxFut<'a, Result<(), StoreError>>;
}

pub trait EventSink: Send + Sync {
    fn push(&self, e: Event) -> Result<(), QueueFull>;
}

pub trait SecretStore: crate::verify::SecretSource {
    fn set<'a>(
        &'a self,
        name: &'a str,
        value: zeroize::Zeroizing<String>,
    ) -> BoxFut<'a, Result<(), StoreError>>;
}

pub trait VectorIndex: Send + Sync {
    fn upsert<'a>(
        &'a self,
        chunks: &'a [(ChunkId, Vec<f32>, ChunkMeta)],
    ) -> BoxFut<'a, Result<(), StoreError>>;
    fn query<'a>(
        &'a self,
        v: &'a [f32],
        k: usize,
        filter: &'a ChunkFilter,
    ) -> BoxFut<'a, Result<Vec<(ChunkId, f32)>, StoreError>>;
    /// Swaps the active index after a re-embedding run (AST-05).
    fn swap_active<'a>(&'a self, index: IndexId) -> BoxFut<'a, Result<(), StoreError>>;
}

pub trait Store: Send + Sync {
    fn projects(&self) -> &dyn ProjectRepo;
    fn builds(&self) -> &dyn BuildRepo;
    fn deployments(&self) -> &dyn DeploymentRepo;
    fn blocks(&self) -> &dyn BlockRepo;
    fn claims(&self) -> &dyn ClaimRepo;
    fn drift(&self) -> &dyn DriftRepo;
    fn jobs(&self) -> &dyn JobRepo;
    fn events(&self) -> &dyn EventSink;
    fn secrets(&self) -> &dyn SecretStore;
    fn vectors(&self) -> &dyn VectorIndex;
}
