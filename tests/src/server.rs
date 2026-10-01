//! A `liyasa serve` under test (WP-14).
//!
//! The router is driven in process: every acceptance test here is about what
//! the server answers, not about sockets, and a request through
//! `tower::ServiceExt` is the same path a request off the wire takes once the
//! listener has handed it over.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use http::{Request, Response, StatusCode};
use liyasa_server::routes::bundle::Bundle;
use liyasa_server::routes::limiter::Limiter;
use liyasa_server::routes::{AppState, ServerConfig};
use liyasa_store::{IngestQueue, MasterKey, SqliteStore};
use tower::ServiceExt as _;

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct Harness {
    pub state: Arc<AppState>,
    pub router: Router,
    /// What `routes::application` mounted, so a test can assert on why a
    /// subtree is absent as well as that it is (RFC 1403).
    pub mounted: Vec<liyasa_server::routes::MountRecord>,
    pub dist: PathBuf,
    root: PathBuf,
    /// Present exactly when `Setup::analytics` was set. Held so a test can
    /// drain the ingest queue deterministically; see [`Harness::flush_analytics`].
    writer: Option<liyasa_store::Writer>,
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// How the harness is set up. The defaults are a served site with limits
/// turned off, which is what a test that is not about limits wants.
pub struct Setup {
    pub name: String,
    /// `None` builds the standard fixture site.
    pub dist: Option<PathBuf>,
    pub limiter: Option<Arc<Limiter>>,
    pub config: ServerConfig,
    pub with_store: bool,
    /// `liyasa.json` as the subtrees read it. `None` is a site with no
    /// sections beyond the defaults, which is a public site with no auth.
    pub site_config: Option<serde_json::Value>,
    /// Reads the search index out of the bundle's `dist/`, the way
    /// `main.rs` does. On by default: an instance that serves a site serves
    /// its index, and leaving it off made every search answer 503 from the
    /// no-index branch while the tests passed on it.
    ///
    /// Set it false for a test whose subject IS an instance without one —
    /// that is a real shape, and it is the one `/_liyasa/search` answers 503
    /// for. Prefer this to pointing `dist` at an empty directory, which also
    /// removes the bundle and would make the 503 mean something else.
    pub search_index: bool,
    /// Opens an `analytics.db` and publishes its pool before composing, the
    /// way `Runtime::spawn_ingest_at` does. Off by default: most tests want
    /// the shape of an instance that opened none.
    pub analytics: bool,
    /// Attaches a persistent drift `RecordStore` on the application pool, the
    /// way `main` does. Off by default, because an instance that keeps no
    /// records is a shape worth testing and is what most tests want.
    ///
    /// A test that turns this on needs a multi-threaded runtime:
    /// `RecordStore` is synchronous and `SqliteDrift` bridges with
    /// `block_in_place`, which panics on a current-thread runtime.
    pub drift: bool,
}

impl Setup {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            dist: None,
            limiter: Some(Arc::new(Limiter::unlimited())),
            config: ServerConfig::default(),
            with_store: true,
            site_config: None,
            search_index: true,
            analytics: false,
            drift: false,
        }
    }
}

/// Builds the fixture site once per test and returns its `dist/`, complete
/// with the `_headers` the hosting seam generates.
pub fn fixture_dist(name: &str) -> (crate::hosting::Site, PathBuf) {
    let site = crate::hosting::Site::build(
        &format!("server-{name}"),
        crate::hosting::CONFIG,
        liyasa_build::engine::Options::default(),
    );
    let dist = site.dist();
    (site, dist)
}

impl Harness {
    pub async fn new(setup: Setup) -> (Self, Option<crate::hosting::Site>) {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "liyasa-server-{}-{}-{n}",
            setup.name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a test directory");

        let (site, dist) = match setup.dist {
            Some(dist) => (None, dist),
            None => {
                let (site, dist) = fixture_dist(&setup.name);
                (Some(site), dist)
            }
        };

        let ingest = IngestQueue::new(1024, 64);
        let ingest_for_analytics = ingest.clone();
        let config = match setup.site_config {
            Some(value) => ServerConfig {
                site_config: Arc::new(value),
                ..setup.config
            },
            None => setup.config,
        };
        let mut state = AppState::new(config).with_ingest(ingest.clone());
        if let Some(limiter) = setup.limiter {
            state = state.with_limiter(limiter);
        }
        // A harness pointed at a directory with no bundle leaves the state
        // without one on purpose: that is the shape a readiness test needs.
        if !state.config.collector_only
            && let Ok(bundle) = Bundle::open(&dist)
        {
            state = state.with_bundle(Arc::new(bundle));
            // The same two steps `main.rs:398-405` performs, from the same
            // `dist/`. Without the index every request to `/_liyasa/search`
            // answered 503 from the no-index branch, and the search tests
            // passed on it because they assert `!= 404`, a JSON content type
            // and cache headers — all of which a 503 satisfies. So no test in
            // the tree drove a search that returned a result, which is the
            // "handler nothing reaches" shape one layer up from the one
            // `rest_04_search.rs` was written for.
            if setup.search_index
                && let Some(index) = liyasa_server::routes::search::open(&dist)
            {
                state = state.with_search_index(Arc::new(index));
            }
        }
        if setup.with_store {
            let store = SqliteStore::open(
                &root.join("liyasa.db"),
                MasterKey::generate().expect("a key"),
                ingest,
            )
            .await
            .expect("a store");
            if setup.drift {
                state = state.with_drift_records(Arc::new(
                    liyasa_server::routes::drift::SqliteDrift::new(store.clone_pool()),
                ));
            }
            state = state.with_store(Arc::new(store));
        }
        let state = Arc::new(state);
        let mut writer = None;
        if setup.analytics {
            // The writer opens the database and publishes its pool; the
            // dashboard reads through that pool rather than opening a second
            // one. Done here, before `application`, for the same reason the
            // runtime does it before mounting.
            let opened = liyasa_store::Writer::open(
                &root.join("analytics.db"),
                ingest_for_analytics,
                liyasa_store::IngestOptions::default(),
            )
            .await
            .expect("an analytics database");
            state.publish_analytics_pool(opened.pool().clone());
            // Kept rather than dropped. In the runtime the writer lives in a
            // task that drains the queue every second; here there is no task,
            // so a test that pushes an event and then reads the table finds
            // nothing and reads as "the call site records nothing". `flush`
            // below is what turns that into an answer about the call site.
            writer = Some(opened);
        }
        // RFC 1403: the same composition the binary performs. A harness that
        // builds its own router tests an application the product never runs,
        // which is exactly how two packages' HTTP surfaces went unrouted.
        let application = liyasa_server::routes::application(state.clone());
        (
            Self {
                state,
                router: application.router,
                mounted: application.mounted,
                dist,
                root,
                writer,
            },
            site,
        )
    }

