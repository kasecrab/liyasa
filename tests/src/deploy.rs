//! A `liyasa serve` with the deploy routes merged on (WP-16).
//!
//! `routes::router` is WP-14's and does not know about `deploy::routes`, so
//! the two are merged here exactly as `liyasa serve` will merge them once that
//! one line lands. Everything else is the server the other acceptance tests
//! drive.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use http::{Request, Response};
use liyasa_core::ids::{BuildId, Fingerprint, ProjectId};
use liyasa_core::net::BoxFut;
use liyasa_core::store::BuildStatus;
use liyasa_server::deploy::rollback::{AuditEntry, CdnPurge, PurgeError};
use liyasa_server::deploy::service::DeployState;
use liyasa_server::routes::{AppState, ServerConfig};
use liyasa_store::records::BuildRecord;
use liyasa_store::{IngestQueue, MasterKey, SqliteStore};
use tower::ServiceExt as _;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Records every purge so a test can assert the tag rather than the mechanism.
#[derive(Debug, Default)]
pub struct RecordingPurge {
    tags: Mutex<Vec<String>>,
    fail: bool,
}

impl RecordingPurge {
    pub fn failing() -> Self {
        Self {
            tags: Mutex::new(Vec::new()),
            fail: true,
        }
    }

    pub fn tags(&self) -> Vec<String> {
        self.tags
            .lock()
            .expect("the recorder is not poisoned")
            .clone()
    }
}

impl CdnPurge for RecordingPurge {
    fn purge_tag<'a>(&'a self, tag: &'a str) -> BoxFut<'a, Result<(), PurgeError>> {
        self.tags
            .lock()
            .expect("the recorder is not poisoned")
            .push(tag.to_owned());
        let fail = self.fail;
        Box::pin(async move {
            match fail {
                true => Err(PurgeError::Failed("the edge refused".to_owned())),
                false => Ok(()),
            }
        })
    }
}

#[derive(Debug, Default)]
pub struct RecordingAudit(Mutex<Vec<AuditEntry>>);

impl RecordingAudit {
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.0.lock().expect("the recorder is not poisoned").clone()
    }
}

impl liyasa_server::deploy::rollback::Audit for RecordingAudit {
    fn record(&self, entry: &AuditEntry) {
        if let Ok(mut entries) = self.0.lock() {
            entries.push(entry.clone());
        }
    }
}

pub struct Harness {
    pub state: Arc<AppState>,
    pub store: Arc<SqliteStore>,
    pub router: Router,
    pub project: ProjectId,
    root: PathBuf,
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Harness {
    /// Builds a server with a store and one project, and lets the caller
    /// finish the deploy state — which needs the project's id — before the
    /// router is made.
    pub async fn build<F>(name: &str, configure: F) -> Self
    where
        F: FnOnce(DeployState, ProjectId) -> DeployState,
    {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root =
            std::env::temp_dir().join(format!("liyasa-deploy-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a test directory");
        let ingest = IngestQueue::new(1024, 64);
        let store = Arc::new(
            SqliteStore::open(
                &root.join("liyasa.db"),
                MasterKey::generate().expect("a key"),
                ingest.clone(),
            )
            .await
            .expect("a store"),
        );
        let project = store
            .projects_typed()
            .create(&format!("{name}-{n}"), name)
            .await
            .expect("a project")
            .id;
        let state = Arc::new(
            AppState::new(ServerConfig::default())
                .with_ingest(ingest)
                .with_store(store.clone()),
        );
        let deploy = configure(
            DeployState::new(state.clone()).expect("a store is configured"),
            project,
        );
        let router = liyasa_server::routes::router(state.clone())
            .merge(liyasa_server::deploy::routes::router(Arc::new(deploy)));
        Self {
            state,
            store,
            router,
            project,
            root,
        }
    }

    /// A second project in the same store, so fairness has something to be
    /// fair between.
    pub async fn another_project(&self, slug: &str) -> ProjectId {
        self.store
            .projects_typed()
            .create(slug, slug)
            .await
            .expect("another project")
            .id
    }

    pub async fn send(&self, request: Request<Body>) -> Response<Body> {
        self.router
            .clone()
            .oneshot(request)
            .await
            .expect("the router answers")
    }

    pub async fn get(&self, path: &str) -> Response<Body> {
        self.send(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .expect("a request"),
        )
        .await
    }

    pub async fn post(&self, path: &str, body: serde_json::Value) -> Response<Body> {
        self.send(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("a request"),
        )
        .await
    }

    pub async fn post_raw(
        &self,
        path: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Response<Body> {
        let mut request = Request::builder().method("POST").uri(path);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        self.send(request.body(Body::from(body.to_vec())).expect("a request"))
            .await
    }

    /// A succeeded build of `env`, with a `dist` directory that exists so the
    /// retention sweep has something to remove.
    pub async fn build_record(&self, env: &str, seed: &str, created_at: i64) -> BuildId {
        let id = BuildId(Fingerprint::of(seed.as_bytes()));
        let dist = self.root.join("dist").join(seed);
        std::fs::create_dir_all(&dist).expect("a bundle directory");
        self.store
            .builds_typed()
            .put(&BuildRecord {
                id,
                project: self.project,
                env: env.to_owned(),
                status: BuildStatus::Succeeded,
                dist: dist.to_string_lossy().into_owned(),
                created_at,
                updated_at: created_at + 1_000,
                version: 1,
            })
            .await
            .expect("a build record");
        id
    }

    pub fn dist_of(&self, seed: &str) -> PathBuf {
        self.root.join("dist").join(seed)
    }
}

pub async fn body_json(response: Response<Body>) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("a body");
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}
