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
use std::time::SystemTime;

use liyasa_store::jobs::Enqueue;
use liyasa_store::records::JobRecord;
use liyasa_verify::drift::review::{self, Digest};
use serde_json::json;

use super::AppState;
use super::overdue::Flagged;
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
        // Flag first, then read. The pass is what produces the records this
        // job then digests, so reading before flagging would digest
        // yesterday's set and report today's send against it. Before this
        // existed `review::overdue` had no caller at all and the digest below
        // ran over an empty set on every instance.
        let flagged = match super::overdue::flag(state, records, SystemTime::now()) {
            Some(Ok(flagged)) => flagged,
            Some(Err(error)) => {
                return Outcome::Failed(format!("recording the overdue pages: {error}"));
            }
            // A collector serves no bundle, so there is no manifest and no
            // page to have an opinion about. Not a failure, and distinct
            // below from "nothing was overdue".
            None => {
                return Outcome::Skipped(
                    "this instance serves no bundle, so there is no manifest to read review                      dates from"
                        .to_owned(),
                );
            }
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
        Outcome::Done(summarise(&digest, &sent, &flagged))
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
    // An empty digest has its own reason. Without one the row reads
    // `"sent": 0, "reason": null`, which cannot tell an operator apart:
    //
    //   nothing was overdue        the good case
    //   nothing is being checked   what every instance did until the caller
    //                              for `review::overdue` existed
    //
    // WP-20c found this one field over from where `unowned` closed the same
    // rule, and said it was self-retiring: once a caller exists, this reason
    // firing means genuinely nothing is overdue. It has retired, so the
    // wording no longer disclaims the absent caller — `summarise` carries
    // `examined` and `flagged` instead, which say how many pages the pass
    // looked at and what it recorded, so an empty digest is readable as a
    // result rather than taken on trust.
    //
    // Still not "no owner" alone: a page can be overdue and unowned, which is
    // every page today because nothing reads `DOCOWNERS`. That is why the
    // reason names the distinction instead of claiming the site is reviewed.
    if digest.owners.is_empty() {
        return Sent {
            reason: Some(
                "no owner had an overdue page; any overdue page nobody owns is in `unowned`, \
                 and `examined` says how many pages the pass read",
            ),
            ..Sent::default()
        };
    }
    // Mail is checked AFTER the digest, not before: with nothing overdue
    // there was nothing to send, so "no mail block" would be a true statement
    // about an irrelevant thing. The configuration only matters once there is
    // a reminder it would have carried.
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
fn summarise(digest: &Digest, sent: &Sent, flagged: &Flagged) -> serde_json::Value {
    json!({
        // The pass, not the send. `examined` is the denominator for every
        // count here: without it `{"owners": [], "unowned": []}` cannot be
        // told apart from a pass that read no pages at all, which is what
        // every run of this job did before the caller existed.
        "examined": flagged.examined,
        "flagged": {
            "created": flagged.report.created,
            "updated": flagged.report.updated,
            // `verify.drift.autoResolve` is false by default, so a page that
            // came back inside its cadence gets `gone_since` set and waits for
            // an owner's approval rather than closing itself. A zero here with
            // a non-zero `examined` is that, not a pass that looked at nothing.
            "resolved": flagged.report.resolved,
        },
        // `W0639`, one per page whose `reviewed:` is not a date. Such a page is
        // also flagged, so this is not a list of pages that were skipped.
        "unreadableDates": flagged
            .problems
            .iter()
            .map(|problem| problem.message.clone())
            .collect::<Vec<_>>(),
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
