//! A link that has been failing past its grace period (VER-51).
//!
//! `core::links` checks the link and answers how long it has been failing;
//! `FailingSince::is_drift` is the grace comparison. What it deliberately does
//! not do is decide that the failure is a record — the note at the top of that
//! module says so. This is that decision.
//!
//! A build-time failure is `W0404` and stays `W0404`: a link that broke this
//! morning is a warning on the build, not a task for an author. Only one that
//! has outlasted `verify.links.grace` becomes drift, which is what stops a
//! flaky host filling the list.

use std::collections::BTreeSet;
use std::time::SystemTime;

use liyasa_core::ids::Route;

use crate::core::config::LinksConfig;
use crate::core::links::{FailingSince, LinkOutcome, LinkStatus};

use super::record::{Candidate, DriftKey, DriftKind};

/// One checked link, how long it has been failing, and the pages that carry it.
///
/// `since` is the store's: `core::links` computes an age and keeps nothing
/// between runs, so the caller that has the previous sweep supplies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailingLink {
    pub status: LinkStatus,
    pub since: FailingSince,
    pub pages: Vec<Route>,
}

/// One candidate per link that is broken and out of grace.
pub fn candidates(links: &[FailingLink], config: &LinksConfig, now: SystemTime) -> Vec<Candidate> {
    let grace = config.grace.as_duration();
    links
        .iter()
        .filter(|link| link.status.outcome.is_broken() && link.since.is_drift(grace, now))
        .map(|link| {
            Candidate::new(
                DriftKind::Link {
                    url: link.status.url.clone(),
                    reason: reason(&link.status.outcome),
                    failing_since: link.since.0,
                },
                link.pages.clone(),
            )
        })
        .collect()
}

/// Every link the sweep requested, working or not, which is what lets a link
/// that came back close its record (RFC 2060).
pub fn coverage(links: &[FailingLink]) -> BTreeSet<DriftKey> {
    links
        .iter()
        .map(|link| DriftKey::Link(link.status.url.clone()))
        .collect()
}

fn reason(outcome: &LinkOutcome) -> String {
    match outcome {
        LinkOutcome::Broken {
            status: Some(status),
            reason,
        } => format!("{status} {reason}"),
        LinkOutcome::Broken {
            status: None,
            reason,
        } => reason.clone(),
        // `candidates` filters to broken, so this is unreachable through the
        // public path and is not worth a panic on the way to a record.
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests;
