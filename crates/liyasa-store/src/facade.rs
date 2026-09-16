//! `SqliteStore`: the frozen `Store` contract over the typed repositories
//! (PRD §34.9, RFC 1400).
//!
//! `liyasa-core`'s entity structs carry no fields, so a consumer holding
//! `&dyn Store` can ask whether a row exists, delete it, count what matches a
//! query, and drive the job queue. Everything that needs a field goes through
//! the typed API on this struct (`projects_typed()`, `jobs_typed()`, and the
//! rest), which is what the server uses.

use std::path::Path;
use std::time::Duration;

use liyasa_core::ids::{BlockId, BuildId, FactId, JobId, ProjectId};
use liyasa_core::net::BoxFut;
use liyasa_core::store::{
    BlockLineage, BlockQuery, BlockRecord, BlockRepo, Build, BuildQuery, BuildRepo, Claim,
    ClaimQuery, ClaimRepo, Deployment, DeploymentQuery, DeploymentRepo, Drift, DriftQuery,
    DriftRepo, EventSink, Job, JobQuery, JobRepo, Page, Project, ProjectQuery, ProjectRepo, Repo,
    SecretStore, Store, StoreError, VectorIndex,
};
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::db::{self, OpenOptions, sql_error};
use crate::ingest::IngestQueue;
use crate::jobs::Jobs;
use crate::repos::{Builds, Deployments, Domains, Feedback, Projects, Webhooks};
use crate::secrets::{MasterKey, Secrets};

/// The message every field-carrying write gets until the entity structs have
/// fields. Reaching it is a bug in the caller, not a database failure: the
/// typed API is right there.
fn no_fields(entity: &str) -> StoreError {
    StoreError::Io(format!(
        "`{entity}` carries no fields; write it through the typed API (RFC 1400)"
    ))
}

pub struct SqliteStore {
    app: SqlitePool,
    projects: ProjectShim,
    builds: BuildShim,
    deployments: DeploymentShim,
    blocks: BlockShim,
    claims: ClaimShim,
    drift: DriftShim,
    jobs: JobShim,
    events: IngestQueue,
    secrets: Secrets,
    vectors: NoVectorIndex,
    domains: Domains,
    feedback: Feedback,
    webhooks: Webhooks,
}

impl std::fmt::Debug for SqliteStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteStore").finish_non_exhaustive()
    }
}

impl SqliteStore {
    /// Opens `liyasa.db` at `path`, migrates it, and decrypts the secrets.
    pub async fn open(
        path: &Path,
        key: MasterKey,
        events: IngestQueue,
    ) -> Result<Self, StoreError> {
        let app = db::open(path, &OpenOptions::default(), db::APP).await?;
        Self::from_pool(app, key, events).await
    }

    pub async fn from_pool(
        app: SqlitePool,
        key: MasterKey,
        events: IngestQueue,
    ) -> Result<Self, StoreError> {
        let secrets = Secrets::open(app.clone(), key).await?;
        Ok(Self {
            projects: ProjectShim(Projects::new(app.clone())),
            builds: BuildShim(Builds::new(app.clone())),
            deployments: DeploymentShim(Deployments::new(app.clone())),
            blocks: BlockShim,
            claims: ClaimShim,
            drift: DriftShim,
            jobs: JobShim(Jobs::new(app.clone())),
            domains: Domains::new(app.clone()),
            feedback: Feedback::new(app.clone()),
            webhooks: Webhooks::new(app.clone()),
            events,
            secrets,
            vectors: NoVectorIndex,
            app,
        })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.app
    }

    pub fn projects_typed(&self) -> &Projects {
        &self.projects.0
    }

    pub fn builds_typed(&self) -> &Builds {
        &self.builds.0
    }

    pub fn deployments_typed(&self) -> &Deployments {
        &self.deployments.0
    }

    pub fn jobs_typed(&self) -> &Jobs {
        &self.jobs.0
    }

    pub fn domains(&self) -> &Domains {
        &self.domains
    }

    pub fn feedback(&self) -> &Feedback {
        &self.feedback
    }

    pub fn webhooks(&self) -> &Webhooks {
        &self.webhooks
    }

    pub fn secrets_typed(&self) -> &Secrets {
        &self.secrets
    }

    pub fn ingest(&self) -> &IngestQueue {
        &self.events
    }
}

impl Store for SqliteStore {
    fn projects(&self) -> &dyn ProjectRepo {
        &self.projects
    }
    fn builds(&self) -> &dyn BuildRepo {
        &self.builds
    }
    fn deployments(&self) -> &dyn DeploymentRepo {
        &self.deployments
    }
    fn blocks(&self) -> &dyn BlockRepo {
        &self.blocks
    }
    fn claims(&self) -> &dyn ClaimRepo {
        &self.claims
    }
    fn drift(&self) -> &dyn DriftRepo {
        &self.drift
    }
    fn jobs(&self) -> &dyn JobRepo {
        &self.jobs
    }
    fn events(&self) -> &dyn EventSink {
        &self.events
    }
    fn secrets(&self) -> &dyn SecretStore {
        &self.secrets
    }
    fn vectors(&self) -> &dyn VectorIndex {
        &self.vectors
    }
}

