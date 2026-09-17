//! The router subtree this package mounts (ANA-70, REST-06, RFC 1704).
//!
//! `bin/wp-paths.tsv` gives WP-17 no directory inside `crates/liyasa-server`,
//! so the handlers for this package's endpoints live here and
//! `liyasa-server`'s composition point registers [`mount`] by name.
//!
//! **This router applies no authorization of its own, and cannot.**
//! `Permission` is defined in `liyasa-server` and §34.7 already has
//! `liyasa-server` depending on `liyasa-analytics`, so applying it here would
//! close a dependency cycle. The permission is declared as data instead —
//! [`crate::api::Auth`] on each endpoint — and the seam that mounts this
//! wraps it. [`schema_router`] is separate for exactly that reason: ANA-02's
//! event schema is read by collectors that have no dashboard credential, so
//! it must not be inside whatever layer wraps [`mount`].
//!
//! Every handler is a thin shell over a function in this crate that is already
//! tested against a real database. What is new here, and what the tests in
//! `tests/it/serve.rs` cover, is the query-string parsing and the JSON shape —
//! which is the seam the dashboard reads and therefore the one that can drift.

use std::sync::Arc;

use axum::Router;
use axum::extract::{RawQuery, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use liyasa_core::ids::ProjectId;
use liyasa_core::store::StoreError;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::sqlite::SqlitePool;

use crate::insights::{self, Inputs, PageFacts};
use crate::query::{DAY_MS, Filters, Grain, Range, RangeSpec};
use crate::{actions, assistant, digest, feedback, integrations, privacy, retention, schema, search, traffic};

/// What the handlers read. Both pools are opened by `liyasa-store` and handed
/// in; this crate never opens a database (RFC 1700).
pub struct Analytics {
    /// `analytics.db`: `event`, `agg_hour`, `ingest_drops`.
    pub analytics: SqlitePool,
    /// `liyasa.db`: feedback and the job table.
    pub app: SqlitePool,
    pub site: String,
    pub project: Option<ProjectId>,
    pub retention: retention::Policy,
    /// The `integrations` block of `liyasa.json` (ANA-60).
    pub integrations: Value,
    /// From the build and the verification engine (ANA-40). Empty is allowed:
    /// the three cards that need it are then absent rather than guessed.
    pub pages: Vec<PageFacts>,
}

impl Analytics {
    fn inputs(&self) -> Inputs<'_> {
        Inputs {
            analytics: &self.analytics,
            app: &self.app,
            pages: &self.pages,
        }
    }
}

/// The window, grain and filters every read carries.
#[derive(Debug, Clone)]
struct Window {
    range: Range,
    grain: Grain,
    filters: Filters,
    compare: bool,
    limit: i64,
}

/// The numeric parameters, read separately from the filters so that an
/// unknown name in either half is ignored rather than failing the request.
#[derive(Debug, Default, Deserialize)]
struct Bounds {
    from: Option<i64>,
    to: Option<i64>,
    days: Option<i64>,
    grain: Option<String>,
    compare: Option<String>,
    limit: Option<i64>,
}

impl Window {
    /// A request with no window at all is the last 28 days, which is what the
    /// dashboard opens on. A malformed one is the same rather than a 400: a
    /// bookmarked URL from an older release should show traffic.
    fn parse(query: Option<&str>, now: i64) -> Self {
        let raw = query.unwrap_or_default();
        let bounds: Bounds = serde_urlencoded_lite(raw);
        let spec = match (bounds.from, bounds.to, bounds.days) {
            (Some(from), Some(to), _) if to > from => RangeSpec::Between { from, to },
            (_, _, Some(days)) if days > 0 => RangeSpec::Last { days },
            _ => RangeSpec::default(),
        };
        let range = spec.resolve(now);
        let grain = match bounds.grain.as_deref() {
            Some("hour") => Grain::Hour,
            Some("day") => Grain::Day,
            _ if range.span() <= 2 * DAY_MS => Grain::Hour,
            _ => Grain::Day,
        };
        Self {
            range,
            grain,
            filters: Filters::from_query(raw),
            compare: bounds.compare.as_deref() == Some("1"),
            limit: bounds.limit.unwrap_or(50).clamp(1, 500),
        }
    }
}

