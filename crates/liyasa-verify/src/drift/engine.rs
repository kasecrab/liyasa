//! Candidates in, records out.
//!
//! The engine does four things and none of them is an observation: it drops the
//! candidates whose check class is off, grades the rest against
//! `verify.drift.severityThreshold`, reconciles them with the records already
//! stored, and closes the ones whose condition no longer holds in a run that
//! covered them.
//!
//! That last one is the whole reason [`Coverage`] exists. A scoped run —
//! `verify --changed`, a link sweep, one refreshed source — is silent about
//! every subject it did not look at, and reading that silence as "fixed" would
//! resolve the site's drift the first time somebody edited one page
//! (RFC 2060).

use std::collections::BTreeSet;
use std::time::SystemTime;

use liyasa_core::document::{Edge, EdgeOrigin};
use liyasa_core::ids::Route;
use liyasa_core::verify::{DriftReport, StoreError};
use serde_json::{Map, Value};

use crate::core::config::{DriftConfig, DriftSeverity};
use crate::core::policy::{CheckClass, Policy, PolicyLevel};
use crate::graph::MemoryGraph;
use crate::sources::routes_of;

use super::policy::DriftPolicy;
use super::record::{Candidate, DriftKey, DriftKind, DriftRecord, DriftState, Resolution};
use super::store::RecordStore;

/// What a run examined, which is what its silence about a subject means.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Coverage {
    /// Nothing beyond the candidates themselves is known to have been looked
    /// at, so no record is closed. The default, because it is the only one that
    /// is true of every caller.
    #[default]
    Observed,
    /// Exactly these subjects were examined. An open record whose key is here
    /// and which produced no candidate no longer holds.
    Subjects(BTreeSet<DriftKey>),
    /// Every subject of every kind was examined — a full sweep, not a full
    /// build of one kind.
    Everything,
}

impl Coverage {
    fn covers(&self, key: &DriftKey) -> bool {
        match self {
            Self::Observed => false,
            Self::Subjects(keys) => keys.contains(key),
            Self::Everything => true,
        }
    }
}

/// How an origin in the graph maps to the route a reader sees.
///
/// An `Impact` names blocks and VER-23's record lists pages, and only the graph
/// knows the pairing (RFC 2001). This is the seam rather than a `MemoryGraph`
/// field so a caller with the pairing from somewhere else — a build report, a
/// test — can hand it over without a graph.
pub trait Routes: Send + Sync {
    fn routes(&self, blocks: &[(EdgeOrigin, Vec<Edge>)]) -> Result<Vec<Route>, StoreError>;
}

pub struct GraphRoutes<'g>(pub &'g MemoryGraph);

impl Routes for GraphRoutes<'_> {
    fn routes(&self, blocks: &[(EdgeOrigin, Vec<Edge>)]) -> Result<Vec<Route>, StoreError> {
        routes_of(self.0, blocks)
    }
}

pub struct Engine<'a> {
    records: &'a dyn RecordStore,
    config: &'a DriftConfig,
    routes: &'a dyn Routes,
    coverage: Coverage,
    now: SystemTime,
}

impl<'a> Engine<'a> {
    pub fn new(
        records: &'a dyn RecordStore,
        config: &'a DriftConfig,
        routes: &'a dyn Routes,
    ) -> Self {
        Self {
            records,
            config,
            routes,
            coverage: Coverage::Observed,
            now: SystemTime::now(),
        }
    }

    #[must_use]
    pub fn at(mut self, now: SystemTime) -> Self {
        self.now = now;
        self
    }

    #[must_use]
    pub fn covering(mut self, coverage: Coverage) -> Self {
        self.coverage = coverage;
        self
    }

    pub fn routes_of(&self) -> &dyn Routes {
        self.routes
    }

    /// The flow: grade, reconcile, close.
    pub fn record(
        &self,
        candidates: &[Candidate],
        policy: &Policy,
    ) -> Result<DriftReport, StoreError> {
        let drift = DriftPolicy::new(self.config);
        let mut report = DriftReport::default();
        let mut seen = BTreeSet::new();

        for candidate in candidates {
            if !recordable(candidate, policy) {
                continue;
            }
            let key = candidate.key();
            seen.insert(key.clone());
            match self.records.find(&key)? {
                Some(stored) => {
                    if let Some((next, reopened)) =
                        self.updated(stored, candidate, drift.grade(candidate))
                    {
                        self.records.save(&next)?;
                        let counter = if reopened {
                            &mut report.created
                        } else {
                            &mut report.updated
                        };
                        *counter = counter.saturating_add(1);
                    }
                }
                None => {
                    let Some(severity) = drift.grade(candidate) else {
                        continue;
                    };
                    self.records
                        .save(&candidate.clone().opened(severity, self.now))?;
                    report.created = report.created.saturating_add(1);
                }
            }
        }

        report.resolved = self.close_absent(&seen, drift.auto_resolves())?;
        Ok(report)
    }

