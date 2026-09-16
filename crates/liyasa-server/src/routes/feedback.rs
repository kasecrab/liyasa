//! Reader and agent feedback (RX-50, RX-51, RX-52, RX-53).
//!
//! Feedback text is untrusted input at trust level `anonymous` (§30.2.2): it
//! is rate limited under its own pool, size capped, scrubbed before it is
//! stored, and returned to the dashboard as plain text with a flag saying so.
//! Nothing here renders it, and nothing here stores who sent it.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use liyasa_store::records::{FeedbackKind, FeedbackRecord, FeedbackStatus};
use liyasa_store::repos::FeedbackQuery;
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use super::api::{Json, JsonStatus, PageParams, Paged};
use super::problem::Problem;

/// RX-52: 4 KB, counted on the whole body so a client cannot split a large
/// payload across fields.
pub const MAX_BODY: usize = 4 * 1024;

/// RX-50's list. An unknown category is refused rather than stored, so the
/// dashboard's filter always has a fixed set.
pub const CATEGORIES: &[&str] = &["inaccurate", "unclear", "missing", "outdated", "other"];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackInput {
    pub route: String,
    /// `page`, `code`, or `agent`; a report from an agent is the `agent` kind.
    #[serde(default)]
    pub kind: Option<String>,
    /// `1` or `-1`.
    #[serde(default)]
    pub rating: Option<i32>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    /// RX-51: which code block the thumb was on.
    #[serde(default)]
    pub block_id: Option<String>,
    /// RX-52: what the agent was trying to do when the page failed it.
    #[serde(default)]
    pub task: Option<String>,
}

fn parse_kind(text: Option<&str>) -> Option<FeedbackKind> {
    match text {
        None | Some("page") => Some(FeedbackKind::Page),
        Some("code") => Some(FeedbackKind::Code),
        Some("agent") => Some(FeedbackKind::Agent),
        Some(_) => None,
    }
}

fn parse_status(text: &str) -> Option<FeedbackStatus> {
    match text {
        "open" => Some(FeedbackStatus::Open),
        "triaged" => Some(FeedbackStatus::Triaged),
        "resolved" => Some(FeedbackStatus::Resolved),
        _ => None,
    }
}

fn kind_text(kind: FeedbackKind) -> &'static str {
    match kind {
        FeedbackKind::Page => "page",
        FeedbackKind::Code => "code",
        FeedbackKind::Agent => "agent",
    }
}

/// The dashboard's view of one row. `textIsPlain` is not decoration: it is the
/// contract that the dashboard must not render this as Markdown (RX-52).
pub fn to_json(record: &FeedbackRecord) -> Value {
    json!({
        "id": record.id,
        "route": record.route,
        "kind": kind_text(record.kind),
        "rating": record.rating,
        "category": record.category,
        "text": record.text,
        "textIsPlain": true,
        "blockId": record.block_id,
        "task": record.task,
        "status": liyasa_store::repos::feedback_status_text(record.status),
        "notes": record.notes,
        "createdAt": record.created_at,
        "updatedAt": record.updated_at,
    })
}

