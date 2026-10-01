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
        let sent = send(state, &digest).await;
        Outcome::Done(summarise(&digest, &sent))
    })
}

/// What happened to each owner's reminder.
#[derive(Debug, Default)]
struct Sent {
    delivered: usize,
    failed: Vec<String>,
    reason: Option<&'static str>,
}

/// Sends one message per owner.
///
/// `Mail::send` rather than `send_link`, and the difference is not stylistic.
/// `send_link` is never awaited and reports nothing, because AUTH-09 needs the
/// magic-link endpoint to answer identically whether or not the address is
/// known — awaiting only when a link was minted is a timing oracle for exactly
/// what the identical response hides. A reminder is under no such constraint:
/// the address came from `DOCOWNERS`, and an operator wants to know whether it
/// arrived. `auth/state.rs` says "do not tidy these into one shape".
async fn send(state: &Arc<AppState>, digest: &Digest) -> Sent {
    let Some(mail) = state.mail() else {
        return Sent {
            reason: Some("no `mail` block is configured, so no reminder was sent"),
            ..Sent::default()
        };
    };
    let mut sent = Sent::default();
    for owner in &digest.owners {
        let body = body_for(owner);
        match mail.send(&owner.owner, "Pages due for review", &body).await {
            Ok(()) => sent.delivered += 1,
            // One owner's bad address does not stop the rest, and the failure
            // is named rather than counted: "three failed" tells an operator
            // nothing they can act on.
            Err(error) => sent.failed.push(format!("{}: {error}", owner.owner)),
        }
    }
    sent
}

/// One owner's reminder, worst-first as `review::digest` ordered it.
fn body_for(owner: &review::OwnerDigest) -> String {
    let mut body = String::from("These pages are past their review cadence:\n\n");
    for (route, weight) in &owner.pages {
        match weight {
            Some(weight) => {
                body.push_str(&format!("  {}  (traffic {weight:.0})\n", route.as_str()))
            }
            // No traffic figure sorts last rather than first, so this is the
            // tail of the list and says so rather than showing a zero it does
            // not know.
            None => body.push_str(&format!("  {}  (no traffic data)\n", route.as_str())),
        }
    }
    body
}

/// What the job row records, which is the audit of a send rather than the
/// reminder text.
///
/// `unowned` is reported beside the owners and never folded into them. An
/// overdue page nobody owns has no reminder to send, and dropping it silently
/// would make a project with no `DOCOWNERS` produce an empty digest and read
/// as fully reviewed — absent and empty must not serialise to the same thing.
fn summarise(digest: &Digest, sent: &Sent) -> serde_json::Value {
    json!({
        "owners": digest
            .owners
            .iter()
            .map(|owner| json!({ "owner": owner.owner, "pages": owner.pages.len() }))
            .collect::<Vec<_>>(),
        "unowned": digest.unowned.iter().map(|route| route.as_str()).collect::<Vec<_>>(),
        "sent": sent.delivered,
        // Named rather than counted: a count tells an operator nothing they
        // can act on, and an empty list is not the same as no attempt.
        "failed": sent.failed,
        "reason": sent.reason,
    })
}
