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
pub mod problem;
pub mod serve;
pub mod session;
pub mod site;
pub mod telemetry;
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
    pub analytics_enabled: bool,
    /// The header an operator's edge sets for region (§33.1 item 10). Read
    /// only from a trusted proxy.
    pub region_header: Option<String>,
    pub jobs_lease: Duration,
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
            analytics_enabled: true,
            region_header: None,
            jobs_lease: Duration::from_secs(60),
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
        tokio::spawn(async move {
            if let Err(error) = store.webhooks().queue(&id, &event_type, &payload).await {
                tracing::warn!(target: "liyasa_server", %error, "a webhook could not be queued");
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
