//! A verified example that does not pass (VER-01, VER-73).
//!
//! This reads a completed [`Run`](crate::core::Run) rather than taking part in
//! one, so `core` never needs to know `drift` exists.
//!
//! Only `CheckOutcome::Fail` is drift. An `Error` is the *checker* failing — an
//! unpinned container image, a sandbox refused before it was consulted, a
//! secret that did not resolve — and recording that as documentation drift
//! would put a configuration problem on an author's task list and, worse, would
//! make "this site has drift" true for a reason nobody can fix by editing a
//! page. A `Skip` is not drift either: nothing was checked.

use std::collections::BTreeSet;

use liyasa_core::ids::{CheckId, Route};
use liyasa_core::verify::{CheckOutcome, CheckResult};

use super::record::{Candidate, DriftKey, DriftKind};

/// One candidate per failing check.
pub fn candidates(results: &[CheckResult]) -> Vec<Candidate> {
    results
        .iter()
        .filter_map(|result| {
            let CheckOutcome::Fail { excerpt } = &result.outcome else {
                return None;
            };
            let page = route_of(&result.id)?;
            Some(Candidate::new(
                DriftKind::Check {
                    check: result.id.clone(),
                    excerpt: excerpt.clone(),
                },
                vec![page],
            ))
        })
        .collect()
}

/// Every check the run examined, pass or fail — which is the set that lets a
/// check that started passing again close its record (RFC 2060).
pub fn coverage(results: &[CheckResult]) -> BTreeSet<DriftKey> {
    results
        .iter()
        .map(|result| DriftKey::Check(result.id.clone()))
        .collect()
}

/// The route out of a `CheckId`, which is the inverse of
/// [`check_id`](crate::runners::check_id).
///
/// `CheckId` is `<page route>#<block id>#<n>` (§34.9) and a `Route` is a
/// site-relative path, so it cannot contain the separator and the first one
/// ends the route. The round trip against `check_id` is pinned by a test, so a
/// change to the format breaks here rather than silently producing records
/// attributed to no page.
pub fn route_of(check: &CheckId) -> Option<Route> {
    let (route, _) = check.as_str().split_once('#')?;
    (!route.is_empty()).then(|| Route::new(route))
}

#[cfg(test)]
mod tests;