/// `POST /_liyasa/feedback`.
pub async fn submit(
    State(state): State<Arc<AppState>>,
    request: axum::extract::Request,
) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Feedback is not configured",
        )
        .detail("this instance has no store")
        .into_response();
    };
    let (parts, body) = request.into_parts();
    let idempotency_key = parts
        .headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if let Some(key) = &idempotency_key
        && let Some((status, body)) = state.idempotency.get("/_liyasa/feedback", key)
    {
        return JsonStatus(status, body).into_response();
    }

    let bytes = match axum::body::to_bytes(body, MAX_BODY).await {
        Ok(bytes) => bytes,
        Err(_) => return Problem::too_large(MAX_BODY as u64).into_response(),
    };
    let input: FeedbackInput = match serde_json::from_slice(&bytes) {
        Ok(input) => input,
        Err(error) => return Problem::bad_request(error.to_string()).into_response(),
    };

    let Some(kind) = parse_kind(input.kind.as_deref()) else {
        return Problem::bad_request("`kind` is one of page, code, agent").into_response();
    };
    if let Some(category) = &input.category
        && !CATEGORIES.contains(&category.as_str())
    {
        return Problem::bad_request(format!("`category` is one of {}", CATEGORIES.join(", ")))
            .into_response();
    }
    if input.rating.is_some_and(|r| r != 1 && r != -1) {
        return Problem::bad_request("`rating` is 1 or -1").into_response();
    }
    if input.route.is_empty() || !input.route.starts_with('/') {
        return Problem::bad_request("`route` is a site-relative path").into_response();
    }
    if kind == FeedbackKind::Agent && input.task.is_none() && input.text.is_none() {
        return Problem::bad_request("an agent report carries `task` or `text`").into_response();
    }

    let now = liyasa_store::now_ms();
    let record = FeedbackRecord {
        id: format!("fb_{}", liyasa_store::new_ulid()),
        project: None,
        route: input.route.clone(),
        kind,
        rating: input.rating,
        category: input.category.clone(),
        // Scrubbed before it is stored, never after: a secret that reached
        // the table has already leaked (§30.2.4).
        text: input.text.as_deref().map(|t| state.scrub(t)),
        block_id: input.block_id.clone(),
        task: input.task.as_deref().map(|t| state.scrub(t)),
        status: FeedbackStatus::Open,
        notes: String::new(),
        created_at: now,
        updated_at: now,
    };
    if let Err(error) = store.feedback().insert(&record).await {
        return Problem::store(&error).into_response();
    }

    // The feedback event is one the server emits: it is not client-reportable
    // and it is never dropped under pressure (ANA-08).
    let _ = state.ingest.push(liyasa_store::records::EventRecord {
        ts: now,
        site: state.config.site.clone(),
        env: state.config.env.clone(),
        route: record.route.clone(),
        kind: "feedback".to_owned(),
        caller: json!({ "kind": if kind == FeedbackKind::Agent { "agent" } else { "human" } }),
        format: "html".to_owned(),
        props: json!({ "rating": record.rating, "category": record.category, "feedbackKind": kind_text(kind) }),
        ..Default::default()
    });

    let body = json!({ "id": record.id, "status": "open" });
    if let Some(key) = &idempotency_key {
        state
            .idempotency
            .put("/_liyasa/feedback", key, StatusCode::CREATED, &body);
    }
    state.notify_webhook(
        "feedback.received",
        &json!({ "feedback": to_json(&record) }),
    );
    JsonStatus(StatusCode::CREATED, body).into_response()
}

/// `serde_urlencoded` does not support `flatten`, so the paging parameters
/// are spelled out rather than borrowed from `PageParams`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListParams {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    pub route: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
}

impl ListParams {
    fn page(&self) -> liyasa_core::store::Page {
        PageParams {
            cursor: self.cursor.clone(),
            limit: self.limit,
        }
        .to_page()
    }
}

/// `GET /_liyasa/feedback` (RX-53).
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let page = params.page();
    let query = FeedbackQuery {
        route: params.route.clone(),
        kind: match params.kind.as_deref() {
            Some(text) => match parse_kind(Some(text)) {
                Some(kind) => Some(kind),
                None => return Problem::bad_request("unknown `kind`").into_response(),
            },
            None => None,
        },
        status: match params.status.as_deref() {
            Some(text) => match parse_status(text) {
                Some(status) => Some(status),
                None => return Problem::bad_request("unknown `status`").into_response(),
            },
            None => None,
        },
    };
    match store.feedback().list(&query, &page).await {
        Ok(rows) => {
            let limit = page.limit;
            Paged::new(rows.iter().map(to_json).collect::<Vec<_>>(), limit, |row| {
                row["id"].as_str().unwrap_or_default().to_owned()
            })
            .into_response()
        }
        Err(error) => Problem::store(&error).into_response(),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusInput {
    pub status: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// `PATCH /_liyasa/feedback/{id}` (RX-53's workflow).
pub async fn set_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    axum::Json(input): axum::Json<StatusInput>,
) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let Some(status) = parse_status(&input.status) else {
        return Problem::bad_request("`status` is one of open, triaged, resolved").into_response();
    };
    // An internal note is operator text, but it lands next to anonymous text
    // in the same column, so it goes through the same scrubber.
    let note = input.note.as_deref().map(|n| state.scrub(n));
    match store
        .feedback()
        .set_status(&id, status, note.as_deref())
        .await
    {
        Ok(()) => match store.feedback().get(&id).await {
            Ok(Some(record)) => Json(to_json(&record)).into_response(),
            Ok(None) => Problem::not_found("feedback").into_response(),
            Err(error) => Problem::store(&error).into_response(),
        },
        Err(liyasa_core::store::StoreError::NotFound) => {
            Problem::not_found("feedback").into_response()
        }
        Err(error) => Problem::store(&error).into_response(),
    }
}

