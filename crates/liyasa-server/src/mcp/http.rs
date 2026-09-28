//! Streamable HTTP: the transport `liyasa serve` speaks (MCP-01).
//!
//! ## One endpoint, three methods
//!
//! POST carries a request and answers it. GET is where a server pushes
//! messages the client did not ask for, and this server has none to push —
//! no subscriptions, no progress, no sampling — so it answers `405` with an
//! `Allow` header rather than holding a stream open that will never emit.
//! The specification permits exactly that, and a client that waits on an
//! empty stream looks to its user like a server that has hung. DELETE ends a
//! session and answers `204`, so a client's cleanup succeeds.
//!
//! ## No sessions
//!
//! Every request here is self-contained: the corpus is the build, the scope
//! comes from the request's own `Principal`, and nothing is carried between
//! calls. So no `Mcp-Session-Id` is issued, which the specification reads as
//! "this server does not use sessions" and clients handle by not sending one.
//! Issuing an id we then ignored would be worse than not issuing one — it
//! would promise continuity across the replicas a deployment has several of.
//!
//! ## Cross-origin
//!
//! Allowed, deliberately. The DNS-rebinding warning in the specification is
//! about a server bound to localhost holding a private corpus; this one is a
//! public documentation site whose every answer is already served, unchanged,
//! at a URL any browser can fetch. Refusing a browser-based agent would
//! withhold nothing and break the client MCP-01 most expects. A private site
//! still refuses per page, because [`super::reader::Scope`] carries the
//! session and `groups::decide` runs on every read.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::response::{IntoResponse, Response as HttpResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use http::{HeaderMap, StatusCode, header};
use liyasa_core::ai::TrustLevel;
use serde_json::{Value, json};

use super::jsonrpc::{self, Incoming};
use super::protocol::{self, Server};
use super::reader::{Scope, SiteReader};
use super::tools::Host;
use super::{discovery, feedback};
use crate::auth::session::Principal;
use crate::routes::AppState;

/// The canonical endpoint (RFC 1900).
pub const PATH: &str = "/mcp";

/// The alias `routes::pool_for` charges the `Mcp` rate-limit bucket on.
pub const ALIAS: &str = "/_liyasa/mcp";

/// The two discovery paths MCP-02 names. Both serve the same document,
/// because the ecosystem has used both and an agent that guesses wrong would
/// otherwise conclude the site has no server.
pub const WELL_KNOWN: &[&str] = &["/.well-known/mcp", "/.well-known/mcp.json"];

/// The largest MCP request body. A tool's arguments are a query and a route;
/// `report_issue` is capped smaller still by
/// [`super::tools::MAX_REPORT_BYTES`].
pub const MAX_BODY: usize = 256 * 1024;

/// What the subtree serves from.
pub struct McpState {
    pub app: Arc<AppState>,
    pub reader: Arc<dyn SiteReader>,
    /// The card `/.well-known/mcp` answers with, built once at startup.
    pub card: Value,
}

impl McpState {
    /// Who is asking, from the request's own session.
    ///
    /// Trust is `Anonymous` whatever the session says. An agent signed in as
    /// somebody may READ what that somebody may read — that is what `reader`
    /// carries — but its text is still a program's text, and §30.2.2 does not
    /// raise a caller's trust because it authenticated.
    fn scope(&self, principal: Option<&Principal>) -> Scope {
        Scope {
            site: site_default(&self.app),
            reader: principal.cloned(),
            trust: TrustLevel::Anonymous,
        }
    }
}

/// The site default this connection is served under.
///
/// `routes::site_default` is private to that module and this is the same
/// question, so it is asked the same way: a site with no `auth` subtree
/// mounted has no sessions to have and is public, which is what a bare
/// `liyasa serve` gets (AUTH-01).
pub fn site_default(app: &AppState) -> crate::auth::groups::SiteDefault {
    app.auth_state()
        .map(|auth| auth.site_default())
        .unwrap_or(crate::auth::groups::SiteDefault::Public)
}

/// The router for both spellings of the endpoint and both discovery paths.
///
/// **Written as string literals rather than as the constants beside them, and
/// not in a loop.** The route census in `tests/server/no_caller_ratchet.rs`
/// reads the first string literal after each `.route(` across this crate's
/// source; `.route(PATH,` would make it read the next unrelated literal in
/// this file and quietly corrupt the census that proves every rate-limit pool
/// has an endpoint. `the_constants_and_the_routes_agree` below is what keeps
/// the two spellings from drifting.
pub fn router(state: Arc<McpState>) -> Router {
    Router::new()
        .route("/mcp", post(handle).get(no_stream).delete(end_session))
        .route(
            "/_liyasa/mcp",
            post(handle).get(no_stream).delete(end_session),
        )
        .route("/.well-known/mcp", get(card))
        .route("/.well-known/mcp.json", get(card))
        .with_state(state)
}

