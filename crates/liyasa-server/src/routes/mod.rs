//! Everything WP-14 serves: the site, health and metrics, the limiter, the
//! ingest endpoint, feedback, jobs, and webhooks.
//!
//! One `AppState` is shared by every handler. It holds what a request needs
//! and nothing a request may write: the bundle and the configuration are
//! read-only, the queue and the limiter are internally synchronized, and the
//! store is the only thing that persists.

pub mod acme;
pub mod api;
pub mod bundle;
pub mod client_ip;
pub mod deployments;
pub mod events;
pub mod feedback;
pub mod health;
pub mod httpdate;
pub mod jobs;
pub mod limiter;
pub mod metrics;
pub mod mount;
pub mod problem;
pub mod serve;
pub mod session;
pub mod site;
pub mod telemetry;
pub mod tools;
pub mod work;
pub mod tls;
pub mod webhooks;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Router, middleware};
use http::{HeaderMap, StatusCode, request::Parts};
use liyasa_core::server::{RateLimitKey, RateLimitPool, RateLimiter};
use liyasa_store::{IngestQueue, SqliteStore};
use liyasa_verify::core::scrub::Scrubber;
use serde_json::{Value, json};

use api::Idempotency;
use bundle::Bundle;
use client_ip::TrustedProxies;
use limiter::Limiter;
use metrics::Metrics;
use problem::Problem;
use session::DailySalt;
use telemetry::Tracer;

/// What `liyasa serve` was told, after `liyasa.json` and the flags are merged.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// The analytics `site` field; the project slug.
    pub site: String,
    pub env: String,
    /// HOST-08: no outbound request of any kind.
    pub offline: bool,
    pub drain_timeout: Duration,
    /// ANA-09: accept events and serve nothing.
    pub collector_only: bool,
    /// Origins a collector accepts events for. Empty means same-origin only.
    pub collector_origins: Vec<String>,
    /// Sites a collector accepts events for (ANA-09). A collector with none
    /// configured accepts only its own `site`, because an open collector lets
    /// any client write into any site's aggregates.
    pub collector_sites: Vec<String>,
    pub analytics_enabled: bool,
    /// The header an operator's edge sets for region (§33.1 item 10). Read
    /// only from a trusted proxy.
    pub region_header: Option<String>,
    pub jobs_lease: Duration,
    /// `liyasa.json` after overlays. Each subtree reads its own section out of
    /// this rather than `ServerConfig` growing a field per package (RFC 1403).
    pub site_config: Arc<serde_json::Value>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            site: "liyasa".to_owned(),
            env: "production".to_owned(),
            offline: false,
            drain_timeout: Duration::from_secs(30),
            collector_only: false,
            collector_origins: Vec::new(),
            collector_sites: Vec::new(),
            analytics_enabled: true,
            region_header: None,
            jobs_lease: Duration::from_secs(60),
            site_config: Arc::new(serde_json::Value::Null),
        }
    }
}

pub struct AppState {
    pub config: ServerConfig,
    pub bundle: Option<Arc<Bundle>>,
    pub store: Option<Arc<SqliteStore>>,
    pub ingest: IngestQueue,
    pub limiter: Arc<Limiter>,
    pub proxies: TrustedProxies,
    pub metrics: Arc<Metrics>,
    pub tracer: Arc<Tracer>,
    pub salt: DailySalt,
    pub idempotency: Idempotency,
    pub scrubber: Scrubber,
    /// The ACME tokens this replica is answering for (HOST-02).
    pub challenges: Arc<acme::Challenges>,
    pub started: Instant,
    draining: AtomicBool,
    /// What `application` mounted, so readiness can report it (RFC 1403).
    /// Written once, by `application`, before the server accepts a request.
    mounted: std::sync::OnceLock<Vec<MountRecord>>,
    /// A handle to this state, for the background work an event starts
    /// (RFC 1404). Weak so the state does not hold itself alive; written once,
    /// by `application`.
    self_arc: std::sync::OnceLock<std::sync::Weak<AppState>>,
    /// Where the `auth` subtree publishes the state its endpoints were built
    /// from, so a second consumer gets THAT object rather than building a
    /// second one (RFC 1403, "One state, two consumers"). The session layer
    /// is the second consumer: two `AuthState`s means two `Sessions` tables,
    /// a cookie minted by `POST /_liyasa/auth/password` that resolves against
    /// neither, and a server where sign-in appears to work and every later
    /// request is anonymous. A `OnceLock` rather than a field because only
    /// the subtree knows whether there is one — a public site has none.
    auth_state: std::sync::OnceLock<Arc<crate::auth::state::AuthState>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("config", &self.config)
            .field("draining", &self.draining)
            .finish_non_exhaustive()
    }
}