// ---- shims: existence, deletion and counting over the typed repositories ----

struct ProjectShim(Projects);

impl Repo<Project> for ProjectShim {
    fn get<'a>(&'a self, id: &'a ProjectId) -> BoxFut<'a, Result<Option<Project>, StoreError>> {
        Box::pin(async move { Ok(self.0.get(id).await?.map(|_| Project::default())) })
    }

    fn put<'a>(&'a self, _e: &'a Project) -> BoxFut<'a, Result<(), StoreError>> {
        // TODO(rfc-1400)
        Box::pin(async move { Err(no_fields("Project")) })
    }

    fn delete<'a>(&'a self, id: &'a ProjectId) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(self.0.delete(id))
    }

    fn list<'a>(
        &'a self,
        q: &'a ProjectQuery,
        page: Page,
    ) -> BoxFut<'a, Result<Vec<Project>, StoreError>> {
        Box::pin(async move {
            let rows = self.0.list(q.slug.as_deref(), &page).await?;
            Ok(rows.iter().map(|_| Project::default()).collect())
        })
    }
}

impl ProjectRepo for ProjectShim {}

struct BuildShim(Builds);

impl Repo<Build> for BuildShim {
    fn get<'a>(&'a self, id: &'a BuildId) -> BoxFut<'a, Result<Option<Build>, StoreError>> {
        Box::pin(async move { Ok(self.0.get(id).await?.map(|_| Build::default())) })
    }

    fn put<'a>(&'a self, _e: &'a Build) -> BoxFut<'a, Result<(), StoreError>> {
        // TODO(rfc-1400)
        Box::pin(async move { Err(no_fields("Build")) })
    }

    fn delete<'a>(&'a self, id: &'a BuildId) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(self.0.delete(id))
    }

    fn list<'a>(
        &'a self,
        q: &'a BuildQuery,
        page: Page,
    ) -> BoxFut<'a, Result<Vec<Build>, StoreError>> {
        Box::pin(async move {
            let rows = self
                .0
                .list(q.project.as_ref(), q.env.as_deref(), q.status, &page)
                .await?;
            Ok(rows.iter().map(|_| Build::default()).collect())
        })
    }
}

impl BuildRepo for BuildShim {
    fn latest_for<'a>(
        &'a self,
        project: &'a ProjectId,
        env: &'a str,
    ) -> BoxFut<'a, Result<Option<Build>, StoreError>> {
        Box::pin(async move {
            Ok(self
                .0
                .latest_for(project, env)
                .await?
                .map(|_| Build::default()))
        })
    }
}

struct DeploymentShim(Deployments);

/// A deployment is keyed by environment, and the frozen `Entity` impl gives it
/// a `BuildId`, so the marker API addresses it by the build it points at.
impl Repo<Deployment> for DeploymentShim {
    fn get<'a>(&'a self, id: &'a BuildId) -> BoxFut<'a, Result<Option<Deployment>, StoreError>> {
        Box::pin(async move {
            let rows = self.0.list(None, None).await?;
            Ok(rows
                .iter()
                .any(|d| &d.build == id)
                .then(Deployment::default))
        })
    }

    fn put<'a>(&'a self, _e: &'a Deployment) -> BoxFut<'a, Result<(), StoreError>> {
        // TODO(rfc-1400)
        Box::pin(async move { Err(no_fields("Deployment")) })
    }

    fn delete<'a>(&'a self, id: &'a BuildId) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async move {
            for deployment in self.0.list(None, None).await? {
                if &deployment.build == id {
                    self.0.delete(&deployment.project, &deployment.env).await?;
                }
            }
            Ok(())
        })
    }

    fn list<'a>(
        &'a self,
        q: &'a DeploymentQuery,
        _page: Page,
    ) -> BoxFut<'a, Result<Vec<Deployment>, StoreError>> {
        Box::pin(async move {
            let rows = self.0.list(q.project.as_ref(), q.env.as_deref()).await?;
            Ok(rows.iter().map(|_| Deployment::default()).collect())
        })
    }
}

impl DeploymentRepo for DeploymentShim {
    fn point<'a>(&'a self, env: &'a str, build: &'a BuildId) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async move {
            let project = sqlx::query("SELECT project_id FROM build WHERE id = ?")
                .bind(build.to_string())
                .fetch_optional(self.0.pool())
                .await
                .map_err(sql_error)?
                .ok_or(StoreError::NotFound)?;
            let project: String = project.try_get("project_id").map_err(sql_error)?;
            let project = ProjectId::parse(&project).ok_or(StoreError::NotFound)?;
            self.0.point(&project, env, build).await
        })
    }
}

/// The truth-graph tables (`block`, `block_lineage`, `claim`, `drift`) are
/// added by the packages that own those entities' shapes, as their own
/// numbered migrations. Until then these repositories are empty rather than
/// guessing a schema (RFC 1400).
struct BlockShim;

