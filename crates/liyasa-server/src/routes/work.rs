//! Who runs a job (RFC 1404).
//!
//! The job store has leases, heartbeats, per-key de-duplication and a backoff
//! ladder, and until this module nothing claimed a row. A package contributes
//! a name, a handler and how the job starts; the claim-heartbeat-complete loop
//! lives here once, because the lease is the part that must not be
//! reimplemented per package.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use liyasa_core::net::BoxFut;
use liyasa_core::store::StoreError;
use liyasa_store::Enqueue;
use liyasa_store::records::JobRecord;
use serde_json::{Value, json};

use super::AppState;
use crate::{assistant, deploy};
// The sweep's three items by name, not by path: with `assistant::` on each,
// the registration is 101 columns and `max_width` is 100, so rustfmt would put
// back the multi-line shape a union merge cannot survive. A name collision
// with another package's import here is a compile error, which is loud; an
// interleaved literal is not (defect 192). WP-18 can replace these three lines
// with one `pub const` in its own module whenever it next touches that file.
use crate::assistant::{RETENTION_JOB, retention_due, run_sweep};

/// One claimed job's run. The worker owns the lease around it.
pub type Run<'a> = BoxFut<'a, Outcome>;

/// What a handler did.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Done(Value),
    /// Retried on the backoff ladder while attempts remain.
    Failed(String),
    /// Not this instance's to run, and not an error. Completed rather than
    /// retried: a job whose precondition is a missing configuration would
    /// otherwise burn the whole ladder to reach `dead` and tell the operator
    /// nothing.
    Skipped(String),
}

/// What starts a job.
pub enum Trigger {
    /// Enqueued by whoever does the thing; the worker only runs it.
    Caller,
    /// After a deployment succeeds. `None` means this deployment needs none.
    DeploymentSucceeded(fn(&Arc<AppState>, &Value) -> Option<Enqueue>),
    /// On a schedule the builder owns. Called on every tick; it returns the
    /// row to enqueue when the job is due and `None` when it is not.
    ///
    /// There is deliberately no interval here. The de-duplication key does
    /// the work — a key that is the schedule's current bucket means every
    /// replica may fire and exactly one row exists — so an interval field
    /// would be read by nothing, and a schedule that lives in configuration
    /// (`verify.links.schedule` is a `DurationSetting`, VER-51) could not be
    /// expressed as a constant anyway.
    Scheduled(fn(&Arc<AppState>) -> Option<Enqueue>),
}

/// What runs a claimed row.
pub type RunFn = for<'a> fn(&'a Arc<AppState>, &'a JobRecord) -> Run<'a>;
/// Whether a scheduled job is due this tick, and the row to enqueue if it is.
pub type DueFn = fn(&Arc<AppState>) -> Option<Enqueue>;
/// The same question for a deployment that has just succeeded.
pub type AfterDeployFn = fn(&Arc<AppState>, &Value) -> Option<Enqueue>;

pub struct JobKind {
    pub name: &'static str,
    pub trigger: Trigger,
    pub run: RunFn,
}

/// Constructors, because a registration in this file must fit on one line.
///
/// `kinds()` is `merge=union`: two packages appending in the same window merge
/// by line, and a multi-line struct literal interleaves into something
/// `cargo fmt` cannot parse. That happened on 2026-09-28 — one swallowed `},`
/// left the file with 55 `{` against 54 `}`, and every branch chained under it
/// died at the gate's FORMAT step in two seconds, before any of its own code
/// compiled. Seven packages looked red for one missing brace (defect 192).
///
/// "Write one line" is not by itself achievable: a three-field literal is 116
/// columns and `rustfmt.toml` sets `max_width = 100`, so rustfmt puts back the
/// shape the rule forbids. What works is writing something one line can point
/// at — either one of these constructors, or better, a `pub const JobKind` in
/// your own module, which this list then names in a line short enough to
/// survive:
///
/// ```text
/// // crates/liyasa-server/src/deploy/jobs.rs — yours, cannot be interleaved
/// pub const BUILD: JobKind = JobKind::caller(BUILD_JOB, run_build);
///
/// // here
/// crate::deploy::jobs::BUILD,
/// ```
///
/// The const is better than a short constructor call and not merely shorter:
/// the reasoning for a registration travels with it. A comment beside a
/// literal in a union-merged file can be separated from what it describes by
/// somebody else's append, and a stranded comment is not a compile error —
/// which makes it worse than one, not better.
impl JobKind {
    /// Enqueued by whoever does the thing; the worker only runs it.
    pub const fn caller(name: &'static str, run: RunFn) -> Self {
        Self {
            name,
            trigger: Trigger::Caller,
            run,
        }
    }