impl AppState {
    pub fn new(config: ServerConfig) -> Self {
        Self {
            ingest: IngestQueue::new(100_000, 5_000),
            bundle: None,
            store: None,
            limiter: Arc::new(Limiter::new()),
            proxies: TrustedProxies::default(),
            metrics: Arc::new(Metrics::new()),
            tracer: Arc::new(Tracer::disabled()),
            salt: DailySalt::new(),
            idempotency: Idempotency::default(),
            scrubber: Scrubber::new(),
            challenges: Arc::new(acme::Challenges::default()),
            started: Instant::now(),
            mounted: std::sync::OnceLock::new(),
            self_arc: std::sync::OnceLock::new(),
            auth_state: std::sync::OnceLock::new(),
            draining: AtomicBool::new(false),
            config,
        }
    }

    pub fn with_bundle(mut self, bundle: Arc<Bundle>) -> Self {
        self.bundle = Some(bundle);
        self
    }

    pub fn with_store(mut self, store: Arc<SqliteStore>) -> Self {
        self.ingest = store.ingest().clone();
        self.store = Some(store);
        self
    }

    pub fn with_ingest(mut self, ingest: IngestQueue) -> Self {
        self.ingest = ingest;
        self
    }

    pub fn with_proxies(mut self, proxies: TrustedProxies) -> Self {
        self.proxies = proxies;
        self
    }

    pub fn with_limiter(mut self, limiter: Arc<Limiter>) -> Self {
        self.limiter = limiter;
        self
    }

    pub fn with_tracer(mut self, tracer: Arc<Tracer>) -> Self {
        self.tracer = tracer;
        self
    }

    /// Secret values the scrubber must redact wherever they appear.
    pub fn with_scrubber(mut self, scrubber: Scrubber) -> Self {
        self.scrubber = scrubber;
        self
    }

    /// What the application mounted. Empty before `application` has run,
    /// which is the case in a test that builds a router by hand.
    pub fn mounted(&self) -> &[MountRecord] {
        self.mounted.get().map(Vec::as_slice).unwrap_or_default()
    }

    /// The state the `auth` subtree built, or `None` on a site that mounted no
    /// auth. Anything that needs authentication state — the session layer
    /// above all — takes it from here and never constructs its own.
    pub fn auth_state(&self) -> Option<&Arc<crate::auth::state::AuthState>> {
        self.auth_state.get()
    }

    /// Called once by the `auth` subtree's adapter, before any request.
    pub(crate) fn publish_auth_state(&self, state: Arc<crate::auth::state::AuthState>) {
        let _ = self.auth_state.set(state);
    }

    pub fn draining(&self) -> bool {
        self.draining.load(Ordering::SeqCst)
    }

    pub fn begin_drain(&self) {
        self.draining.store(true, Ordering::SeqCst);
    }

    /// The address the limiter charges and the session key hashes. Falls back
    /// to loopback when the listener gave no peer, which is what an in-process
    /// test looks like.
    pub fn client_ip(&self, parts: &Parts) -> IpAddr {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        self.proxies.client_ip(peer, &parts.headers)
    }

    /// The reader's region, from the edge header and only through a trusted
    /// proxy: Liyasa ships no geolocation database and never looks an address
    /// up (§33.1 item 10, AUTH-50).
    pub fn region(&self, headers: &HeaderMap) -> Option<String> {
        if self.proxies.is_empty() {
            return None;
        }
        let mut names: Vec<&str> = vec!["cf-ipcountry", "x-vercel-ip-country"];
        if let Some(custom) = &self.config.region_header {
            names.push(custom.as_str());
        }
        names
            .iter()
            .find_map(|name| headers.get(*name).and_then(|v| v.to_str().ok()))
            .map(|value| value.trim().to_ascii_uppercase())
            .filter(|value| value.len() == 2 && value.chars().all(|c| c.is_ascii_uppercase()))
    }

    pub fn scrub(&self, text: &str) -> String {
        self.scrubber.scrub(text)
    }

