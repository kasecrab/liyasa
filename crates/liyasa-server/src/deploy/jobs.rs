//! This package's two entries in the job registry (RFC 1404).
//!
//! They are `const` items here, rather than struct literals written into
//! `routes/work.rs`, because that file is `merge=union`. A multi-line literal
//! appended by two packages in the same window interleaves line by line, and
//! what comes out has more `{` than `}`: that is how this branch came to be
//! syntactically invalid on 2026-09-28, killing the gate at the format step in
//! two seconds for every branch chained under it. A union merge cannot
//! interleave a single line with anything.
//!
//! The rationale that would otherwise be a comment beside each entry lives
//! here for the same reason — a comment stranded from its entry is not a
//! compile error, which is worse, not better.

use crate::routes::work::{self, JobKind};

/// `deploy.build`. `Trigger::Caller` because the deploy queue already enqueues
/// the row: a push, a pull request and a manual trigger all go through
/// `BuildRequest::to_enqueue`, so registering the handler changes nothing
/// about how the job arrives. A `DeploymentSucceeded` trigger would enqueue a
/// second one.
pub const BUILD: JobKind = JobKind {
    name: crate::deploy::queue::JOB_NAME,
    trigger: work::Trigger::Caller,
    run: crate::deploy::worker::run,
};

/// `deploy.retention`, the sweep of GIT-23. The de-duplication key is the day
/// bucket, so every replica may fire its own tick and exactly one row exists —
/// which is why `Trigger::Scheduled` carries no interval.
pub const RETENTION: JobKind = JobKind {
    name: crate::deploy::retention::JOB_NAME,
    trigger: work::Trigger::Scheduled(crate::deploy::retention::daily),
    run: crate::deploy::retention::run_sweep,
};