    /// Enqueued by the worker's own timer when `due` says so.
    pub const fn scheduled(name: &'static str, due: DueFn, run: RunFn) -> Self {
        Self {
            name,
            trigger: Trigger::Scheduled(due),
            run,
        }
    }

    /// Enqueued after a deployment succeeds, when `after` says so.
    pub const fn after_deploy(name: &'static str, after: AfterDeployFn, run: RunFn) -> Self {
        Self {
            name,
            trigger: Trigger::DeploymentSucceeded(after),
            run,
        }
    }
}

impl std::fmt::Debug for JobKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobKind").field("name", &self.name).finish()
    }
}

/// Every job kind, in no particular order. One line per package (RFC 1404).
///
/// **No to-do list lives here, deliberately.** This file is append-only and
/// `merge=union`, so the package that satisfies a "still owed by WP-NN" note
/// is the one package forbidden to delete it — the note outlives what it
/// describes and the fleet reads a stale list as current (defect 203). An
/// earlier version of this comment named two jobs as owed and was wrong within
/// the day.
///
/// The absence is already reported where it belongs: [`report`] logs at warn
/// when nothing is registered, and `/_liyasa/ready` names the kinds nobody can
/// run. Those tell an operator; the defect ledger tells the fleet. Neither
/// goes stale in a file nobody may rewrite.
///
/// Nor is it a pinned list in a test, and the next person here will want to
/// make one. Do not. A pin makes this package's test assert a claim about
/// another package's naming, so it fails when WP-16 registers `build.run`
/// instead — reddening their branch for being right. **A test is the wrong
/// place for a claim about work that has not happened**, and — see
/// `ast_01_embed_job` — counting across this whole list is the wrong place for
/// a claim about work that has.
///
/// Append your entry; never rewrite the list. This file is on path-guard's
/// shared list and is `merge=union` in `.gitattributes`, which is what makes
/// one-line-per-package work when two packages register in the same window.
/// Before you register a name, grep for it — a name used as an orphan fixture
/// stops being orphaned the moment you claim it:
///
/// ```text
/// git grep -n 'your.job.name' -- tests/ crates/*/tests/
/// ```
// WP-18's two, as consts because `&[...]` is only promoted to `'static` when
// every element is one — a `const fn` call in the list is a temporary and does
// not promote. One line each, which is what union can merge. They belong in
// `assistant/`, which is WP-18's path; moving them is a cut and paste.
const ASSISTANT_EMBED: JobKind = JobKind::caller(deploy::queue::EMBED_JOB, assistant::run_index);
const ASSISTANT_RETENTION: JobKind = JobKind::scheduled(RETENTION_JOB, retention_due, run_sweep);

pub fn kinds() -> &'static [JobKind] {
    &[
        // WP-18. The name is `deploy::queue::EMBED_JOB` rather than a literal
        // or `liyasa_ai`'s copy: `queue_embedding` is what actually enqueues
        // this row, so the enqueuer owns the spelling. `Trigger::Caller` for
        // the same reason — a `DeploymentSucceeded` trigger would enqueue a
        // second row (RFC 1404, corrected).
        JobKind {
            name: crate::deploy::queue::EMBED_JOB,
            trigger: Trigger::Caller,
            run: crate::assistant::run_index,
        },
        JobKind {
            name: crate::assistant::RETENTION_JOB,
            trigger: Trigger::Scheduled(crate::assistant::retention_due),
            run: crate::assistant::run_sweep,
        },
        // WP-16, one self-contained line each; rationale in `crate::deploy::jobs`.
        crate::deploy::jobs::BUILD,
        crate::deploy::jobs::RETENTION,
        // WP-17. The names are `liyasa_analytics::actions`' constants, which
        // is also what enqueues them, so the registry and the enqueuer cannot
        // spell the same job two ways.
        JobKind {
            name: super::analytics::RETENTION_JOB,
            trigger: Trigger::Scheduled(super::analytics::retention_due),
            run: super::analytics::run_retention,
        },
        JobKind {
            name: super::analytics::DIGEST_JOB,
            trigger: Trigger::Scheduled(super::analytics::digest_due),
            run: super::analytics::run_digest,
        },
        ASSISTANT_EMBED,
        ASSISTANT_RETENTION,
        super::analytics::RETENTION,
        super::analytics::DIGEST,
    ]
}