    /// Scrubs every string inside a JSON value, however deep.
    pub fn scrub_value(&self, value: &Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.scrub(text)),
            Value::Array(items) => {
                Value::Array(items.iter().map(|v| self.scrub_value(v)).collect())
            }
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), self.scrub_value(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// Whether an event body may attribute itself to `site` (ANA-09).
    ///
    /// A served instance reads no site from a body at all, so anything is
    /// "allowed" in the sense that it is discarded and replaced. A collector
    /// accepts only the sites it was configured for, and a collector that was
    /// configured with none accepts nothing: an open collector lets any client
    /// write into any site's aggregates, and those are the thirteen-month
    /// record rather than the ninety-day one.
    pub fn site_allowed(&self, site: &str) -> bool {
        if !self.config.collector_only {
            return true;
        }
        self.config
            .collector_sites
            .iter()
            .any(|allowed| allowed == site)
            || site == self.config.site
    }

    /// ANA-09: a collector accepts only the origins it was configured with.
    /// A server that also serves the site accepts its own requests, which
    /// carry no `Origin` or its own.
    pub fn origin_allowed(&self, origin: Option<&str>) -> bool {
        if !self.config.collector_only {
            return true;
        }
        match origin {
            None => true,
            Some(origin) => self
                .config
                .collector_origins
                .iter()
                .any(|allowed| allowed == origin || allowed == "*"),
        }
    }

    /// Queues a webhook delivery for every interested subscription (REST-10).
    /// Fire and forget: a request never waits on a receiver.
    pub fn notify_webhook(&self, event_type: &str, data: &Value) {
        let Some(store) = self.store.clone() else {
            return;
        };
        let envelope = webhooks::envelope(event_type, data);
        let payload = serde_json::to_string(&envelope).unwrap_or_else(|_| "{}".to_owned());
        let id = envelope["id"].as_str().unwrap_or_default().to_owned();
        let event_type = event_type.to_owned();
        let state = self.self_arc.get().and_then(std::sync::Weak::upgrade);
        let data = data.clone();
        tokio::spawn(async move {
            if let Err(error) = store.webhooks().queue(&id, &event_type, &payload).await {
                tracing::warn!(target: "liyasa_server", %error, "a webhook could not be queued");
            }
            // RFC 1404: the same event that notifies a receiver starts the
            // work packages registered for it.
            if event_type == "deployment.succeeded"
                && let Some(state) = state
                && let Err(error) = work::on_deployment(&state, work::kinds(), &data).await
            {
                tracing::warn!(target: "liyasa_server", %error, "a post-deploy job could not be queued");
            }
        });
    }
}

/// Which budget a request is charged to (§30.2.5). Operations endpoints are
/// deliberately unlimited: a monitoring system polls them, and refusing a
/// probe turns a healthy replica unhealthy.
pub fn pool_for(path: &str, wants_markdown: bool) -> Option<RateLimitPool> {
    match path {
        "/_liyasa/health" | "/_liyasa/ready" | "/_liyasa/metrics" => None,
        // A directory validating a challenge must never be rate limited: the
        // certificate would fail to issue.
        p if p.starts_with("/.well-known/acme-challenge/") => None,
        "/_liyasa/e" => Some(RateLimitPool::Pages),
        p if p.starts_with("/_liyasa/feedback") => Some(RateLimitPool::Feedback),
        p if p.starts_with("/_liyasa/api/") => Some(RateLimitPool::Rest),
        p if p.starts_with("/_liyasa/mcp") => Some(RateLimitPool::Mcp),
        p if p.starts_with("/_liyasa/search") => Some(RateLimitPool::Search),
        p if p.starts_with("/_liyasa/assistant") => Some(RateLimitPool::Assistant),
        p if p.starts_with("/_liyasa/proxy") => Some(RateLimitPool::PlaygroundProxy),
        p if p.starts_with("/_liyasa/auth") => Some(RateLimitPool::Auth),
        // `.md` and negotiated Markdown have their own, higher pool: agent
        // fetching is a goal, not abuse (AUTH-14).
        p if p.ends_with(".md") || wants_markdown => Some(RateLimitPool::AgentPages),
        _ => Some(RateLimitPool::Pages),
    }
}