async fn card(State(state): State<Arc<McpState>>) -> HttpResponse {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, discovery::CONTENT_TYPE),
            // A card is cheap to fetch and changes only when the site is
            // rebuilt, and an agent that re-reads it every call is the
            // traffic MCP-05's limits exist for.
            (header::CACHE_CONTROL, "public, max-age=300"),
        ],
        cors(),
        Json(state.card.clone()),
    )
        .into_response()
}

/// GET: no server-initiated stream. See the module comment.
async fn no_stream() -> HttpResponse {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::ALLOW, "POST, DELETE")],
        cors(),
        Json(json!({
            "error": "this server sends no messages a client did not ask for, so there is no \
                      stream to open; POST your requests to this same URL"
        })),
    )
        .into_response()
}

/// DELETE: there is no session to end, and saying so as `204` lets a client's
/// cleanup path succeed instead of reporting a failure at shutdown.
async fn end_session() -> HttpResponse {
    (StatusCode::NO_CONTENT, cors()).into_response()
}

async fn handle(
    State(state): State<Arc<McpState>>,
    request: axum::extract::Request,
) -> HttpResponse {
    let headers = request.headers().clone();
    let principal = request.extensions().get::<Principal>().cloned();
    // Read from the extensions rather than extracted: `ConnectInfo` is only
    // present when the binary served with `into_make_service_with_connect_info`,
    // and a handler that REQUIRED it would 500 under every test harness that
    // drives the router directly.
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    // The address the limiter and the session key are keyed on: the proxy
    // chain's client, not the socket's peer, wherever the deployment declared
    // its proxies (§30.2.5).
    let peer = state.app.proxies.client_ip(peer, &headers);

    if let Some(refusal) = protocol_version_refusal(&headers) {
        return refusal;
    }

    let bytes = match axum::body::to_bytes(request.into_body(), MAX_BODY).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return refuse(
                StatusCode::PAYLOAD_TOO_LARGE,
                jsonrpc::INVALID_REQUEST,
                format!("an MCP request may be {MAX_BODY} bytes"),
            );
        }
    };
    let body = match std::str::from_utf8(&bytes) {
        Ok(body) => body,
        Err(_) => {
            return refuse(
                StatusCode::BAD_REQUEST,
                jsonrpc::PARSE_ERROR,
                "the request body is not UTF-8".to_owned(),
            );
        }
    };

    let incoming = jsonrpc::parse(body);
    let request = match &incoming {
        Incoming::Call(request) | Incoming::Notify(request) => request,
        Incoming::Refuse(response) => {
            return (StatusCode::BAD_REQUEST, cors(), Json(json_of(response))).into_response();
        }
    };

    let sink = feedback::StoreIssues::new(state.app.clone(), state.clone());
    let server = Server {
        host: Host {
            reader: state.reader.as_ref(),
            // No model is wired here yet: the assistant's `ReaderContext` is
            // constructed only in tests (defect 146), so `ask` degrades and
            // says so rather than pretending. When WP-18 lands a constructor
            // this is where it goes, and nothing else changes.
            assistant: None,
            issues: Some(&sink),
        },
        scope: state.scope(principal.as_ref()),
    };

    let method = request.method.clone();
    let answered = protocol::dispatch(&server, request).await;
    record_call(&state, &method, &server.scope, peer, &headers);

    match answered {
        Some(response) => (StatusCode::OK, cors(), Json(json_of(&response))).into_response(),
        // A notification takes no response. `202` with an empty body is what
        // the specification asks for, and it is not the same as `204`: the
        // message was accepted, not "nothing happened".
        None => (StatusCode::ACCEPTED, cors(), Body::empty()).into_response(),
    }
}

