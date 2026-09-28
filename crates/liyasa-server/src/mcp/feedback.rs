//! Where `report_issue` writes (MCP-03).
//!
//! Into the feedback table, as `FeedbackKind::Agent` — the shape RX-52
//! already defines for "an agent reporting a page that failed it". A second
//! table for the same fact would give the maintainers two inboxes and a
//! reason to read neither.
//!
//! Three properties MCP-03 asks for, and where each one is:
//!
//! - **Unauthenticated by design, at `anonymous` trust.** Nothing here asks
//!   for a session. The trust level is fixed in
//!   [`super::http::McpState::scope`] and never rises.
//! - **Size capped.** In [`super::tools::read_issue`], before anything is
//!   written, on the whole submission rather than per field.
//! - **Scrubbed.** Every free-text field goes through `AppState::scrub`
//!   before it is stored, never after: a secret that reached the table has
//!   already leaked (§30.2.4).
//!
//! What it deliberately does not do is return anything an anonymous caller
//! could use to read the report back. The receipt is the record's own id, for
//! quoting to a maintainer; there is no tool that fetches one.

use std::sync::Arc;

use liyasa_core::net::BoxFut;
use liyasa_store::records::{FeedbackKind, FeedbackRecord, FeedbackStatus};
use serde_json::json;

use super::http::McpState;
use super::reader::{Scope, ToolFailure};
use super::tools::{Issue, Issues};
use crate::routes::AppState;

/// The feedback table as an [`Issues`] sink.
pub struct StoreIssues {
    app: Arc<AppState>,
    state: Arc<McpState>,
}

impl StoreIssues {
    pub fn new(app: Arc<AppState>, state: Arc<McpState>) -> Self {
        Self { app, state }
    }
}

impl Issues for StoreIssues {
    fn report<'a>(
        &'a self,
        issue: &'a Issue,
        scope: &'a Scope,
    ) -> BoxFut<'a, Result<String, ToolFailure>> {
        Box::pin(async move {
            let Some(store) = self.app.store.clone() else {
                return Err(ToolFailure::Unavailable(
                    "this instance has no store configured, so there is nowhere to file a \
                     report"
                        .to_owned(),
                ));
            };
            // A report about a page the caller may not read is a report about
            // a page they should not know exists, and the route would go
            // into a maintainer's inbox as evidence they enumerated it.
            let route = match &issue.route {
                Some(route) => match self.state.reader.page(route, None, scope) {
                    Ok(page) => page.route,
                    Err(_) => {
                        return Err(ToolFailure::BadInput(format!(
                            "`{route}` is not a page of this site; omit `route` if the report \
                             is not about one"
                        )));
                    }
                },
                None => "/".to_owned(),
            };

            let now = liyasa_store::now_ms();
            let record = FeedbackRecord {
                id: format!("fb_{}", liyasa_store::new_ulid()),
                project: None,
                route,
                kind: FeedbackKind::Agent,
                // RX-52 reserves the rating for a human's thumb.
                rating: None,
                category: None,
                text: Some(self.app.scrub(&issue.detail)).filter(|text| !text.is_empty()),
                block_id: None,
                task: Some(self.app.scrub(&issue.summary)),
                status: FeedbackStatus::Open,
                notes: String::new(),
                created_at: now,
                updated_at: now,
            };
            store
                .feedback()
                .insert(&record)
                .await
                .map_err(|error| ToolFailure::Internal(error.to_string()))?;

            self.app.notify_webhook(
                "feedback.received",
                &json!({ "feedback": { "id": record.id, "route": record.route, "kind": "agent" } }),
            );
            Ok(format!(
                "Filed as `{}`. It is open for this site's maintainers; quote that id if you \
                 need to refer to it.",
                record.id
            ))
        })
    }
}