/// Names claimed with no handler, logged once per process rather than once per
/// claim: a binary older than whoever enqueued would otherwise say it on every
/// pass.
static UNKNOWN: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

fn note_unknown(name: &str) {
    let mut seen = UNKNOWN.lock().unwrap_or_else(|e| e.into_inner());
    if seen.insert(name.to_owned()) {
        tracing::info!(
            target: "liyasa_server",
            job = name,
            "no handler is registered for this job; leaving it for a binary that has one"
        );
    }
}

/// Claims and runs at most `limit` jobs. Returns how many ran, so a test can
/// drive the worker a pass at a time rather than waiting on a timer.
pub async fn run_once(
    state: &Arc<AppState>,
    kinds: &[JobKind],
    worker: &str,
) -> Result<usize, StoreError> {
    run_up_to(state, kinds, worker, 16).await
}

pub async fn run_up_to(
    state: &Arc<AppState>,
    kinds: &[JobKind],
    worker: &str,
    limit: usize,
) -> Result<usize, StoreError> {
    let Some(store) = state.store.clone() else {
        // Nothing to claim from. Not an error: a collector has no store and
        // no jobs (ANA-09).
        return Ok(0);
    };
    let jobs = store.jobs_typed();
    let lease = state.config.jobs_lease;
    let mut ran = 0;

    while ran < limit {
        let Some(job) = jobs.claim(worker, lease).await? else {
            break;
        };
        let Some(kind) = kinds.iter().find(|k| k.name == job.name) else {
            note_unknown(&job.name);
            // Release rather than fail: the package that owns it may be
            // merging right now, and failing it through its attempts because
            // this binary is older would be a self-inflicted outage.
            //
            // Scoped to this job. `release` is by worker and would re-queue
            // everything else this worker holds, which is harmless only while
            // exactly one job is held at a time — a property of this loop
            // that a later change could remove without noticing (WP-16).
            jobs.release_one(&job.id).await?;
            // Nothing else is claimable that this binary can run: `claim`
            // orders by priority and would hand back the same row.
            break;
        };

        // The lease is renewed while the handler runs, so a job that takes
        // longer than one lease is not stolen from underneath it.
        let heartbeat = {
            let store = store.clone();
            let id = job.id;
            let every = lease / 3;
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(every).await;
                    if store.jobs_typed().heartbeat(&id).await.is_err() {
                        return;
                    }
                }
            })
        };
        let outcome = (kind.run)(state, &job).await;
        heartbeat.abort();

        match outcome {
            Outcome::Done(result) => jobs.complete(&job.id, result).await?,
            Outcome::Skipped(reason) => {
                tracing::info!(
                    target: "liyasa_server",
                    job = %job.name,
                    reason = %reason,
                    "job skipped"
                );
                jobs.complete(&job.id, json!({ "skipped": reason })).await?;
            }
            Outcome::Failed(error) => {
                tracing::warn!(
                    target: "liyasa_server",
                    job = %job.name,
                    error = %error,
                    "job failed"
                );
                jobs.fail(&job.id, &error).await?;
            }
        }
        ran += 1;
    }
    Ok(ran)
}