fn pool_name(pool: RateLimitPool) -> &'static str {
    match pool {
        RateLimitPool::Pages => "pages",
        RateLimitPool::AgentPages => "agentPages",
        RateLimitPool::Search => "search",
        RateLimitPool::Assistant => "assistant",
        RateLimitPool::Feedback => "feedback",
        RateLimitPool::PlaygroundProxy => "proxy",
        RateLimitPool::Rest => "rest",
        RateLimitPool::Mcp => "mcp",
        RateLimitPool::Auth => "auth",
        _ => "other",
    }
}

/// The metric label for a request, which is never the path itself: a label
/// per route is unbounded cardinality.
fn route_class(path: &str) -> &'static str {
    match path {
        "/_liyasa/health" => "health",
        "/_liyasa/ready" => "ready",
        "/_liyasa/metrics" => "metrics",
        "/_liyasa/e" => "events",
        p if p.starts_with("/_liyasa/feedback") => "feedback",
        p if p.starts_with("/_liyasa/api/") => "api",
        p if p.starts_with("/_liyasa/") => "internal",
        p if p.ends_with(".md") => "markdown",
        _ => "page",
    }
}

/// Rate limiting, metrics, and the request span, in that order.
pub async fn observe(
    State(state): State<Arc<AppState>>,
    request: axum::extract::Request,
    next: middleware::Next,
) -> Response {
    let started = Instant::now();
    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    let class = route_class(&path);
    let wants_markdown = bundle::prefers_markdown(
        request
            .headers()
            .get(http::header::ACCEPT)
            .and_then(|v| v.to_str().ok()),
    );

    let parent = request
        .headers()
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .and_then(telemetry::parse_traceparent);
    let mut recording = state.tracer.start(format!("{method} {class}"), parent);
    recording.attribute("http.request.method", json!(method.as_str()));
    recording.attribute("url.path", json!(path));
    recording.attribute("liyasa.route_class", json!(class));

    let pool = pool_for(&path, wants_markdown);
    if let Some(pool) = pool {
        let (parts, body) = request.into_parts();
        let subject = limiter::subject_for(state.client_ip(&parts));
        let key = RateLimitKey { pool, subject };
        if let Err(retry) = state.limiter.check(&key) {
            let seconds = limiter::retry_after_seconds(retry);
            state
                .metrics
                .increment("liyasa_rate_limited_total", &[("pool", pool_name(pool))], 1);
            state.metrics.increment(
                "liyasa_http_requests_total",
                &[("class", class), ("status", "429")],
                1,
            );
            recording.attribute("http.response.status_code", json!(429));
            state.tracer.end(recording, 2);
            // AUTH-14: a complete `429` with `Retry-After`. No interstitial,
            // no held-open body, and never a `200`.
            return Problem::rate_limited(seconds).into_response();
        }
        let request = axum::extract::Request::from_parts(parts, body);
        return finish(state, next, request, recording, started, class).await;
    }
    finish(state, next, request, recording, started, class).await
}

async fn finish(
    state: Arc<AppState>,
    next: middleware::Next,
    request: axum::extract::Request,
    mut recording: telemetry::Recording,
    started: Instant,
    class: &'static str,
) -> Response {
    let traceparent = telemetry::traceparent(recording.trace(), recording.id(), true);
    let mut response = next.run(request).await;
    let status = response.status();
    recording.attribute("http.response.status_code", json!(status.as_u16()));
    state
        .tracer
        .end(recording, if status.is_server_error() { 2 } else { 1 });
    if state.tracer.is_enabled()
        && let Ok(value) = http::HeaderValue::from_str(&traceparent)
    {
        response.headers_mut().insert("traceparent", value);
    }
    state.metrics.increment(
        "liyasa_http_requests_total",
        &[("class", class), ("status", status.as_str())],
        1,
    );
    state.metrics.observe(
        "liyasa_http_request_duration_seconds",
        &[("class", class)],
        started.elapsed().as_secs_f64(),
    );
    response
}

