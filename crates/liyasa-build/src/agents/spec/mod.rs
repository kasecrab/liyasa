//! The Agent-Friendly Documentation Spec check set, its scoring model, and the
//! report (PRD §25; SPEC-01, SPEC-03, SPEC-04, SPEC-05).

pub mod checks;
pub mod options;
pub mod report;
pub mod score;

pub use checks::{CHECKS, Category, Check, Requires, Scope, Severity, Weight};
pub use options::{Options, Sampling, Thresholds};
pub use report::{Effect, Finding, Report};
pub use score::{CheckResult, Grade, Outcome, RunFacts, Score};

/// The spec release the check set is held to. `agents.specVersion` defaults to
/// it and SPEC-03 fails a release when the two disagree.
pub const SPEC_VERSION: &str = "0.6.0";