    /// Drains the ingest queue into `analytics.db` and returns how many rows
    /// were written.
    ///
    /// The runtime has a task doing this on a timer; a test needs it to have
    /// happened before it reads. Without this an assertion over the `event`
    /// table is about the writer's timing rather than about the call site that
    /// pushed, and an empty table reads as "nothing was recorded" when the row
    /// is sitting in memory.
    ///
    /// Loops because `Writer::flush` writes at most one batch per call.
    pub async fn flush_analytics(&mut self) -> usize {
        let Some(writer) = self.writer.as_mut() else {
            panic!("flush_analytics needs Setup {{ analytics: true, .. }}");
        };
        let mut written = 0;
        loop {
            let batch = writer.flush().await.expect("the analytics writer flushes");
            if batch == 0 {
                return written;
            }
            written += batch;
        }
    }

    /// The analytics pool the dashboard reads through, for a test that wants
    /// to run a report rather than an HTTP request.
    pub fn analytics_pool(&self) -> &liyasa_store::SqlitePool {
        self.writer
            .as_ref()
            .expect("analytics_pool needs Setup { analytics: true, .. }")
            .pool()
    }

    /// The common case: a served site with no limiter.
    pub async fn serving(name: &str) -> (Self, crate::hosting::Site) {
        let (harness, site) = Self::new(Setup::new(name)).await;
        (harness, site.expect("the fixture site"))
    }

    pub async fn send(&self, request: Request<Body>) -> Response<Body> {
        self.router
            .clone()
            .oneshot(request)
            .await
            .expect("the router answers")
    }

    /// Any method, for a route whose existence is the thing under test.
    pub async fn request(&self, method: &str, path: &str) -> Response<Body> {
        self.send(
            builder(method, path)
                .body(Body::empty())
                .expect("a request"),
        )
        .await
    }

    pub async fn get(&self, path: &str) -> Response<Body> {
        self.send(builder("GET", path).body(Body::empty()).expect("a request"))
            .await
    }

    /// A request from a given client address, which is what a limiter test
    /// needs in order to be several clients.
    pub async fn get_from(&self, path: &str, last_octet: u8) -> Response<Body> {
        let mut request = builder("GET", path).body(Body::empty()).expect("a request");
        request.extensions_mut().insert(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(203, 0, 113, last_octet)),
            40000,
        )));
        self.send(request).await
    }

    pub async fn get_with(&self, path: &str, headers: &[(&str, &str)]) -> Response<Body> {
        let mut builder = builder("GET", path);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        self.send(builder.body(Body::empty()).expect("a request"))
            .await
    }

    pub async fn post_json(&self, path: &str, body: serde_json::Value) -> Response<Body> {
        self.send(
            builder("POST", path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("a request"),
        )
        .await
    }
}

fn builder(method: &str, path: &str) -> http::request::Builder {
    Request::builder().method(method).uri(path)
}

/// Reads a whole response body; every response this server sends is complete
/// before it is sent, so this never blocks on a stream (AUTH-14).
pub async fn body_bytes(response: Response<Body>) -> Vec<u8> {
    axum::body::to_bytes(response.into_body(), 32 * 1024 * 1024)
        .await
        .expect("a complete body")
        .to_vec()
}

pub async fn body_text(response: Response<Body>) -> String {
    String::from_utf8_lossy(&body_bytes(response).await).into_owned()
}

pub async fn body_json(response: Response<Body>) -> serde_json::Value {
    let bytes = body_bytes(response).await;
    serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        panic!(
            "a JSON body: {e}; body was {}",
            String::from_utf8_lossy(&bytes)
        )
    })
}

pub fn header<'a>(response: &'a Response<Body>, name: &str) -> Option<&'a str> {
    response.headers().get(name).and_then(|v| v.to_str().ok())
}

/// Asserts the status and returns the response, so a failure says what came
/// back rather than just that it was wrong.
pub fn expect_status(response: Response<Body>, status: StatusCode) -> Response<Body> {
    assert_eq!(
        response.status(),
        status,
        "headers: {:?}",
        response.headers()
    );
    response
}
