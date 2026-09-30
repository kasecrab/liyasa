//! `POST /_liyasa/assistant` (AST-11, AST-12, defect 146).
//!
//! The retrieval filter was written, tested and unreachable: `ReaderContext`
//! builds the `ChunkQuery` the store applies during retrieval (RFC 1807), and
//! nothing outside a test ever constructed one. This is the handler that does,
//! which is the whole of defect 146 — the policy was never the missing half.
//!
//! **Entitlements come from the session, never from the body.** `groups` and
//! `region` are read off the request's [`Principal`]; the body carries only
//! what a reader may choose for themselves — where they are, what they had
//! selected, which version and locale they are reading. A body field for
//! `groups` would be a reader naming their own entitlements, which is the
//! disclosure the filter exists to prevent, so there is no such field and
//! `ask_does_not_take_entitlements_from_the_body` is the test that keeps it
//! that way.
//!
//! The same decision `routes::search` reaches for a query, by the same
//! reasoning and from the same `Principal`.

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use http::{StatusCode, header};
use liyasa_ai::assistant::ReaderContext;
use liyasa_ai::assistant::tools::Tools;
use liyasa_ai::config::AiConfig;
use liyasa_core::ids::{Locale, Route, Version};
use serde::Deserialize;
use serde_json::json;

use crate::assistant::gate::GatedTools;
use crate::auth::session::Principal;
use crate::routes::AppState;
use crate::routes::bundle::Bundle;
use crate::routes::mount::Mount;
use crate::routes::tools::ServerTools;

/// The path AST-20's `rateLimits` charges and `pool_for` buckets.
///
/// Declared for callers that need to name it; the `.route` below repeats the
/// literal on purpose. `tests/server/no_caller_ratchet.rs` finds a mounted
/// surface by reading the first string literal after each `.route(`, so
/// `.route(PATH, ..)` is invisible to it — the census went on reporting this
/// pool as unrouted with the router sitting right here, which is the failure
/// mode that census exists to prevent, achieved by satisfying it.
pub const PATH: &str = "/_liyasa/assistant";

/// Built once at mount rather than per request: reading `ai` out of the site
/// config parses the whole object, and it cannot change without a deploy.
pub struct AssistantState {
    pub app: Arc<AppState>,
    pub bundle: Arc<Bundle>,
    pub config: AiConfig,
}

/// What a reader may say about themselves.
///
/// Deliberately without `groups` or `region`: see the module note.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AskBody {
    question: String,
    current_page: Option<String>,
    selection: Option<String>,
    version: Option<String>,
    locale: Option<String>,
    // No `threadId`: AST-16 keeps the transcript in the browser until the
    // server half exists, and a field this handler deserializes and never
    // reads would read as conversation memory that works. Serde ignores keys
    // it does not name, so a client that sends one is not broken by its
    // absence.
}

/// The reader asking, from the session for anything that decides what they may
/// see and from the body for everything else.
fn reader_context(body: &AskBody, principal: Option<&Principal>) -> ReaderContext {
    let (groups, region, signed_in) = match principal {
        Some(principal) => (
            principal.groups.iter().cloned().collect(),
            principal.region.clone(),
            true,
        ),
        // An anonymous reader has no groups, which the filter reads as public
        // content only.
        None => (Vec::new(), None, false),
    };
    ReaderContext {
        current_page: body.current_page.as_deref().map(Route::new),
        selection: body.selection.clone(),
        version: body.version.as_deref().map(Version::new),
        locale: body.locale.as_deref().map(Locale::new),
        region,
        groups,
        signed_in,
    }
}

/// Reader-dependent by construction, so a shared cache holding one reader's
/// answer is the disclosure the filter exists to avoid (AUTH-13).
fn private(status: StatusCode, body: serde_json::Value) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_owned()),
    )
        .into_response()
}

