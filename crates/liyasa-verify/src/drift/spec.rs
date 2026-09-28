//! Spec drift: what a moved operation does to the pages that document it
//! (VER-12, API-21).
//!
//! The comparison is `sources::openapi::operation_changes`, which diffs two
//! versions of a document over exactly the facets a reader of the page sees —
//! parameters and request body, responses, and what it takes to call the
//! operation. The walk is `PathImpact::operation_impact`. Both are built. This
//! is the record and the finding, which is the half API-21 was waiting for.
//!
//! It lives here rather than in `liyasa-openapi` or `liyasa-build` because
//! neither of those can hold it: the comparison needs the dependency graph to
//! know which page documents the operation, and `liyasa-build` does not depend
//! on `liyasa-verify` at all. `liyasa-cli` depends on both and is the caller.

use std::time::Duration;

use liyasa_core::document::EdgeOrigin;
use liyasa_core::ids::{Fingerprint, Route};
use liyasa_core::verify::{CheckOutcome, CheckResult, StoreError};

use crate::core::Scrubber;
use crate::sources::OperationImpact;

use super::engine::Routes;
use super::facts::id_of;
use super::record::{Candidate, DriftKind};

pub fn candidates(
    impacts: &[OperationImpact],
    routes: &dyn Routes,
) -> Result<Vec<Candidate>, StoreError> {
    impacts
        .iter()
        .map(|impact| {
            let pages = routes.routes(&impact.blocks)?;
            Ok(Candidate::new(
                DriftKind::Operation {
                    spec: impact.change.spec.clone(),
                    op: impact.change.op.clone(),
                    diff: impact.change.diff.clone(),
                },
                pages,
            )
            .with_blocks(impact.blocks.clone()))
        })
        .collect()
}

/// VER-12 flags a page with the diff, so the diff is what the finding carries.
///
/// These are `facts` class findings, not a class of their own: a spec is a truth
/// source, and `verify.policy` has no `spec` key to turn off.
pub fn checks(
    impacts: &[OperationImpact],
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
    impact: &OperationImpact,
    origin: &EdgeOrigin,
    route: &Route,
    scrubber: &Scrubber,
) -> CheckResult {
    let change = &impact.change;
    let moved = change.diff.join(", ");
    let excerpt = scrubber.excerpt(&format!(
        "`{}` in `{}` moved: {moved}",
        change.op, change.spec
    ));
    CheckResult {
        id: id_of(origin, route),
        outcome: CheckOutcome::Fail { excerpt },
        duration: Duration::ZERO,
        digest: Fingerprint::of_parts([
            route.as_str().as_bytes(),
            change.spec.as_bytes(),
            change.op.as_bytes(),
            moved.as_bytes(),
        ]),
    }
}

#[cfg(test)]
mod tests;