/// `GET /_liyasa/api/v1/content?path=` (REST-04).
pub async fn content(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<ContentParams>,
) -> Response {
    let Some(bundle) = state.bundle.clone() else {
        return Problem::new(StatusCode::SERVICE_UNAVAILABLE, "No deployment")
            .detail("this instance serves no site")
            .into_response();
    };
    let Some(path) = params.path else {
        return Problem::bad_request("`path` is required").into_response();
    };
    match site::content(&bundle, &path) {
        Some(body) => api::Json(body).into_response(),
        None => Problem::not_found("page").into_response(),
    }
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ContentParams {
    pub path: Option<String>,
}

/// Everything that is not a `/_liyasa/` route is a page (§6.4).
pub async fn page(State(state): State<Arc<AppState>>, request: axum::extract::Request) -> Response {
    let Some(bundle) = state.bundle.clone() else {
        return Problem::new(StatusCode::SERVICE_UNAVAILABLE, "No deployment")
            .detail("this instance serves no site")
            .into_response();
    };
    let path = request.uri().path().to_owned();
    site::serve(&bundle, &path, request.headers()).into_response()
}

/// What one subtree contributed, for the startup log and for readiness.
#[derive(Debug, Clone)]
pub struct MountRecord {
    pub name: &'static str,
    pub mounted: bool,
    /// Why this instance mounts nothing for it. Present exactly when
    /// `mounted` is false.
    pub skipped: Option<String>,
}

/// The whole application: every package's routes, and what each one did.
pub struct Application {
    pub router: Router,
    pub mounted: Vec<MountRecord>,
    /// Everything the subtrees raised while being built, to be reported once
    /// at startup rather than once per request.
    pub diagnostics: liyasa_core::diagnostics::Diagnostics,
}

/// Everything `liyasa serve` mounts (RFC 1403).
///
/// The binary calls exactly this and so does every test harness, so the
/// application the product runs and the application a test asserts against
/// cannot drift apart. Two packages' complete HTTP surfaces were dead code in
/// the shipped binary for days because each composed its own router in its own
/// harness; this function exists so that cannot happen again.
pub fn application(state: Arc<AppState>) -> Application {
    let mut router = router(state.clone());
    let mut mounted = Vec::new();
    let mut diagnostics = liyasa_core::diagnostics::Diagnostics::new();

    for subtree in mount::subtrees() {
        // A collector serves no site and no subtree: it accepts events and
        // nothing else (ANA-09).
        if state.config.collector_only {
            mounted.push(MountRecord {
                name: subtree.name,
                mounted: false,
                skipped: Some("this instance is a collector and serves no routes".to_owned()),
            });
            continue;
        }
        let contribution = (subtree.mount)(&state);
        diagnostics.extend(contribution.diagnostics.into_vec());
        match contribution.router {
            Some(subtree_router) => {
                // The subtree declares what a caller must hold; the seam
                // applies it, because a subtree outside this crate cannot
                // name a `Permission` without a dependency cycle (RFC 1403).
                let subtree_router = match subtree.permission {
                    Some(permission) => mount::guarded(subtree_router, permission),
                    None => subtree_router,
                };
                router = router.merge(subtree_router);
                mounted.push(MountRecord {
                    name: subtree.name,
                    mounted: true,
                    skipped: None,
                });
            }
            None => mounted.push(MountRecord {
                name: subtree.name,
                mounted: false,
                skipped: Some(
                    contribution
                        .skipped
                        .unwrap_or_else(|| "not configured on this instance".to_owned()),
                ),
            }),
        }
    }
    // Readiness answers from the state, not from the router, so record it
    // before the first request can ask.
    let _ = state.mounted.set(mounted.clone());
    let _ = state.self_arc.set(Arc::downgrade(&state));
    Application {
        router,
        mounted,
        diagnostics,
    }
}

/// Only this package's own routes. `application` starts from it; a test aimed
/// at one package's handlers may use it, but nothing that claims to be about
/// the product should.
pub fn router(state: Arc<AppState>) -> Router {
    let collector_only = state.config.collector_only;
    let mut router = Router::new()
        .route("/_liyasa/health", get(health::health))
        .route("/_liyasa/ready", get(health::ready))
        .route("/_liyasa/metrics", get(health::metrics))
        .route(
            "/_liyasa/e",
            post(events::ingest).options(events::preflight),
        )
        // HOST-02: answered from memory while an order is in flight, and a
        // 404 the rest of the time.
        .route("/.well-known/acme-challenge/{token}", get(acme::challenge));

    if !collector_only {
        router = router
            .route(
                "/_liyasa/feedback",
                post(feedback::submit).get(feedback::list),
            )
            .route("/_liyasa/feedback/summary", get(feedback::summary))
            .route("/_liyasa/feedback/{id}", patch(feedback::set_status))
            .route("/_liyasa/api/v1/content", get(content))
            .route("/_liyasa/api/v1/jobs", get(jobs::list))
            .route("/_liyasa/api/v1/jobs/{id}", get(jobs::get))
            .route("/_liyasa/api/v1/jobs/{id}/retry", post(jobs::retry))
            .route("/_liyasa/api/v1/jobs/{id}/cancel", post(jobs::cancel))
            .route(
                "/_liyasa/api/v1/deployments",
                get(deployments::list).post(deployments::create),
            )
            .route(
                "/_liyasa/api/v1/deployments/{env}",
                get(deployments::current).delete(deployments::delete),
            )
            .route(
                "/_liyasa/api/v1/deployments/{env}/rollback",
                post(deployments::rollback),
            )
            .route(
                "/_liyasa/api/v1/webhooks",
                get(webhooks::list).post(webhooks::subscribe),
            )
            .route(
                "/_liyasa/api/v1/webhooks/{id}",
                axum::routing::delete(webhooks::unsubscribe),
            )
            .fallback(page);
    } else {
        // ANA-09: a collector serves no site, and says so rather than 404ing
        // every path as if the site were merely missing.
        router = router.fallback(|| async {
            Problem::new(StatusCode::NOT_FOUND, "Collector only")
                .detail("this instance accepts analytics events and serves no site")
                .into_response()
        });
    }

    router
        .layer(middleware::from_fn_with_state(state.clone(), observe))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_traffic_is_charged_to_its_own_higher_pool() {
        assert_eq!(
            pool_for("/guide.md", false),
            Some(RateLimitPool::AgentPages)
        );
        assert_eq!(pool_for("/guide", true), Some(RateLimitPool::AgentPages));
        assert_eq!(pool_for("/guide", false), Some(RateLimitPool::Pages));
        assert_eq!(
            pool_for("/_liyasa/feedback", false),
            Some(RateLimitPool::Feedback)
        );
        assert_eq!(
            pool_for("/_liyasa/api/v1/jobs", false),
            Some(RateLimitPool::Rest)
        );
    }

    #[test]
    fn an_operations_endpoint_is_never_rate_limited() {
        for path in ["/_liyasa/health", "/_liyasa/ready", "/_liyasa/metrics"] {
            assert_eq!(pool_for(path, false), None, "{path}");
        }
    }

    #[test]
    fn a_metric_label_is_a_class_rather_than_a_path() {
        assert_eq!(route_class("/guides/install"), "page");
        assert_eq!(route_class("/guides/install.md"), "markdown");
        assert_eq!(route_class("/_liyasa/health"), "health");
        assert_eq!(route_class("/_liyasa/api/v1/jobs"), "api");
    }

    #[test]
    fn a_region_header_is_ignored_without_a_trusted_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert("cf-ipcountry", "US".parse().expect("a value"));
        let state = AppState::new(ServerConfig::default());
        assert_eq!(state.region(&headers), None);

        let trusting = AppState::new(ServerConfig::default())
            .with_proxies(TrustedProxies::new(&["10.0.0.0/8".to_owned()]));
        assert_eq!(trusting.region(&headers), Some("US".to_owned()));

        headers.insert("cf-ipcountry", "not-a-country".parse().expect("a value"));
        assert_eq!(trusting.region(&headers), None, "only a two-letter code");
    }

    #[test]
    fn a_collector_accepts_only_the_origins_it_was_given() {
        let mut config = ServerConfig {
            collector_only: true,
            collector_origins: vec!["https://docs.example.com".to_owned()],
            ..ServerConfig::default()
        };
        let state = AppState::new(config.clone());
        assert!(state.origin_allowed(Some("https://docs.example.com")));
        assert!(!state.origin_allowed(Some("https://evil.example")));
        assert!(
            state.origin_allowed(None),
            "a same-origin post carries none"
        );

        config.collector_only = false;
        let serving = AppState::new(config);
        assert!(
            serving.origin_allowed(Some("https://anything")),
            "a serving instance is not a collector"
        );
    }

    #[test]
    fn scrubbing_reaches_every_string_in_a_nested_value() {
        let state = AppState::new(ServerConfig::default())
            .with_scrubber(Scrubber::with_secrets(["sk_live_abcdef123456"]));
        let scrubbed = state.scrub_value(&json!({
            "note": "the key is sk_live_abcdef123456",
            "nested": [{ "also": "sk_live_abcdef123456" }],
            "count": 3,
        }));
        assert!(
            !scrubbed.to_string().contains("sk_live_abcdef123456"),
            "{scrubbed}"
        );
        assert_eq!(scrubbed["count"], 3);
    }
}
