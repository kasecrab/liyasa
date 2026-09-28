//! Fact drift: what a changed value does to the pages that state it (VER-23).
//!
//! The comparison itself is already built. `Refresher` re-reads a source and
//! diffs it into `FactChange`s, and `PathImpact` walks the graph into the blocks
//! each change reaches. What was missing is the two things a caller does with
//! that: the findings a verify report shows, and the records that outlive the
//! run.
//!
//! [`checks`] is the `facts` check class (CLI-06). One `CheckResult` per block
//! that renders a value that moved, with the `CheckId` the rest of the report
//! machinery already groups by.
//!
//! [`candidates`] is the same impacts as drift, one per change.

use std::time::Duration;

use liyasa_core::document::EdgeOrigin;
use liyasa_core::ids::{CheckId, Fingerprint, Route};
use liyasa_core::verify::{CheckOutcome, CheckResult, FactValue, Impact, StoreError};

use crate::core::Scrubber;
use crate::runners::check_id;

use super::engine::Routes;
use super::record::{Candidate, DriftKind};

/// One drift candidate per change, carrying the pages it reached and the
/// evidence that proves each one.
pub fn candidates(impacts: &[Impact], routes: &dyn Routes) -> Result<Vec<Candidate>, StoreError> {
    impacts
        .iter()
        .map(|impact| {
            let pages = routes.routes(&impact.blocks)?;
            Ok(Candidate::new(
                DriftKind::Fact {
                    fact: impact.change.fact.clone(),
                    old: impact.change.old.clone(),
                    new: impact.change.new.clone(),
                    change: impact.change.kind,
                },
                pages,
            )
            .with_blocks(impact.blocks.clone()))
        })
        .collect()
}

/// The `facts` check class: one result per block that states a value which
/// moved.
///
/// Every one of them fails. A block reached by a change is a block whose
/// rendered value is the old one until the page is built again — there is no
/// passing case to report, because a fact that did not move produces no
/// `FactChange` and no `Impact`.
pub fn checks(
    impacts: &[Impact],
    routes: &dyn Routes,
    scrubber: &Scrubber,
) -> Result<Vec<CheckResult>, StoreError> {
    let mut out = Vec::new();
    for impact in impacts {
        let origins: Vec<EdgeOrigin> = impact
            .blocks
            .iter()
            .map(|(origin, _)| origin.clone())
            .collect();
        for (origin, route) in origins.iter().zip(routes.route_each(&origins)?) {
            let Some(route) = route else {
                continue;
            };
            out.push(finding(impact, origin, &route, scrubber));
        }
    }
    Ok(out)
}

fn finding(
    impact: &Impact,
    origin: &EdgeOrigin,
    route: &Route,
    scrubber: &Scrubber,
) -> CheckResult {
    let excerpt = scrubber.excerpt(&format!(
        "`{}` is {} here and {} at its source",
        impact.change.fact,
        show(impact.change.old.as_ref()),
        show(impact.change.new.as_ref()),
    ));
    CheckResult {
        id: id_of(origin, route),
        outcome: CheckOutcome::Fail { excerpt },
        // Comparing two values that are already in hand takes no measurable
        // time, and a made-up number would be worse than zero.
        duration: Duration::ZERO,
        // What the result is about, so the same staleness on the next run has
        // the same digest and a different value does not.
        digest: Fingerprint::of_parts([
            route.as_str().as_bytes(),
            impact.change.fact.as_str().as_bytes(),
            show(impact.change.new.as_ref()).as_bytes(),
        ]),
    }
}

/// A `Page` origin has no block to name, and the contract's `CheckId` wants
/// one, so the page's own identity stands in for it.
///
/// Shared with [`super::spec`]: both report a finding about one block of one
/// page, and they have to name it the same way or a report cannot group them.
pub(super) fn id_of(origin: &EdgeOrigin, route: &Route) -> CheckId {
    match origin {
        EdgeOrigin::Block(_, block) => check_id(route, block, 0),
        EdgeOrigin::Page(page) => CheckId::new(format!("{}#{page}#0", route.as_str())),
    }
}

/// A short form for a diagnostic, not the reader's rendering — that is the
/// theme's and needs a locale this crate does not have.
fn show(value: Option<&FactValue>) -> String {
    let Some(value) = value else {
        return "absent".to_owned();
    };
    match value {
        FactValue::Str(text) | FactValue::Date(text) | FactValue::Enum(text) => text.clone(),
        FactValue::Num(number) => number.to_string(),
        FactValue::Percent(number) => format!("{number}%"),
        FactValue::Currency {
            amount,
            minor,
            code,
        } => currency(*amount, *minor, code),
        FactValue::Bool(flag) => flag.to_string(),
        FactValue::List(values) => {
            let parts: Vec<String> = values.iter().map(|v| show(Some(v))).collect();
            format!("[{}]", parts.join(", "))
        }
        // An object, and whatever a later `FactValue` variant is. JSON is
        // unambiguous and short enough for an excerpt.
        other => serde_json::to_string(other).unwrap_or_else(|_| "?".to_owned()),
    }
}

fn currency(amount: i64, minor: u8, code: &str) -> String {
    let scale = 10i64.pow(u32::from(minor));
    let whole = amount / scale;
    let rest = (amount % scale).abs();
    if minor == 0 {
        return format!("{whole} {code}");
    }
    format!("{whole}.{rest:0>width$} {code}", width = usize::from(minor))
}

#[cfg(test)]
mod tests;
