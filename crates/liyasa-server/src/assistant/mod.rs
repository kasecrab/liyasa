//! What the server runs for the assistant's two jobs (RFC 1404, defect 130).
//!
//! RFC 1404 splits a job in two. `liyasa-ai` exports the work as plain
//! functions over its own types; this module is the thin half that builds
//! those types from [`AppState`] and calls them, because `run`'s signature
//! names `AppState` and `JobRecord` and `liyasa-server` is the crate that
//! depends on `liyasa-ai` rather than the other way round.
//!
//! **Both handlers can currently only skip, and that is the honest answer.**
//! `MemoryStore` is the one `VectorStore` in the workspace (defect 52) and
//! nothing stores a transcript, so the preconditions are absent rather than
//! broken. `Outcome::Skipped` completes the row with a reason an operator can
//! read, instead of failing it through the backoff ladder to `dead` — which is
//! what RFC 1404 added `Skipped` for. The same shape `routes::tools` already
//! uses for its `Option<Arc<dyn VectorStore>>`.

use std::sync::Arc;

use liyasa_ai::indexing::JobPayload;
use liyasa_store::Enqueue;
use liyasa_store::records::JobRecord;
use serde_json::json;

use crate::routes::AppState;
use crate::routes::work::{Outcome, Run};

/// The retention sweep's name. Unlike the index job, nothing else enqueues
/// this one, so the constant lives here with its only caller.
pub const RETENTION_JOB: &str = "assistant.retention";

const MS_PER_DAY: u64 = 24 * 60 * 60 * 1000;

/// Re-embeds what a deploy changed (AST-01).
///
/// The row is enqueued by `deploy::queue::queue_embedding`, which is why the
/// registration is `Trigger::Caller`: a `DeploymentSucceeded` trigger would
/// enqueue a *second* row and be saved only if it happened to rebuild the same
/// de-duplication key.
pub fn run_index<'a>(state: &'a Arc<AppState>, job: &'a JobRecord) -> Run<'a> {
    Box::pin(async move {
        // A payload this handler cannot read is a producer defect, not a
        // missing precondition, so it fails and is recorded rather than
        // quietly skipped.
        let payload = match serde_json::from_value::<JobPayload>(job.payload.clone()) {
            Ok(payload) => payload,
            Err(e) => {
                return Outcome::Failed(format!(
                    "the embedding job's payload is not what this build reads: {e}"
                ));
            }
        };
        if state.store.is_none() {
            return Outcome::Skipped("this instance has no store".to_owned());
        }
        // The vector index is the missing half. When one exists, this is where
        // `liyasa_ai::jobs::IndexJob` is built and run; the orchestration is
        // already there and tested against `MemoryStore`.
        Outcome::Skipped(format!(
            "no vector index is configured, so build {} cannot be embedded; \
             `sqlite-vec` is not yet wired (defect 52)",
            payload.deployment
        ))
    })
}

/// Deletes transcripts past their retention window (AST-22).
pub fn run_sweep<'a>(state: &'a Arc<AppState>, _job: &'a JobRecord) -> Run<'a> {
    Box::pin(async move {
        if state.store.is_none() {
            return Outcome::Skipped("this instance has no store".to_owned());
        }
        Outcome::Skipped(
            "no transcript store is configured, so there is nothing to sweep".to_owned(),
        )
    })
}

/// Enqueues the sweep once a day, at most once across every replica.
///
/// The key is the day bucket, so every replica may fire and `(name, key)`
/// being uniquely indexed over live rows means exactly one exists. That is
/// why `Trigger::Scheduled` carries no interval (RFC 1404).
pub fn retention_due(_state: &Arc<AppState>) -> Option<Enqueue> {
    let now = liyasa_store::now_ms().max(0) as u64;
    let day = now / MS_PER_DAY;
    Some(Enqueue {
        payload: json!({ "day": day }),
        max_attempts: 3,
        ..Enqueue::new(RETENTION_JOB, day.to_string())
    })
}
