//! The join between a rendered page and the runners (RFC 1308).
//!
//! Every mechanism §14 needs was complete and tested before this module
//! existed, and none of it had ever run against a real page: `attrs::read`
//! turns a fence into a [`BlockVerify`], `BlockVerify::spec` turns that into a
//! `CheckSpec`, [`Registry`] picks a runner and the runner runs it. What was
//! missing is the walk — something that finds the fenced blocks of a document
//! and drives that machinery. This is only that.
//!
//! Three things it decides, because nothing else could (RFC 1308):
//!
//! - It takes the **Rendered AST plus a route**, never a path or a `Document`.
//!   `liyasa-build` depends on this crate, so the other direction is a cycle.
//! - A block the author asked to verify and which did not run is still
//!   **reported**, as a skip naming why. The report may say "did not run"; it
//!   may never say nothing.
//! - It holds a **deadline from `verify.budget`** and checks it before starting
//!   a check rather than interrupting one.

use std::time::{Duration, Instant};

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::Block;
use liyasa_core::ids::Route;
use liyasa_core::verify::{CheckOutcome, CheckResult, Sandbox, SecretSource};

use crate::core::config::VerifyConfig;
use crate::core::plan;
use crate::core::runners::{Registry, no_runner};

/// One page, as the orchestrator needs to see it.
pub struct Page<'a> {
    pub route: Route,
    /// The root of the Rendered AST.
    pub root: &'a Block,
}

/// What one run produced.
#[derive(Debug, Default)]
pub struct Run {
    pub results: Vec<CheckResult>,
    pub problems: Diagnostics,
}

impl Run {
    pub fn passed(&self) -> usize {
        self.count(|o| matches!(o, CheckOutcome::Pass))
    }

    pub fn failed(&self) -> usize {
        self.count(|o| matches!(o, CheckOutcome::Fail { .. } | CheckOutcome::Error(_)))
    }

    pub fn skipped(&self) -> usize {
        self.count(|o| matches!(o, CheckOutcome::Skip { .. }))
    }

    fn count(&self, f: impl Fn(&CheckOutcome) -> bool) -> usize {
        self.results.iter().filter(|r| f(&r.outcome)).count()
    }
}

/// The wall clock a run gets, from `verify.budget` (VER-72 is unbuilt; RFC
/// 1308 §3).
///
/// It is consulted **before** a check starts and never interrupts one: a
/// sandboxed run killed halfway leaves a container and a cache entry in an
/// unclear state, and a single check is already bounded by its own timeout.
pub struct Budget {
    deadline: Option<Instant>,
}

impl Budget {
    pub fn of(limit: Duration) -> Self {
        Self {
            deadline: Some(Instant::now() + limit),
        }
    }

    /// No ceiling. For a caller that has its own, such as a scheduled sweep.
    pub const fn unbounded() -> Self {
        Self { deadline: None }
    }

    /// `verify.budget.deploy` — what a deploy build gets.
    pub fn deploy(config: &VerifyConfig) -> Self {
        Self::of(config.budget.deploy.as_duration())
    }

    /// `verify.budget.full` — what a complete run gets.
    pub fn full(config: &VerifyConfig) -> Self {
        Self::of(config.budget.full.as_duration())
    }

    fn spent(&self) -> bool {
        self.deadline.is_some_and(|at| Instant::now() >= at)
    }
}

pub struct Orchestrator<'a> {
    pub config: &'a VerifyConfig,
    pub sandbox: &'a dyn Sandbox,
    pub secrets: &'a dyn SecretSource,
    /// Where `fixture=` and `expect-file=` are read from (VER-01).
    pub vfs: &'a dyn liyasa_core::vfs::Vfs,
}

impl Orchestrator<'_> {
    /// Plans and runs every page, building the registry from the plan.
    ///
    /// Two phases because the contracts force it: VER-01's `env=`, `setup=` and
    /// `fixture=` live in a `Bindings` table that `Runner::run` cannot be
    /// handed per call, so it is registry state and the registry cannot exist
    /// until the walk has produced the table (RFC 1309).
    pub async fn verify(&self, pages: &[Page<'_>], budget: &mut Budget) -> Run {
        // The probe must hold BOTH halves or it reports an in-process
        // language as unclaimed: `regex` and `mermaid` are in-process only,
        // `shell` and `python` sandboxed only.
        let (sandboxed, mut problems) = crate::runners::sandboxed(self.config);
        let probe = combined(sandboxed);
        let plan = plan::site(pages, self.config, &probe, self.vfs);
        problems.extend(plan.problems.into_vec());

        // The run registry is the same shape, but built from the plan's
        // bindings so VER-01's `env=` and `fixture=` reach the job.
        let (bound, more) = crate::runners::sandboxed_with(self.config, plan.bindings.clone());
        problems.extend(more.into_vec());
        let registry = combined(bound);

        let mut out = self.run(plan.checks, &registry, budget).await;
        out.problems.extend(problems.into_vec());
        out
    }

    /// Phase two: runs what the plan says to run.
    ///
    /// `registry` must be the one built from the plan's bindings, or every
    /// check runs with a default binding — which is the defect this shape
    /// exists to prevent, so `verify` is the entry point to prefer.
    pub async fn run(
        &self,
        checks: Vec<plan::Planned>,
        registry: &Registry,
        budget: &mut Budget,
    ) -> Run {
        let mut out = Run::default();
        let mut queued = 0usize;
        for check in checks {
            if let Some(settled) = check.settled {
                out.results.push(result(&check.spec, settled));
                continue;
            }
            if budget.spent() {
                queued += 1;
                out.results.push(result(
                    &check.spec,
                    CheckOutcome::Skip {
                        reason: BUDGET_REASON.to_owned(),
                    },
                ));
                continue;
            }
            let Some(runner) = registry.for_language(&check.lang) else {
                // The probe claimed it and the run registry does not, which can
                // only happen if the two were built from different config.
                out.problems.push(no_runner(&check.lang));
                out.results.push(result(
                    &check.spec,
                    CheckOutcome::Skip {
                        reason: format!("no runner claims the language `{}`", check.lang),
                    },
                ));
                continue;
            };
            out.results
                .push(runner.run(&check.spec, self.sandbox, self.secrets).await);
        }
        if queued > 0 {
            out.problems.push(budget_exceeded(queued));
        }
        out
    }
}

/// The in-process runners plus the sandboxed ones. In-process first, so a
/// language both could claim is answered without a container.
fn combined(sandboxed: Registry) -> Registry {
    let mut runners = crate::core::runners::in_process().into_runners();
    runners.extend(sandboxed.into_runners());
    Registry::new(runners)
}

/// The reason a check not started for want of time carries, and what `W0622`
/// counts.
const BUDGET_REASON: &str = "the verification budget was spent before this check started";

fn budget_exceeded(queued: usize) -> Diagnostic {
    Diagnostic::new(
        code::W0622,
        format!("the verification budget was spent; {queued} checks did not start"),
    )
    .help("raise `verify.budget.deploy`, or run the remainder off the deploy path")
}

/// A `CheckResult` for an outcome the orchestrator decided without running
/// anything. The duration is zero because nothing ran, and the digest is the
/// spec's, so a skip is still addressable in the report.
fn result(spec: &liyasa_core::verify::CheckSpec, outcome: CheckOutcome) -> CheckResult {
    CheckResult {
        id: spec.id.clone(),
        outcome,
        duration: Duration::ZERO,
        digest: liyasa_core::ids::Fingerprint::of(spec.id.as_str()),
    }
}

#[cfg(test)]
mod tests;