/// `GET /_liyasa/feedback/summary?route=` — the per-page ratio the reader
/// runtime and the dashboard both show (RX-50, ANA-30).
pub async fn summary(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Response {
    let Some(store) = state.store.clone() else {
        return Problem::not_found("store").into_response();
    };
    let Some(route) = params.route else {
        return Problem::bad_request("`route` is required").into_response();
    };
    match store.feedback().ratio(&route).await {
        Ok((up, down)) => Json(json!({ "route": route, "up": up, "down": down })).into_response(),
        Err(error) => Problem::store(&error).into_response(),
    }
}

/// `llms.txt` documents the agent channel; this is the fragment that
/// describes it, so the text and the route can never disagree (RX-52).
pub fn llms_txt_fragment(origin: &str) -> String {
    format!(
        "## Reporting a problem\n\n\
         If a page on this site failed the task you were doing, report it:\n\n\
         POST {origin}/_liyasa/feedback\n\
         Content-Type: application/json\n\n\
         {{\"route\": \"/the/page\", \"kind\": \"agent\", \"task\": \"what you were trying to do\", \
         \"text\": \"what was wrong or missing\"}}\n\n\
         The body is capped at 4 KB and the endpoint is rate limited. No \
         identity is required and none is recorded.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_categories_are_the_documented_five() {
        assert_eq!(
            CATEGORIES,
            ["inaccurate", "unclear", "missing", "outdated", "other"]
        );
    }

    #[test]
    fn a_kind_defaults_to_page_and_an_unknown_one_is_refused() {
        assert_eq!(parse_kind(None), Some(FeedbackKind::Page));
        assert_eq!(parse_kind(Some("agent")), Some(FeedbackKind::Agent));
        assert_eq!(parse_kind(Some("shout")), None);
    }

    #[test]
    fn the_dashboard_row_says_the_text_is_plain() {
        let record = FeedbackRecord {
            id: "fb_1".to_owned(),
            project: None,
            route: "/install".to_owned(),
            kind: FeedbackKind::Agent,
            rating: None,
            category: None,
            text: Some("[click here](javascript:alert(1))".to_owned()),
            block_id: None,
            task: Some("install the CLI".to_owned()),
            status: FeedbackStatus::Open,
            notes: String::new(),
            created_at: 1,
            updated_at: 1,
        };
        let row = to_json(&record);
        assert_eq!(row["textIsPlain"], true);
        assert_eq!(row["kind"], "agent");
        assert_eq!(
            row["text"], "[click here](javascript:alert(1))",
            "the text is stored and returned verbatim; it is the renderer's job never to render it"
        );
    }

    #[test]
    fn the_llms_txt_fragment_names_the_endpoint_and_the_cap() {
        let fragment = llms_txt_fragment("https://docs.example.com");
        assert!(fragment.contains("POST https://docs.example.com/_liyasa/feedback"));
        assert!(fragment.contains("4 KB"));
        assert!(fragment.contains("\"kind\": \"agent\""));
    }
}
