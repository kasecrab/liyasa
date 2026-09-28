//! Records as the verify report shows them.
//!
//! `report::DriftEntry` was written with nothing producing one — its doc comment
//! says the engine that creates and resolves them is this package's. This is the
//! conversion, and it is here rather than in `report/` so that `report` keeps
//! knowing nothing about how a record is made.
//!
//! One entry per affected page, because `PageReport::drift` is per page and a
//! record that reaches four pages is work on four pages.

use std::time::{Duration, SystemTime};

use liyasa_core::ids::Route;

use crate::report::DriftEntry;

use super::facts::show;
use super::record::{DriftKind, DriftRecord};

/// Every open record, paired with the page it belongs under.
///
/// Resolved records are left out: the report shows what is open. `age` is
/// measured from `first_seen`, which for a record that drifted again after a
/// close is the current episode rather than the first one ever (RFC 2063).
pub fn entries(records: &[DriftRecord], now: SystemTime) -> Vec<(Route, DriftEntry)> {
    let mut out: Vec<(Route, DriftEntry)> = records
        .iter()
        .filter(|record| record.is_open())
        .flat_map(|record| {
            let entry = DriftEntry {
                id: record.key().job_id().to_string(),
                summary: summary(&record.kind),
                severity: record.severity,
                age: now
                    .duration_since(record.first_seen)
                    .unwrap_or(Duration::ZERO),
            };
            record
                .pages
                .iter()
                .cloned()
                .map(move |page| (page, entry.clone()))
        })
        .collect();
    out.sort_by(|(left, a), (right, b)| {
        left.cmp(right)
            .then_with(|| b.severity.cmp(&a.severity))
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// One line, in the terms the person who has to fix it thinks in.
pub fn summary(kind: &DriftKind) -> String {
    match kind {
        DriftKind::Fact { fact, old, new, .. } => format!(
            "`{fact}` is {} here and {} at its source",
            show(old.as_ref()),
            show(new.as_ref())
        ),
        DriftKind::Operation { spec, op, diff } => {
            format!("`{op}` in `{spec}` moved: {}", diff.join(", "))
        }
        DriftKind::Link { url, reason, .. } => format!("{url} has been failing: {reason}"),
        DriftKind::Check { excerpt, .. } => format!("a verified example fails: {excerpt}"),
        DriftKind::Review {
            reviewed,
            cadence,
            overdue_by,
            ..
        } => match reviewed {
            None => format!("never reviewed; the cadence is {}", days(*cadence)),
            Some(_) => format!(
                "review is {} overdue; the cadence is {}",
                days(*overdue_by),
                days(*cadence)
            ),
        },
    }
}

/// Whole days, because a review cadence is not a stopwatch and "179 days"
/// reads better than a duration.
fn days(duration: Duration) -> String {
    match duration.as_secs() / 86_400 {
        1 => "1 day".to_owned(),
        other => format!("{other} days"),
    }
}

#[cfg(test)]
mod tests;
