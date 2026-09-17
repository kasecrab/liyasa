//! Refreshing every declared source, on a schedule or on demand (VER-22,
//! VER-24, VER-25).
//!
//! The driver is where VER-25's untrusted-build rule is actually applied: a
//! source that leaves the machine is not refreshed by a fork pull request or by
//! a branch outside `verify.sources.trustedBranches`, and the latest
//! *production* snapshot stands in for it. [`DeclaredSource`] refuses the same
//! thing on its own, which is the backstop; this is the path that makes the
//! build work anyway rather than fail.
//!
//! What comes out is [`Facts`]: every fact, its value, and the trust of the
//! source it came from. The trust travels with the value because the page that
//! interpolates it needs it — a value below `operator` is Markdown-escaped
//! (CM-20) and badged in the Truth dashboard (VER-26), and neither is possible
//! from the value alone.

use std::collections::BTreeMap;
use std::time::SystemTime;

use liyasa_core::ai::TrustLevel;
use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::ids::FactId;
use liyasa_core::net::HttpClient;
use liyasa_core::verify::{FactChange, FactValue, Sandbox, Snapshot, TruthSource};
use serde_json::Value;

use super::kinds::{Attestation, BuildTrust, DeclaredSource};
use super::snapshot::SnapshotLog;
use super::trust::needs_escaping;

/// One fact as the build receives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    pub value: FactValue,
    pub source: String,
    pub trust: TrustLevel,
}

/// Every fact a refresh produced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts(pub BTreeMap<FactId, Fact>);

impl Facts {
    pub fn get(&self, fact: &FactId) -> Option<&Fact> {
        self.0.get(fact)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The `facts.*` layer of a template context, as nested objects: a fact
    /// called `plan.pro.price` arrives as `facts.plan.pro.price`, which is what
    /// `fact("plan.pro.price")` walks.
    pub fn as_context(&self) -> Value {
        let mut root = serde_json::Map::new();
        for (id, fact) in &self.0 {
            let parts: Vec<&str> = id.as_str().split('.').collect();
            let Some((last, path)) = parts.split_last() else {
                continue;
            };
            let value = serde_json::to_value(&fact.value).unwrap_or(Value::Null);
            insert_at(&mut root, path, last, value);
        }
        Value::Object(root)
    }

    /// The facts that entered below `operator` trust, which is exactly the set
    /// a build must Markdown-escape before interpolating (CM-20, VER-26).
    ///
    /// This crate cannot do the escaping itself — `escape_untrusted` is
    /// `liyasa-markdown`'s and depending on it from here would invert the
    /// dependency — so it says which values need it.
    pub fn untrusted(&self) -> Vec<&FactId> {
        self.0
            .iter()
            .filter(|(_, fact)| needs_escaping(fact.trust))
            .map(|(id, _)| id)
            .collect()
    }
}

/// What one pass over the sources did.
#[derive(Debug, Clone, Default)]
pub struct RefreshReport {
    /// Sources that were fetched and snapshotted.
    pub refreshed: Vec<String>,
    /// Sources an untrusted build read from the last production snapshot
    /// instead of refreshing (VER-25).
    pub reused: Vec<String>,
    pub changes: Vec<FactChange>,
    pub diagnostics: Vec<Diagnostic>,
    pub facts: Facts,
}

impl RefreshReport {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }
}

/// Refreshes a set of sources into a [`SnapshotLog`].
pub struct Refresher<'a> {
    log: &'a SnapshotLog,
    build: BuildTrust,
    /// What the snapshots are attributed to: a branch, a build, or a schedule.
    by: String,
}

impl<'a> Refresher<'a> {
    pub fn new(log: &'a SnapshotLog, build: BuildTrust, by: impl Into<String>) -> Self {
        Self {
            log,
            build,
            by: by.into(),
        }
    }

