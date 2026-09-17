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
        }
        if setup.with_store {
            let store = SqliteStore::open(
                &root.join("liyasa.db"),
                MasterKey::generate().expect("a key"),
                ingest,
            )
            .await
            .expect("a store");
            state = state.with_store(Arc::new(store));
        }
        let state = Arc::new(state);
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
            },
            site,
        )
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