pub async fn ask(
    State(state): State<Arc<AssistantState>>,
    request: http::Request<axum::body::Body>,
) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return private(
                StatusCode::PAYLOAD_TOO_LARGE,
                json!({ "error": "the question is larger than this endpoint reads" }),
            );
        }
    };
    let body: AskBody = match serde_json::from_slice(&bytes) {
        Ok(body) => body,
        Err(e) => {
            return private(
                StatusCode::BAD_REQUEST,
                json!({ "error": format!("this is not a question this endpoint reads: {e}") }),
            );
        }
    };
    if body.question.trim().is_empty() {
        return private(
            StatusCode::BAD_REQUEST,
            json!({ "error": "`question` is required" }),
        );
    }

    let principal = parts.extensions.get::<Principal>();
    let reader = reader_context(&body, principal);

    // `availability` is the operator's answer to who may ask at all, and it is
    // asked before anything is retrieved.
    if !reader.may_use(state.config.assistant.availability) {
        return private(
            StatusCode::FORBIDDEN,
            json!({
                "error": "this site's assistant is not available to you",
                "availability": format!("{:?}", state.config.assistant.availability).to_lowercase(),
            }),
        );
    }

    // Two different questions, and both have to be asked. `reader.query()`
    // carries the groups the vector store applies during retrieval (RFC 1807);
    // `GatedTools` gates the three tools that read the bundle and never reach
    // the store. Passing only the first would have served every restricted
    // page's Markdown through `get_page`.
    let mut inner = ServerTools::new(state.bundle.clone(), reader.query());
    if let Some(route) = reader.current_page.clone() {
        inner = inner.on_page(route);
    }
    let tools = GatedTools::new(
        inner,
        state.bundle.clone(),
        crate::mcp::http::site_default(&state.app),
        principal.cloned(),
    );

    // No model is constructed anywhere in this crate yet, so `ask` cannot run
    // and this endpoint answers with what the tools can reach on their own:
    // the reader's page and the navigation they may see. The same choice
    // `mcp::Host::ask_degraded` makes — say the answer is passages rather than
    // prose, in the payload, instead of returning prose that was never
    // written. `passages` is a real retrieval under this reader's
    // entitlements, which is the half defect 146 was about.
    let current = match tools.get_current_page().await {
        Ok(page) => page,
        Err(e) => {
            return private(
                StatusCode::BAD_GATEWAY,
                json!({ "error": format!("this site's content could not be read: {e:?}") }),
            );
        }
    };
    let navigation = tools.list_navigation().await.unwrap_or_default();
    let hits = tools
        .search(&body.question, &reader.query())
        .await
        .unwrap_or_default();

    private(
        StatusCode::OK,
        json!({
            "degraded": "no model is configured for `ai.models.assistant`, so this answers with \
                         retrieved passages under the reader's entitlements rather than written \
                         prose",
            "answer": null,
            "passages": hits
                .iter()
                .map(|hit| {
                    json!({
                        "route": hit.record.route,
                        "anchor": hit.record.anchor,
                        "title": hit.record.title,
                        "score": hit.score,
                    })
                })
                .collect::<Vec<_>>(),
            "currentPage": current.map(|page| json!({ "route": page.route, "title": page.title })),
            "navigation": navigation
                .iter()
                .map(|entry| json!({ "route": entry.route, "title": entry.title }))
                .collect::<Vec<_>>(),
        }),
    )
}

pub fn router(state: Arc<AssistantState>) -> Router {
    Router::new()
        .route("/_liyasa/assistant", post(ask))
        .with_state(state)
}

/// WP-18's subtree (RFC 1403).
pub fn mount(app: &Arc<AppState>) -> Mount {
    let Some(bundle) = app.bundle.clone() else {
        return Mount::skipped("this instance serves no site, and every assistant tool reads one");
    };
    let config = match AiConfig::from_site(&app.config.site_config) {
        Ok(config) => config,
        Err(e) => {
            return Mount::skipped(format!("this site's `ai` configuration did not parse: {e}"));
        }
    };
    if !config.assistant.enabled {
        return Mount::skipped("`ai.assistant.enabled` is false in this site's configuration");
    }
    Mount::routes(router(Arc::new(AssistantState {
        app: app.clone(),
        bundle,
        config,
    })))
}