/// Reads the handful of scalars [`Bounds`] wants out of a query string.
///
/// `serde_urlencoded` is not in the dependency table and a whole crate for six
/// optional scalars is not worth a row in it. `url::form_urlencoded` is already
/// here for [`crate::schema::utm_from_route`].
fn serde_urlencoded_lite(raw: &str) -> Bounds {
    let mut bounds = Bounds::default();
    for (name, value) in url::form_urlencoded::parse(raw.trim_start_matches('?').as_bytes()) {
        match name.as_ref() {
            "from" => bounds.from = value.parse().ok(),
            "to" => bounds.to = value.parse().ok(),
            "days" => bounds.days = value.parse().ok(),
            "grain" => bounds.grain = Some(value.into_owned()),
            "compare" => bounds.compare = Some(value.into_owned()),
            "limit" => bounds.limit = value.parse().ok(),
            _ => {}
        }
    }
    bounds
}

fn now_ms() -> i64 {
    liyasa_store::now_ms()
}

/// A query failure is a 500 with a problem body, never a panic and never an
/// empty result that reads as "no traffic".
fn failed(error: StoreError) -> Response {
    let status = match error {
        StoreError::NotFound => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        [(header::CONTENT_TYPE, "application/problem+json")],
        json!({
            "title": "The analytics query failed",
            "detail": error.to_string(),
            "status": status.as_u16(),
        })
        .to_string(),
    )
        .into_response()
}

fn ok(value: Value) -> Response {
    axum::Json(value).into_response()
}

macro_rules! unwrap_or_fail {
    ($expression:expr) => {
        match $expression {
            Ok(value) => value,
            Err(error) => return failed(error),
        }
    };
}

/// The seventeen dashboard endpoints. Carries no authorization; the seam that
/// mounts it applies `Permission::DashboardRead`, which
/// `api::behind(Auth::DashboardRead)` declares.
pub fn mount(state: Arc<Analytics>) -> Router {
    Router::new()
        .route("/_liyasa/api/v1/analytics/series", get(series))
        .route("/_liyasa/api/v1/analytics/totals", get(totals))
        .route("/_liyasa/api/v1/analytics/pages", get(pages))
        .route("/_liyasa/api/v1/analytics/referrers", get(referrers))
        .route("/_liyasa/api/v1/analytics/journeys", get(journeys))
        .route("/_liyasa/api/v1/analytics/variants", get(variants))
        .route("/_liyasa/api/v1/analytics/delivery", get(delivery))
        .route("/_liyasa/api/v1/analytics/horizon", get(horizon))
        .route("/_liyasa/api/v1/analytics/search/queries", get(queries))
        .route("/_liyasa/api/v1/analytics/search/pages", get(search_pages))
        .route("/_liyasa/api/v1/analytics/search/trending", get(trending))
        .route("/_liyasa/api/v1/analytics/assistant", get(assistant_summary))
        .route("/_liyasa/api/v1/analytics/insights", get(cards))
        .route("/_liyasa/api/v1/analytics/insights/act", post(act))
        .route("/_liyasa/api/v1/analytics/integrations", get(vendors))
        .with_state(state)
}

/// ANA-02's published schema, which must stay outside the layer that wraps
/// [`mount`]: a collector validates its events against this document before it
/// is permitted to post any, so it has no dashboard credential.
pub fn schema_router() -> Router {
    Router::new().route(schema::PATH, get(event_schema))
}

async fn event_schema() -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/schema+json"),
            // A published contract changes only with a release.
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        axum::Json(schema::event()),
    )
        .into_response()
}

// ---- traffic (ANA-10) ----

/// The event types a traffic series counts when the caller does not say.
const VIEW_TYPES: &[&str] = &["page_view"];

fn requested_types(raw: Option<&str>) -> Vec<String> {
    let listed: Vec<String> = url::form_urlencoded::parse(raw.unwrap_or_default().as_bytes())
        .filter(|(name, _)| name == "type")
        .map(|(_, value)| value.into_owned())
        .collect();
    if listed.is_empty() {
        VIEW_TYPES.iter().map(|t| (*t).to_owned()).collect()
    } else {
        listed
    }
}

fn borrowed(types: &[String]) -> Vec<&str> {
    types.iter().map(String::as_str).collect()
}

async fn series(State(state): State<Arc<Analytics>>, RawQuery(raw): RawQuery) -> Response {
    let window = Window::parse(raw.as_deref(), now_ms());
    let types = requested_types(raw.as_deref());
    let series = unwrap_or_fail!(
        traffic::series(
            &state.analytics,
            window.range,
            window.grain,
            &window.filters,
            &borrowed(&types),
        )
        .await
    );
    ok(serde_json::to_value(series).unwrap_or_else(|_| json!({})))
}