    /// An open record whose condition no longer holds, in a run that looked.
    fn close_absent(&self, seen: &BTreeSet<DriftKey>, auto: bool) -> Result<u32, StoreError> {
        let mut closed: u32 = 0;
        for mut record in self.records.open_records()? {
            let key = record.key();
            if seen.contains(&key) || !self.coverage.covers(&key) {
                continue;
            }
            if record.gone_since.is_none() {
                record.gone_since = Some(self.now);
            }
            if auto {
                record.state = DriftState::Resolved;
                record.resolved_at = Some(self.now);
                record.resolution = Some(Resolution::Fixed);
                closed = closed.saturating_add(1);
            }
            self.records.save(&record)?;
        }
        Ok(closed)
    }

    /// The record a re-observed subject becomes, or `None` when a reader would
    /// see no difference.
    ///
    /// `severity` is `None` when the candidate is below the threshold. That does
    /// not close the record — the condition still holds — but it does not
    /// change its grade either, so raising the threshold never silently
    /// downgrades what is already open.
    fn updated(
        &self,
        stored: DriftRecord,
        candidate: &Candidate,
        severity: Option<DriftSeverity>,
    ) -> Option<(DriftRecord, bool)> {
        let reopening = stored.state == DriftState::Resolved;
        let next = DriftRecord {
            kind: candidate.kind.clone(),
            severity: severity.unwrap_or(stored.severity),
            state: DriftState::Open,
            pages: candidate.pages.clone(),
            blocks: candidate.blocks.clone(),
            // A subject that drifted, was fixed, and drifted again is new
            // drift, and `created` is the number that has to say so.
            first_seen: if reopening {
                self.now
            } else {
                stored.first_seen
            },
            last_seen: self.now,
            gone_since: None,
            resolved_at: None,
            resolution: None,
            weight: candidate.weight.or(stored.weight),
        };
        let same = next.kind == stored.kind
            && next.severity == stored.severity
            && next.pages == stored.pages
            && next.blocks == stored.blocks
            && next.weight == stored.weight
            && !reopening
            && stored.gone_since.is_none();
        (!same).then_some((next, reopening))
    }

    /// Close a record because a person said so: VER-77's approval in the
    /// dashboard, or the maintenance agent reporting VER-73's task done. `false`
    /// when there was no open record to close.
    pub fn resolve(&self, key: &DriftKey, resolution: Resolution) -> Result<bool, StoreError> {
        let Some(mut record) = self.records.find(key)?.filter(DriftRecord::is_open) else {
            return Ok(false);
        };
        record.state = DriftState::Resolved;
        record.resolved_at = Some(self.now);
        record.resolution = Some(resolution);
        self.records.save(&record)?;
        Ok(true)
    }
}

/// Whether the check class this kind belongs to is on.
///
/// A class set to `off` reports nothing, and a drift record is a report
/// (RFC 2061). A review has no class, so nothing turns it off but
/// `content.reviewCadence`.
fn recordable(candidate: &Candidate, policy: &Policy) -> bool {
    // A subject that reaches no page is a real answer and not a record: there
    // is nothing to fix and nothing to show (RFC 2060).
    if candidate.pages.is_empty() {
        return false;
    }
    class_of(&candidate.kind).is_none_or(|class| policy.level(class) != PolicyLevel::Off)
}

pub fn class_of(kind: &DriftKind) -> Option<CheckClass> {
    match kind {
        // A spec is a truth source, so an operation that moved is a fact that
        // moved as far as `verify.policy` is concerned.
        DriftKind::Fact { .. } | DriftKind::Operation { .. } => Some(CheckClass::Facts),
        DriftKind::Link { .. } => Some(CheckClass::Links),
        DriftKind::Check { .. } => Some(CheckClass::Code),
        DriftKind::Review { .. } => None,
    }
}

/// The per-class levels out of the frozen policy value (RFC 2061).
///
/// `fail_on` is not a class, so it is left out rather than read and reported as
/// a bad key: the object was already validated when config was read, and
/// `apply` has nowhere to put a diagnostic.
pub fn policy_of(policy: &liyasa_core::verify::VerifyPolicy) -> Policy {
    let object: Map<String, Value> = policy
        .rest
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let (parsed, _) = Policy::from_value(&Value::Object(object));
    parsed
}

#[cfg(test)]
mod tests;