impl Repo<BlockRecord> for BlockShim {
    fn get<'a>(&'a self, _id: &'a BlockId) -> BoxFut<'a, Result<Option<BlockRecord>, StoreError>> {
        Box::pin(async move { Ok(None) })
    }
    fn put<'a>(&'a self, _e: &'a BlockRecord) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async move { Err(no_fields("BlockRecord")) })
    }
    fn delete<'a>(&'a self, _id: &'a BlockId) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async move { Ok(()) })
    }
    fn list<'a>(
        &'a self,
        _q: &'a BlockQuery,
        _page: Page,
    ) -> BoxFut<'a, Result<Vec<BlockRecord>, StoreError>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
}

impl BlockRepo for BlockShim {
    fn lineage<'a>(
        &'a self,
        _from: &'a BlockId,
    ) -> BoxFut<'a, Result<Vec<BlockLineage>, StoreError>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
}

struct ClaimShim;

impl Repo<Claim> for ClaimShim {
    fn get<'a>(&'a self, _id: &'a FactId) -> BoxFut<'a, Result<Option<Claim>, StoreError>> {
        Box::pin(async move { Ok(None) })
    }
    fn put<'a>(&'a self, _e: &'a Claim) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async move { Err(no_fields("Claim")) })
    }
    fn delete<'a>(&'a self, _id: &'a FactId) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async move { Ok(()) })
    }
    fn list<'a>(
        &'a self,
        _q: &'a ClaimQuery,
        _page: Page,
    ) -> BoxFut<'a, Result<Vec<Claim>, StoreError>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
}

impl ClaimRepo for ClaimShim {
    fn for_fact<'a>(&'a self, _fact: &'a FactId) -> BoxFut<'a, Result<Vec<Claim>, StoreError>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
}

struct DriftShim;

impl Repo<Drift> for DriftShim {
    fn get<'a>(&'a self, _id: &'a JobId) -> BoxFut<'a, Result<Option<Drift>, StoreError>> {
        Box::pin(async move { Ok(None) })
    }
    fn put<'a>(&'a self, _e: &'a Drift) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async move { Err(no_fields("Drift")) })
    }
    fn delete<'a>(&'a self, _id: &'a JobId) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async move { Ok(()) })
    }
    fn list<'a>(
        &'a self,
        _q: &'a DriftQuery,
        _page: Page,
    ) -> BoxFut<'a, Result<Vec<Drift>, StoreError>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
}

impl DriftRepo for DriftShim {
    fn open<'a>(&'a self, _project: &'a ProjectId) -> BoxFut<'a, Result<Vec<Drift>, StoreError>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
}

struct JobShim(Jobs);

impl Repo<Job> for JobShim {
    fn get<'a>(&'a self, id: &'a JobId) -> BoxFut<'a, Result<Option<Job>, StoreError>> {
        Box::pin(async move { Ok(self.0.get(id).await?.map(|_| Job::default())) })
    }

    fn put<'a>(&'a self, _e: &'a Job) -> BoxFut<'a, Result<(), StoreError>> {
        // TODO(rfc-1400)
        Box::pin(async move { Err(no_fields("Job")) })
    }

    fn delete<'a>(&'a self, id: &'a JobId) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(self.0.delete(id))
    }

    fn list<'a>(&'a self, q: &'a JobQuery, page: Page) -> BoxFut<'a, Result<Vec<Job>, StoreError>> {
        Box::pin(async move {
            let rows = self.0.list(q, page).await?;
            Ok(rows.iter().map(|_| Job::default()).collect())
        })
    }
}

impl JobRepo for JobShim {
    fn claim<'a>(
        &'a self,
        worker: &'a str,
        lease: Duration,
    ) -> BoxFut<'a, Result<Option<Job>, StoreError>> {
        Box::pin(async move { Ok(self.0.claim(worker, lease).await?.map(|_| Job::default())) })
    }

    fn heartbeat<'a>(&'a self, id: &'a JobId) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(self.0.heartbeat(id))
    }
}

/// There is no in-process vector index in any topology (§6.8): vectors live in
/// `vectors.db` through `sqlite-vec`, which the assistant package opens.
struct NoVectorIndex;

impl VectorIndex for NoVectorIndex {
    fn upsert<'a>(
        &'a self,
        _chunks: &'a [(
            liyasa_core::ids::ChunkId,
            Vec<f32>,
            liyasa_core::store::ChunkMeta,
        )],
    ) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async move { Err(unconfigured()) })
    }

    fn query<'a>(
        &'a self,
        _v: &'a [f32],
        _k: usize,
        _filter: &'a liyasa_core::store::ChunkFilter,
    ) -> BoxFut<'a, Result<Vec<(liyasa_core::ids::ChunkId, f32)>, StoreError>> {
        Box::pin(async move { Err(unconfigured()) })
    }

    fn swap_active<'a>(
        &'a self,
        _index: liyasa_core::ids::IndexId,
    ) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async move { Err(unconfigured()) })
    }
}

fn unconfigured() -> StoreError {
    StoreError::Io("no vector index is configured on this instance".to_owned())
}