    /// The sources whose last snapshot is older than `interval`, and every
    /// source that has none. VER-22's schedule, decided rather than slept
    /// through: the caller owns the timer.
    pub fn due<'s>(
        &self,
        sources: &'s [DeclaredSource],
        now: SystemTime,
        interval: std::time::Duration,
    ) -> Vec<&'s DeclaredSource> {
        sources
            .iter()
            .filter(|source| match self.log.latest(source.id()) {
                Ok(Some(stored)) => now
                    .duration_since(stored.snapshot.taken_at)
                    .map(|since| since >= interval)
                    .unwrap_or(false),
                _ => true,
            })
            .collect()
    }

    pub async fn refresh(
        &self,
        sources: &[DeclaredSource],
        http: &dyn HttpClient,
        sandbox: Option<&dyn Sandbox>,
        now: SystemTime,
    ) -> RefreshReport {
        let mut report = RefreshReport::default();
        for source in sources {
            if let Some(problem) = attestation_problem(source, now) {
                report.diagnostics.push(problem);
            }
            let taken = if source.leaves_the_machine() && !self.build.is_trusted() {
                match self.reuse(source, &mut report) {
                    Some(snapshot) => snapshot,
                    None => continue,
                }
            } else {
                match source.snapshot(http, sandbox).await {
                    Ok(snapshot) => {
                        report.refreshed.push(source.id().to_owned());
                        match self
                            .log
                            .record(snapshot.clone(), &self.by, self.build.is_trusted())
                        {
                            Ok(mut changes) => report.changes.append(&mut changes),
                            Err(why) => report.diagnostics.push(Diagnostic::new(
                                code::E0604,
                                format!("`{}` was refreshed but not stored: {why}", source.id()),
                            )),
                        }
                        snapshot
                    }
                    Err(why) => {
                        report.diagnostics.push(Diagnostic::new(
                            code::E0604,
                            format!("`{}` could not be refreshed: {why}", source.id()),
                        ));
                        continue;
                    }
                }
            };
            self.collect(source, &taken, &mut report);
        }
        report
    }

    /// VER-25's fallback: the latest snapshot a trusted build took.
    fn reuse(&self, source: &DeclaredSource, report: &mut RefreshReport) -> Option<Snapshot> {
        match self.log.latest_production(source.id()) {
            Ok(Some(stored)) => {
                report.reused.push(source.id().to_owned());
                Some(stored.snapshot)
            }
            Ok(None) => {
                report.diagnostics.push(
                    Diagnostic::new(
                        code::E0604,
                        format!(
                            "`{}` is not refreshed by an untrusted build and has no production snapshot to stand in",
                            source.id()
                        ),
                    )
                    .help("run a build on a trusted branch once, so there is a snapshot to reuse"),
                );
                None
            }
            Err(why) => {
                report.diagnostics.push(Diagnostic::new(
                    code::E0604,
                    format!("`{}`: {why}", source.id()),
                ));
                None
            }
        }
    }

    fn collect(&self, source: &DeclaredSource, taken: &Snapshot, report: &mut RefreshReport) {
        for (fact, value) in &taken.values {
            if let Some(first) = report.facts.0.get(fact) {
                report.diagnostics.push(Diagnostic::new(
                    code::E0604,
                    format!(
                        "fact `{fact}` comes from both `{}` and `{}`",
                        first.source,
                        source.id()
                    ),
                ));
                continue;
            }
            report.facts.0.insert(
                fact.clone(),
                Fact {
                    value: value.clone(),
                    source: source.id().to_owned(),
                    trust: source.trust(),
                },
            );
        }
    }
}

/// A fact called `a` and one called `a.b` cannot both be reachable, and the
/// first one written keeps its place rather than the last silently replacing
/// it.
fn insert_at(root: &mut serde_json::Map<String, Value>, path: &[&str], last: &str, value: Value) {
    let mut at = root;
    for part in path {
        let entry = at
            .entry(*part)
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        match entry {
            Value::Object(nested) => at = nested,
            _ => return,
        }
    }
    at.entry(last).or_insert(value);
}

fn attestation_problem(source: &DeclaredSource, now: SystemTime) -> Option<Diagnostic> {
    let state = source.attestation(now);
    if state == Attestation::Valid || state == Attestation::NotApplicable {
        return None;
    }
    state.diagnostic(
        source.id(),
        source.spec().expires.as_deref().unwrap_or("its expiry"),
    )
}

#[cfg(test)]
mod tests;
