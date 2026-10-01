//! VER-77's review reminders: the automation that sends what WP-20c builds.
//!
//! `liyasa-verify::drift::review` finds the pages past their cadence, records
//! them as ordinary drift candidates, and groups the open records by owner.
//! Sending is this package's, because only the server has a store that
//! persists between runs — WP-09 left the records half here deliberately, on
//! the grounds that the CLI rebuilds its graph per run, so every record would
//! be `created` and `resolved` always zero. A ledger that never closes
//! anything is worse than no ledger, because it looks like one.

use std::sync::Arc;

use liyasa_store::jobs::Enqueue;
use liyasa_store::records::JobRecord;
use liyasa_verify::drift::review::{self, Digest};
use serde_json::json;

use super::AppState;
use super::work::{JobKind, Outcome, Run};

pub const DIGEST_JOB: &str = "reviews.digest";

const MS_PER_DAY: i64 = 24 * 60 * 60 * 1000;

/// One digest a day, keyed on the day bucket so every replica may fire and the
/// store's unique index over live rows leaves exactly one (RFC 1404).
pub fn digest_due(_state: &Arc<AppState>) -> Option<Enqueue> {
    let day = liyasa_store::now_ms() / MS_PER_DAY;
    Some(Enqueue {
        payload: json!({ "day": day }),
        max_attempts: 3,
        ..Enqueue::new(DIGEST_JOB, format!("day:{day}"))
    })
}

/// The registration, as a const so `work.rs` names it in one line.
pub const DIGEST: JobKind = JobKind::scheduled(DIGEST_JOB, digest_due, run_digest);

/// Renders the reminders and hands them to whatever sends them.
///
/// Skips rather than fails when there is nothing to read or nowhere to send:
/// both are missing configuration, and a job whose precondition is absent
/// would otherwise burn its whole backoff ladder to reach `dead` and tell the
/// operator nothing.
pub fn run_digest<'a>(state: &'a Arc<AppState>, _job: &'a JobRecord) -> Run<'a> {
    Box::pin(async move {
        let Some(records) = state.drift_records() else {
            return Outcome::Skipped(
                "this instance keeps no drift records, so there is nothing to remind anybody \
                 about"
                    .to_owned(),
            );
        };
        let open = match records.open_records() {
            Ok(open) => open,
            Err(error) => return Outcome::Failed(format!("reading the drift records: {error}")),
        };

        // `review::digest` filters to open review records itself, and
        // `DriftKind` is `#[non_exhaustive]` — filtering by kind here would
        // silently drop a kind added later.
        let digest = review::digest(&open);
        Outcome::Done(summarise(&digest))
    })
}

/// What the job row records, which is the audit of a send rather than the
/// reminder text.
///
/// `unowned` is reported beside the owners and never folded into them. An
/// overdue page nobody owns has no reminder to send, and dropping it silently
/// would make a project with no `DOCOWNERS` produce an empty digest and read
/// as fully reviewed — absent and empty must not serialise to the same thing.
fn summarise(digest: &Digest) -> serde_json::Value {
    json!({
        "owners": digest
            .owners
            .iter()
            .map(|owner| json!({ "owner": owner.owner, "pages": owner.pages.len() }))
            .collect::<Vec<_>>(),
        "unowned": digest.unowned.iter().map(|route| route.as_str()).collect::<Vec<_>>(),
        // Nothing sends yet. Said here rather than left to be inferred from an
        // absent field, so an operator reading the job row is not told that
        // reminders went out.
        "sent": false,
        "reason": "no reminder destination is configured",
    })
}