async fn totals(State(state): State<Arc<Analytics>>, RawQuery(raw): RawQuery) -> Response {
    let window = Window::parse(raw.as_deref(), now_ms());
    let previous = window.range.previous();
    let mut out = serde_json::Map::new();
    for (label, types) in [
        ("Page views", &["page_view"][..]),
        ("Markdown fetches", &["markdown_fetch"][..]),
        ("MCP calls", &["mcp_call"][..]),
        ("Searches", &["search"][..]),
    ] {
        let current = unwrap_or_fail!(
            traffic::totals(&state.analytics, window.range, &window.filters, types).await
        );
        let before = if window.compare {
            unwrap_or_fail!(
                traffic::totals(&state.analytics, previous, &window.filters, types).await
            )
            .total()
        } else {
            0
        };
        out.insert(
            label.to_owned(),
            json!({ "current": current.total(), "previous": before }),
        );
    }
    let sessions =
        unwrap_or_fail!(traffic::unique_sessions(&state.analytics, window.range, &window.filters).await);
    let sessions_before = if window.compare {
        unwrap_or_fail!(traffic::unique_sessions(&state.analytics, previous, &window.filters).await)
            .total()
    } else {
        0
    };
    out.insert(
        "Sessions".to_owned(),
        json!({ "current": sessions.total(), "previous": sessions_before }),
    );
    ok(Value::Object(out))
}

async fn pages(State(state): State<Arc<Analytics>>, RawQuery(raw): RawQuery) -> Response {
    let window = Window::parse(raw.as_deref(), now_ms());
    let types = requested_types(raw.as_deref());
    let routes = unwrap_or_fail!(
        traffic::top_routes(
            &state.analytics,
            window.range,
            &window.filters,
            &borrowed(&types),
            window.limit,
        )
        .await
    );
    ok(json!({ "pages": routes }))
}

async fn referrers(State(state): State<Arc<Analytics>>, RawQuery(raw): RawQuery) -> Response {
    let window = Window::parse(raw.as_deref(), now_ms());
    let hosts = unwrap_or_fail!(
        traffic::referrers(&state.analytics, window.range, &window.filters, window.limit).await
    );
    ok(json!({ "referrers": hosts }))
}

async fn journeys(State(state): State<Arc<Analytics>>, RawQuery(raw): RawQuery) -> Response {
    let window = Window::parse(raw.as_deref(), now_ms());
    let (entry, exit) = unwrap_or_fail!(
        traffic::entry_and_exit(&state.analytics, window.range, &window.filters, window.limit).await
    );
    ok(json!({ "entry": entry, "exit": exit }))
}

async fn variants(State(state): State<Arc<Analytics>>, RawQuery(raw): RawQuery) -> Response {
    let window = Window::parse(raw.as_deref(), now_ms());
    let types = requested_types(raw.as_deref());
    let mut out = serde_json::Map::new();
    for dimension in ["version", "locale", "region", "product"] {
        let split = unwrap_or_fail!(
            traffic::by_variant(
                &state.analytics,
                window.range,
                &window.filters,
                dimension,
                &borrowed(&types),
                window.limit,
            )
            .await
        );
        out.insert(dimension.to_owned(), json!(split));
    }
    ok(Value::Object(out))
}

async fn delivery(State(state): State<Arc<Analytics>>, RawQuery(raw): RawQuery) -> Response {
    let window = Window::parse(raw.as_deref(), now_ms());
    let measured = unwrap_or_fail!(
        traffic::beacon_delivery(&state.analytics, window.range, &window.filters).await
    );
    // `ratio` is computed once, here, from the one implementation of the rule
    // that a ratio out of zero is absent rather than zero (ANA-10).
    ok(json!({
        "serverPageViews": measured.server_page_views,
        "clientPageLoads": measured.client_page_loads,
        "ratio": measured.ratio(),
    }))
}

async fn horizon(State(state): State<Arc<Analytics>>) -> Response {
    let reach =
        unwrap_or_fail!(retention::horizon(&state.analytics, state.retention, now_ms()).await);
    ok(serde_json::to_value(reach).unwrap_or_else(|_| json!({})))
}

// ---- search (ANA-20) ----

async fn queries(State(state): State<Arc<Analytics>>, RawQuery(raw): RawQuery) -> Response {
    let window = Window::parse(raw.as_deref(), now_ms());
    let found = unwrap_or_fail!(
        search::queries(&state.analytics, window.range, &window.filters, window.limit).await
    );
    ok(json!({ "queries": found }))
}

