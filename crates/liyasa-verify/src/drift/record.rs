//! What a drift record is.
//!
//! One record is one subject that has moved away from what the docs say about
//! it: a fact whose value changed (VER-23), an API operation whose shape
//! changed (VER-12), a link that has been failing past its grace period
//! (VER-51), a verified example that stopped passing (VER-01), or a page that
//! is past its review cadence (VER-77). The kinds differ in what they carry
//! and in nothing else, so they share one record, one severity scale, and one
//! resolution flow.
//!
//! A record's identity is its [`DriftKey`], not a minted ID. The key is
//! derived from the subject, so the same subject drifting again on the next run
//! updates the record it already has rather than opening a second one
//! (RFC 2061).

use std::time::{Duration, SystemTime};

use liyasa_core::document::{Edge, EdgeOrigin};
use liyasa_core::ids::{CheckId, FactId, Route};
use liyasa_core::verify::{ChangeKind, FactValue};

use crate::core::config::DriftSeverity;

/// The identity of a record across runs.
///
/// A fact or an operation is one subject however many pages it reaches, so its
/// key does not name a page and the pages travel in
/// [`DriftRecord::pages`]. A review is a property of one page, so its key is
/// the route.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DriftKey {
    Fact(FactId),
    Operation { spec: String, op: String },
    Link(String),
    Check(CheckId),
    Review(Route),
}

/// What drifted, with everything specific to that kind of drift.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DriftKind {
    /// VER-23. `old` and `new` are the values the record has to show.
    Fact {
        fact: FactId,
        old: Option<FactValue>,
        new: Option<FactValue>,
        change: ChangeKind,
    },
    /// VER-12. `diff` is the facet list `operation_changes` produced.
    Operation {
        spec: String,
        op: String,
        diff: Vec<String>,
    },
    /// VER-51. `failing_since` is what the grace period was measured against.
    Link {
        url: String,
        reason: String,
        failing_since: SystemTime,
    },
    /// A check that passed before and does not now.
    Check { check: CheckId, excerpt: String },
    /// VER-77. `reviewed` is absent when the page has never recorded one.
    ///
    /// The route is in the variant because a review is a property of one page
    /// and the key has to be derivable from the kind alone.
    Review {
        page: Route,
        owner: String,
        reviewed: Option<SystemTime>,
        cadence: Duration,
        overdue_by: Duration,
    },
}

impl DriftKind {
    pub fn key(&self) -> DriftKey {
        match self {
            Self::Fact { fact, .. } => DriftKey::Fact(fact.clone()),
            Self::Operation { spec, op, .. } => DriftKey::Operation {
                spec: spec.clone(),
                op: op.clone(),
            },
            Self::Link { url, .. } => DriftKey::Link(url.clone()),
            Self::Check { check, .. } => DriftKey::Check(check.clone()),
            Self::Review { page, .. } => DriftKey::Review(page.clone()),
        }
    }

    /// The severity before the blast radius is folded in (RFC 2062).
    pub fn base_severity(&self) -> DriftSeverity {
        match self {
            Self::Fact { change, .. } => match change {
                ChangeKind::Removed => DriftSeverity::High,
                ChangeKind::Changed => DriftSeverity::Medium,
                ChangeKind::Added => DriftSeverity::Low,
            },
            Self::Operation { diff, .. } => operation_severity(diff),
            Self::Link { .. } => DriftSeverity::Medium,
            Self::Check { .. } => DriftSeverity::High,
            Self::Review {
                cadence,
                overdue_by,
                ..
            } => {
                if *overdue_by >= *cadence {
                    DriftSeverity::High
                } else {
                    DriftSeverity::Medium
                }
            }
        }
    }
}

/// An operation is graded by the worst facet that moved, because a page showing
/// a call is wrong if any one of them is wrong.
fn operation_severity(diff: &[String]) -> DriftSeverity {
    diff.iter()
        .map(|facet| match facet.as_str() {
            "removed" => DriftSeverity::Critical,
            "auth" | "parameters" => DriftSeverity::High,
            "responses" => DriftSeverity::Medium,
            // "added", and any facet a later spec differ learns to report.
            _ => DriftSeverity::Low,
        })
        .max()
        .unwrap_or(DriftSeverity::Low)
}

/// One level up, saturating at `Critical`.
pub fn escalated(severity: DriftSeverity) -> DriftSeverity {
    match severity {
        DriftSeverity::Low => DriftSeverity::Medium,
        DriftSeverity::Medium => DriftSeverity::High,
        DriftSeverity::High | DriftSeverity::Critical => DriftSeverity::Critical,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DriftState {
    #[default]
    Open,
    Resolved,
}

/// Why a record closed. VER-73 hands open records to the maintenance agent as
/// tasks, so which of these closed a record is the difference between "the
/// docs were fixed" and "someone said it was fine".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// The condition no longer holds in a run that covered it.
    Fixed,
    /// A person closed it: VER-77's approval in the dashboard, or a task the
    /// maintenance agent reported done.
    Approved { by: String },
    /// The subject is gone — the page was deleted, the fact undeclared, the
    /// link removed from every page that carried it.
    Vanished,
}

/// A subject that has drifted, the pages it affects, and the evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct DriftRecord {
    pub kind: DriftKind,
    pub severity: DriftSeverity,
    pub state: DriftState,
    /// VER-23's "affected pages", sorted and deduplicated.
    pub pages: Vec<Route>,
    /// VER-23's "affected blocks", each with the edge path that proves it.
    pub blocks: Vec<(EdgeOrigin, Vec<Edge>)>,
    pub first_seen: SystemTime,
    pub last_seen: SystemTime,
    pub resolved_at: Option<SystemTime>,
    pub resolution: Option<Resolution>,
    /// VER-77's traffic weight, and `None` for a kind or a deployment with no
    /// analytics behind it. Ordering a digest by it is `review`'s job; the
    /// record only carries it.
    pub weight: Option<f64>,
}

impl DriftRecord {
    pub fn key(&self) -> DriftKey {
        self.kind.key()
    }

    pub fn is_open(&self) -> bool {
        self.state == DriftState::Open
    }
}

#[cfg(test)]
mod tests;
