//! The analytics beacon (ANA-01, ANA-03, ANA-09).
//!
//! One endpoint, `POST /_liyasa/e`, for the events a server cannot see for
//! itself: scroll depth, copy actions, tab choices. It sets no cookie, stores
//! no address, and never waits on the database — the event goes into the
//! bounded queue and the response is sent.
//!
//! The same endpoint is the whole server in `--collector-only` mode, which is
//! what a static site posts to (ANA-09).

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use http::{HeaderValue, StatusCode, header};
use liyasa_store::records::EventRecord;
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use super::problem::Problem;
use super::session;

/// 64 KB is far more than a beacon needs and far less than a body worth
/// buffering.
pub const MAX_BODY: usize = 64 * 1024;

/// What the reader runtime posts. Everything the server can determine for
/// itself is ignored when the client sends it: the client does not get to
/// choose its own session key, its caller kind, or the time.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventInput {
    #[serde(rename = "type")]
    pub kind: String,
    pub route: String,
    #[serde(default)]
    pub props: Value,
    #[serde(default)]
    pub duration_ms: Option<u32>,
    /// Static sites say which site they are; a served site knows.
    #[serde(default)]
    pub site: Option<String>,
    #[serde(default)]
    pub variant: Value,
}

/// One event or a batch; a beacon sent on page hide carries several.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Body {
    One(EventInput),
    Many(Vec<EventInput>),
}

impl Body {
    fn into_vec(self) -> Vec<EventInput> {
        match self {
            Self::One(one) => vec![one],
            Self::Many(many) => many,
        }
    }
}

/// The types a browser may report. A client cannot post a `deployment` event
/// and have it counted: server-side types are emitted by the server.
const CLIENT_TYPES: &[&str] = &[
    "scroll_depth",
    "time_on_page",
    "toc_click",
    "tab_select",
    "code_group_select",
    "copy_code",
    "outbound_click",
    "page_load",
    "search_click",
    "feedback_shown",
];

pub fn is_client_type(kind: &str) -> bool {
    CLIENT_TYPES.contains(&kind)
}

/// Builds the stored event. The caller supplies only what it can see; the
/// rest is the server's (ANA-02, ANA-03).
#[allow(clippy::too_many_arguments)]
pub fn record(
    state: &AppState,
    input: &EventInput,
    addr: std::net::IpAddr,
    user_agent: Option<&str>,
    referrer: Option<&str>,
    country: Option<String>,
) -> EventRecord {
    let (kind, agent_name) = session::classify(user_agent, false);
    let site = input
        .site
        .clone()
        .unwrap_or_else(|| state.config.site.clone());
    let route = match input.route.split_once('?') {
        Some((path, query)) => match session::allowed_query(query) {
            Some(kept) => format!("{path}?{kept}"),
            None => path.to_owned(),
        },
        None => input.route.clone(),
    };
    EventRecord {
        ts: liyasa_store::now_ms(),
        site: site.clone(),
        env: state.config.env.clone(),
        route,
        kind: input.kind.clone(),
        variant: input.variant.clone(),
        caller: json!({ "kind": kind.as_str(), "agent_name": agent_name }),
        format: "html".to_owned(),
        session_key: state.salt.key(addr, user_agent, &site),
        referrer_host: session::referrer_host(referrer),
        device: json!({
            "class": session::device_class(user_agent),
            "os_family": null,
            "browser_family": session::ua_family(user_agent),
        }),
        country,
        duration_ms: input.duration_ms,
        props: state.scrub_value(&input.props),
    }
}

/// `POST /_liyasa/e`.
pub async fn ingest(
    State(state): State<Arc<AppState>>,
    request: axum::extract::Request,
) -> Response {
    let (parts, body) = request.into_parts();

    // ANA-09: a collector accepts events only for the origins it was given.
    let origin = parts
        .headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if !state.origin_allowed(origin.as_deref()) {
        return Problem::new(StatusCode::FORBIDDEN, "Origin not accepted")
            .detail("this collector does not accept events for that origin")
            .into_response();
    }

    if !state.config.analytics_enabled {
        // Turned off is not an error: the runtime keeps posting and the
        // server keeps discarding, which is cheaper than a retry loop.
        return no_content(origin.as_deref(), state.as_ref());
    }

    let bytes = match axum::body::to_bytes(body, MAX_BODY).await {
        Ok(bytes) => bytes,
        Err(_) => return Problem::too_large(MAX_BODY as u64).into_response(),
    };
    let parsed: Body = match serde_json::from_slice(&bytes) {
        Ok(parsed) => parsed,
        Err(error) => return Problem::bad_request(error.to_string()).into_response(),
    };

    let addr = state.client_ip(&parts);
    let user_agent = parts
        .headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let referrer = parts
        .headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok());
    let country = state.region(&parts.headers);

    let mut accepted = 0usize;
    let mut refused = 0usize;
    for input in parsed.into_vec() {
        if !is_client_type(&input.kind) {
            refused += 1;
            continue;
        }
        let event = record(
            state.as_ref(),
            &input,
            addr,
            user_agent,
            referrer,
            country.clone(),
        );
        if state.ingest.push(event).is_err() {
            // ANA-08: a full queue drops rather than blocks, and the metric
            // counts it. The client is told nothing: a beacon has no retry.
            refused += 1;
        } else {
            accepted += 1;
        }
    }
    tracing::debug!(target: "liyasa_server", accepted, refused, "beacon");
    no_content(origin.as_deref(), state.as_ref())
}

/// `204` with no body: a beacon is fire and forget, and an empty body is the
/// cheapest thing `navigator.sendBeacon` can be given.
fn no_content(origin: Option<&str>, state: &AppState) -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    if let Some(origin) = origin
        && state.config.collector_only
        && let Ok(value) = HeaderValue::from_str(origin)
    {
        // Only the collector answers cross-origin, and only for an origin it
        // was configured with.
        let headers = response.headers_mut();
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
        headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    response
}

/// The preflight a cross-origin beacon sends when it carries a JSON content
/// type.
pub async fn preflight(
    State(state): State<Arc<AppState>>,
    request: axum::extract::Request,
) -> Response {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if !state.origin_allowed(origin.as_deref()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    let headers = response.headers_mut();
    if let Some(value) = origin.and_then(|o| HeaderValue::from_str(&o).ok()) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type"),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("86400"),
    );
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_types_a_browser_can_observe_are_accepted() {
        assert!(is_client_type("scroll_depth"));
        assert!(is_client_type("copy_code"));
        assert!(
            !is_client_type("deployment"),
            "a client cannot forge a deployment event"
        );
        assert!(
            !is_client_type("page_view"),
            "views are counted server-side"
        );
        assert!(!is_client_type("feedback"));
    }

    #[test]
    fn a_batch_and_a_single_event_both_parse() {
        let one: Body = serde_json::from_str(r#"{"type":"copy_code","route":"/a"}"#).expect("one");
        assert_eq!(one.into_vec().len(), 1);
        let many: Body = serde_json::from_str(
            r#"[{"type":"copy_code","route":"/a"},{"type":"tab_select","route":"/a"}]"#,
        )
        .expect("many");
        assert_eq!(many.into_vec().len(), 2);
    }
}