async fn search_pages(State(state): State<Arc<Analytics>>, RawQuery(raw): RawQuery) -> Response {
    let window = Window::parse(raw.as_deref(), now_ms());
    let found = unwrap_or_fail!(
        search::per_page(&state.analytics, window.range, &window.filters, window.limit).await
    );
    ok(json!({ "pages": found }))
}

async fn trending(State(state): State<Arc<Analytics>>, RawQuery(raw): RawQuery) -> Response {
    let window = Window::parse(raw.as_deref(), now_ms());
    let found = unwrap_or_fail!(
        search::trending(&state.analytics, window.range, &window.filters, window.limit).await
    );
    ok(json!({ "trending": found }))
}

// ---- assistant, insights, settings ----

async fn assistant_summary(
    State(state): State<Arc<Analytics>>,
    RawQuery(raw): RawQuery,
) -> Response {
    let window = Window::parse(raw.as_deref(), now_ms());
    let found = unwrap_or_fail!(
        assistant::summary(&state.analytics, window.range, &window.filters, window.limit).await
    );
    ok(serde_json::to_value(found).unwrap_or_else(|_| json!({})))
}

async fn cards(State(state): State<Arc<Analytics>>, RawQuery(raw): RawQuery) -> Response {
    let window = Window::parse(raw.as_deref(), now_ms());
    let computed = unwrap_or_fail!(
        insights::compute(&state.inputs(), window.range, &window.filters, now_ms()).await
    );
    ok(json!({ "cards": computed }))
}

/// What the dashboard posts when someone presses a card's button.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActInput {
    pub kind: String,
    pub target: String,
}

async fn act(
    State(state): State<Arc<Analytics>>,
    axum::Json(input): axum::Json<ActInput>,
) -> Response {
    // A closed set. An action name the dashboard does not have is a 400, not a
    // job nobody asked for.
    let job = match input.kind.as_str() {
        "create_page" => actions::create_page_for_query(&input.target, state.project),
        "fix_page" => actions::fix_page(&input.target, "dashboard", state.project),
        other => {
            return (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "application/problem+json")],
                json!({
                    "title": "Not an action",
                    "detail": format!("`{other}` is not an action this dashboard offers"),
                    "status": 400,
                })
                .to_string(),
            )
                .into_response();
        }
    };
    let queued = unwrap_or_fail!(liyasa_store::jobs::Jobs::new(state.app.clone()).enqueue(&job).await);
    ok(json!({ "jobId": queued.id().to_string(), "name": job.name }))
}

async fn vendors(State(state): State<Arc<Analytics>>) -> Response {
    let (enabled, unknown) = integrations::configure(&state.integrations);
    let stuck: Vec<&str> = integrations::gated_without_a_provider(&enabled)
        .iter()
        .map(|c| c.key.as_str())
        .collect();
    let provider = integrations::consent_provider(&enabled).map(|c| c.key.clone());
    ok(json!({
        "enabled": enabled
            .iter()
            .map(|c| json!({
                "key": c.key,
                "name": c.name,
                "consent": c.consent,
                "loadsBeforeConsent": c.loads_before_consent(),
            }))
            .collect::<Vec<_>>(),
        "provider": provider,
        "stuck": stuck,
        "unknown": unknown.iter().map(|u| u.0.clone()).collect::<Vec<_>>(),
        "consentStatement": privacy::CONSENT_STATEMENT,
        "inventory": privacy::INVENTORY,
    }))
}

/// The weekly digest, rendered rather than sent (ANA-42). Delivery is
/// `liyasa-server`'s: the only crate that may open a socket is `liyasa-net`.
pub async fn weekly_digest(
    state: &Analytics,
    week_starting: i64,
    filters: &Filters,
) -> Result<digest::Digest, StoreError> {
    digest::weekly(&state.inputs(), &state.site, week_starting, filters).await
}

/// One retention pass (ANA-06), for the scheduled job to call.
pub async fn sweep(
    state: &Analytics,
    totals: Option<&dyn retention::TotalsSink>,
) -> Result<retention::SweepReport, StoreError> {
    retention::sweep(&state.analytics, state.retention, now_ms(), totals).await
}

/// The feedback reports of ANA-30, which read the application database.
pub async fn feedback_by_page(
    state: &Analytics,
    range: Range,
    limit: i64,
) -> Result<Vec<feedback::PageRating>, StoreError> {
    feedback::by_page(&state.app, range, limit).await
}