/// Enqueues every timer job that is due. At most one row exists per bucket
/// however many replicas fire: `(name, key)` is uniquely indexed over live
/// rows, so the guarantee is the store's rather than the worker's.
pub async fn fire_timers(state: &Arc<AppState>, kinds: &[JobKind]) -> Result<usize, StoreError> {
    let Some(store) = state.store.clone() else {
        return Ok(0);
    };
    let mut queued = 0;
    for kind in kinds {
        let Trigger::Scheduled(build) = &kind.trigger else {
            continue;
        };
        if let Some(enqueue) = build(state) {
            // Rows added, not calls made. Every replica's timer fires for the
            // same bucket and the store's unique index collapses them to one
            // row, so counting the call would report three ticks as three
            // pieces of work when one exists.
            if matches!(
                store.jobs_typed().enqueue(&enqueue).await?,
                liyasa_store::jobs::Enqueued::Queued(_)
            ) {
                queued += 1;
            }
        }
    }
    Ok(queued)
}

/// Enqueues what packages registered for a successful deployment.
pub async fn on_deployment(
    state: &Arc<AppState>,
    kinds: &[JobKind],
    event: &Value,
) -> Result<usize, StoreError> {
    let Some(store) = state.store.clone() else {
        return Ok(0);
    };
    let mut queued = 0;
    for kind in kinds {
        let Trigger::DeploymentSucceeded(build) = &kind.trigger else {
            continue;
        };
        if let Some(enqueue) = build(state, event) {
            // Rows added, not calls made: the same deployment announced twice
            // is one piece of work, because the build is the key.
            if matches!(
                store.jobs_typed().enqueue(&enqueue).await?,
                liyasa_store::jobs::Enqueued::Queued(_)
            ) {
                queued += 1;
            }
        }
    }
    Ok(queued)
}

/// What is registered and what is waiting with nobody to run it.
///
/// A registry that starts cleanly and says nothing is the same defect it
/// exists to fix: a complete mechanism with no caller. This is what makes the
/// absence visible, at startup and at `/_liyasa/ready`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registered {
    pub handled: Vec<String>,
    /// Job names in the queue that no handler claims. Work nobody can do.
    pub orphaned: Vec<String>,
}

pub async fn registered(
    state: &Arc<AppState>,
    kinds: &[JobKind],
) -> Result<Registered, StoreError> {
    let handled: Vec<String> = kinds.iter().map(|k| k.name.to_owned()).collect();
    let orphaned = match state.store.clone() {
        None => Vec::new(),
        Some(store) => store
            .jobs_typed()
            .live_names()
            .await?
            .into_iter()
            .filter(|name| !handled.iter().any(|h| h == name))
            .collect(),
    };
    Ok(Registered { handled, orphaned })
}

/// Says what the worker can and cannot do, once, at startup.
pub fn report(registered: &Registered) {
    if registered.handled.is_empty() {
        tracing::warn!(
            target: "liyasa_server",
            "no job handlers are registered: every job any package enqueues will wait for ever"
        );
    } else {
        tracing::info!(
            target: "liyasa_server",
            handlers = registered.handled.join(", "),
            "job handlers registered"
        );
    }
    for name in &registered.orphaned {
        tracing::warn!(
            target: "liyasa_server",
            job = %name,
            "jobs are queued under this name and no handler is registered for it"
        );
    }
}

/// How often a timer trigger is checked. A job's own interval decides whether
/// it is due; this is only how often the question is asked, and it is short
/// enough that a daily job runs near its bucket boundary.
pub const TICK: Duration = Duration::from_secs(60);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registered_name_appears_once() {
        let names: Vec<&str> = kinds().iter().map(|k| k.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names.len(), sorted.len(), "{names:?}");
    }

    #[test]
    fn an_unknown_name_is_noted_once_per_process() {
        // The second call must not log again; the observable part here is
        // that it does not panic and the set holds one entry.
        note_unknown("some.package.job");
        note_unknown("some.package.job");
        let seen = UNKNOWN.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(seen.iter().filter(|n| *n == "some.package.job").count(), 1);
    }

    #[test]
    fn the_tick_is_shorter_than_the_shortest_useful_bucket() {
        assert!(TICK <= Duration::from_secs(300));
    }
}
