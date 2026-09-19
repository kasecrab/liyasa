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
use liyasa_core::document::{Block, BlockKind, Inline, Node};
use liyasa_core::ids::Route;
use liyasa_core::verify::{CheckOutcome, CheckResult, Sandbox, SecretSource};

use crate::core::config::VerifyConfig;
use crate::core::runners::{Registry, no_runner};
use crate::runners::attrs::{self, Mode, Site};

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

    fn absorb(&mut self, other: Run) {
        self.results.extend(other.results);
        self.problems.extend(other.problems.into_vec());
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
    pub registry: &'a Registry,
    pub config: &'a VerifyConfig,
    pub sandbox: &'a dyn Sandbox,
    pub secrets: &'a dyn SecretSource,
}

impl Orchestrator<'_> {
    /// Runs every verified block of every page, in order, until the budget is
    /// spent.
    ///
    /// [`Self::page`] is the other entry point and the split is deliberate: a
    /// caller driving this from a job queue wants one job per page, because a
    /// sweep's normal outcome is that some checks fail and a backoff ladder
    /// measured in minutes would retry every check in the sweep for the sake
    /// of the few that did (RFC 1404). `site` is for a caller that wants one
    /// unit of work and will handle partial failure itself.
    pub async fn site(&self, pages: &[Page<'_>], budget: &mut Budget) -> Run {
        let mut out = Run::default();
        for page in pages {
            out.absorb(self.page(page, budget).await);
        }
        if budget.spent() {
            let queued = out.skipped_for_budget();
            if queued > 0 {
                out.problems.push(budget_exceeded(queued));
            }
        }
        out
    }

    /// Runs one page's verified blocks.
    pub async fn page(&self, page: &Page<'_>, budget: &mut Budget) -> Run {
        let mut out = Run::default();
        for (nth, block) in verifiable(page.root).into_iter().enumerate() {
            out.absorb(self.block(page, block, nth as u32, budget).await);
        }
        out
    }

    async fn block(&self, page: &Page<'_>, block: &Block, nth: u32, budget: &Budget) -> Run {
        let mut out = Run::default();
        let BlockKind::CodeBlock { lang, attrs, .. } = &block.kind else {
            return out;
        };
        let lang = lang.as_deref().unwrap_or_default();

        // `claimed` is what tells `attrs::read` whether a language nobody runs
        // is a request under `default: "all"`. It is the caller's because the
        // registry is assembled from config.
        let claimed = self.registry.for_language(lang).is_some();
        let (verify, problems) = attrs::read(attrs, self.config, claimed);
        out.problems.extend(problems);

        // No `verify` attribute under `tagged`, or an unclaimed language under
        // `all`: the author asked for nothing, so there is nothing to report.
        let Some(verify) = verify else {
            return out;
        };

        let at = Site {
            page: &page.route,
            block: block.id,
            nth,
            runner: runner_id(self.registry, lang),
            lang,
        };
        let spec = verify.spec(&at, &source_of(block), Vec::new());

        // `verify-chain` means the steps share one sandbox in order (VER-05),
        // which `chain::run` does and this walk does not yet call. Running
        // them independently would produce results that answer a different
        // question from the one the author asked — step two without step
        // one's side effects — so the block is reported as not run instead.
        // Wiring `chain::run` in is the next packet; reporting a wrong answer
        // would not be.
        if verify.chain {
            out.results.push(result(
                &spec,
                CheckOutcome::Skip {
                    reason: CHAIN_REASON.to_owned(),
                },
            ));
            return out;
        }

        if let Mode::Skip(skip) = &verify.mode {
            out.results.push(result(
                &spec,
                CheckOutcome::Skip {
                    reason: skip.reason(),
                },
            ));
            return out;
        }

        // Every path below this point reports the block. The author asked for
        // it to run; a report that omits it is the defect this module exists
        // to close.
        if budget.spent() {
            out.results.push(result(
                &spec,
                CheckOutcome::Skip {
                    reason: BUDGET_REASON.to_owned(),
                },
            ));
            return out;
        }

        let Some(runner) = self.registry.for_language(lang) else {
            out.problems.push(no_runner(lang));
            out.results.push(result(
                &spec,
                CheckOutcome::Skip {
                    reason: format!("no runner claims the language `{lang}`"),
                },
            ));
            return out;
        };

        out.results
            .push(runner.run(&spec, self.sandbox, self.secrets).await);
        out
    }
}

/// What a declared chain carries until `chain::run` is wired into the walk.
const CHAIN_REASON: &str = "`verify-chain` runs the steps in one sandbox, which this run cannot do yet; \
     running them independently would answer a different question";

/// The reason a check not started for want of time carries, and what `W0622`
/// counts.
const BUDGET_REASON: &str = "the verification budget was spent before this check started";

impl Run {
    fn skipped_for_budget(&self) -> usize {
        self.results
            .iter()
            .filter(
                |r| matches!(&r.outcome, CheckOutcome::Skip { reason } if reason == BUDGET_REASON),
            )
            .count()
    }
}

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

/// The id a `Site` records. A language with no runner still needs one, and
/// naming the language is more useful in a report than an empty string.
fn runner_id<'a>(registry: &'a Registry, lang: &'a str) -> &'a str {
    registry.for_language(lang).map_or(lang, |r| r.id())
}

/// Every code block under a root, in document order.
fn verifiable(root: &Block) -> Vec<&Block> {
    let mut out = Vec::new();
    collect(root, &mut out);
    out
}

fn collect<'a>(block: &'a Block, out: &mut Vec<&'a Block>) {
    if matches!(block.kind, BlockKind::CodeBlock { .. }) {
        out.push(block);
    }
    for child in &block.children {
        if let Node::Block(inner) = child {
            collect(inner, out);
        }
    }
}

/// A fence's body. `liyasa-markdown` puts it in one `Inline::Text` child
/// verbatim, and it is taken verbatim: leading whitespace is part of the
/// program.
fn source_of(block: &Block) -> String {
    let mut out = String::new();
    for child in &block.children {
        if let Node::Inline(Inline::Text(text)) = child {
            out.push_str(text);
        }
    }
    out
}

#[cfg(test)]
mod tests;
