//! What config decides about a candidate.
//!
//! Three keys, and every one of them is read here and nowhere else in this
//! package: `verify.drift.severityThreshold` is what a candidate has to reach
//! to become a record, `verify.drift.batchSize` is both the blast radius that
//! escalates a record and the size a big one splits into (RFC 2062, RFC 2065),
//! and `verify.drift.autoResolve` is whether a condition that stopped holding
//! closes its own record.

use liyasa_core::ids::Route;

use crate::core::config::{DriftConfig, DriftSeverity};

use super::record::{Candidate, escalated};

pub struct DriftPolicy<'a> {
    config: &'a DriftConfig,
}

impl<'a> DriftPolicy<'a> {
    pub fn new(config: &'a DriftConfig) -> Self {
        Self { config }
    }

    /// The level this candidate records at, or `None` when it is below
    /// `severityThreshold` and is therefore not a record at all.
    pub fn grade(&self, candidate: &Candidate) -> Option<DriftSeverity> {
        let base = candidate.kind.base_severity();
        let severity = if self.splits(&candidate.pages) {
            escalated(base)
        } else {
            base
        };
        (severity >= self.config.severity_threshold).then_some(severity)
    }

    /// Whether a record over this many pages is more than one unit of work
    /// (VER-73).
    pub fn splits(&self, pages: &[Route]) -> bool {
        self.batch_size().is_some_and(|size| pages.len() > size)
    }

    /// The page batches a record splits into, in route order. One batch when it
    /// does not split, so a caller opening proposals always iterates this.
    pub fn batches<'p>(&self, pages: &'p [Route]) -> Vec<&'p [Route]> {
        match self.batch_size() {
            Some(size) if pages.len() > size => pages.chunks(size).collect(),
            _ => vec![pages],
        }
    }

    pub fn auto_resolves(&self) -> bool {
        self.config.auto_resolve
    }

    /// `batchSize: 0` is "never split" rather than a panic in `chunks`.
    fn batch_size(&self) -> Option<usize> {
        usize::try_from(self.config.batch_size)
            .ok()
            .filter(|size| *size > 0)
    }
}

#[cfg(test)]
mod tests;