/// MCP-05: every call is an analytics event with caller type `agent`.
///
/// `mcp_call` is already `EventClass::Critical`, so it survives the drop
/// order of ANA-08 — an MCP server whose usage disappears first under load is
/// one nobody can show is being used.
fn record_call(state: &McpState, method: &str, scope: &Scope, peer: IpAddr, headers: &HeaderMap) {
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok());
    let _ = state.app.ingest.push(liyasa_store::records::EventRecord {
        ts: liyasa_store::now_ms(),
        site: state.app.config.site.clone(),
        env: state.app.config.env.clone(),
        route: PATH.to_owned(),
        kind: "mcp_call".to_owned(),
        caller: json!({ "kind": "agent" }),
        format: "json".to_owned(),
        // The method, never the arguments: a question an agent asked is the
        // reader's text and §30.2.4 keeps it out of analytics.
        props: json!({ "method": method, "signedIn": scope.reader.is_some() }),
        session_key: state.app.salt.key(peer, user_agent, &state.app.config.site),
        ..Default::default()
    });
}

/// `MCP-Protocol-Version`, when the client sends one.
///
/// The specification says a client SHOULD send it after initializing and that
/// a server MUST answer `400` for a version it does not support. Absent is
/// not unsupported: a client that has not initialized yet has nothing to
/// send, and refusing it would refuse every first request.
fn protocol_version_refusal(headers: &HeaderMap) -> Option<HttpResponse> {
    let asked = headers.get("mcp-protocol-version")?.to_str().ok()?;
    if protocol::SUPPORTED.contains(&asked) {
        return None;
    }
    Some(refuse(
        StatusCode::BAD_REQUEST,
        jsonrpc::INVALID_REQUEST,
        format!(
            "this server speaks {}; the request asked for `{asked}`",
            protocol::SUPPORTED.join(", ")
        ),
    ))
}

fn refuse(status: StatusCode, code: i32, message: String) -> HttpResponse {
    let response = jsonrpc::Response::error(None, code, message);
    (status, cors(), Json(json_of(&response))).into_response()
}

fn json_of(response: &jsonrpc::Response) -> Value {
    serde_json::to_value(response).unwrap_or_else(|error| {
        json!({
            "jsonrpc": jsonrpc::VERSION,
            "id": Value::Null,
            "error": { "code": jsonrpc::INTERNAL_ERROR, "message": error.to_string() }
        })
    })
}

/// See the module comment for why this is open.
fn cors() -> [(header::HeaderName, &'static str); 4] {
    [
        (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        (header::ACCESS_CONTROL_ALLOW_METHODS, "GET, POST, DELETE"),
        (
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            "content-type, mcp-protocol-version, mcp-session-id",
        ),
        (header::ACCESS_CONTROL_EXPOSE_HEADERS, "mcp-session-id"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unsupported_protocol_version_is_refused_and_an_absent_one_is_not() {
        let mut headers = HeaderMap::new();
        assert!(protocol_version_refusal(&headers).is_none());

        headers.insert(
            "mcp-protocol-version",
            protocol::PROTOCOL_VERSION.parse().expect("a header value"),
        );
        assert!(protocol_version_refusal(&headers).is_none());

        headers.insert(
            "mcp-protocol-version",
            "1999-01-01".parse().expect("a header value"),
        );
        assert!(protocol_version_refusal(&headers).is_some());
    }

    /// `router` spells its paths as literals so the route census can see
    /// them; everything else in this module — the shadow check, the card's
    /// own endpoint URL, `pool_for`'s alias — uses the constants. This is
    /// where the two are held together.
    #[test]
    fn the_constants_and_the_routes_agree() {
        // `/mcp` is what every generated `llms.txt` publishes and
        // `/_liyasa/mcp` is what `pool_for` charges; serving one without the
        // other leaves either an agent on a 404 or agent traffic in the human
        // pool (RFC 1900).
        assert_eq!(PATH, "/mcp");
        assert_eq!(ALIAS, "/_liyasa/mcp");
        assert_eq!(WELL_KNOWN, ["/.well-known/mcp", "/.well-known/mcp.json"]);

        // And the literals in `router` are those four and nothing else. Read
        // out of this file's own source, because the failure being guarded
        // against is a literal changing in one place only.
        let source = include_str!("http.rs");
        let body = source
            .split("pub fn router(")
            .nth(1)
            .and_then(|rest| rest.split("\n}\n").next())
            .expect("`router` has a body");
        let mut routed: Vec<&str> = Vec::new();
        for call in body.split(".route(").skip(1) {
            let literal = call.split('"').nth(1).expect("a literal path");
            routed.push(literal);
        }
        routed.sort_unstable();
        let mut expected = vec![PATH, ALIAS];
        expected.extend_from_slice(WELL_KNOWN);
        expected.sort_unstable();
        assert_eq!(routed, expected);
    }
}
