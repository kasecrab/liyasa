//! Runners that need no sandbox (VER-02.2 to VER-02.5, VER-03).
//!
//! VER-03 draws the line: a runner that executes user code needs a container,
//! a runner that only reads it does not. Everything here only reads, so link,
//! fact, spec, HTTP-mock, schema, and Mermaid verification work on a machine
//! with no Docker. The sandboxed runners are WP-21's.
//!
//! Every runner returns a `CheckResult` whose digest depends on the check and
//! its outcome and on nothing else, so re-running an unchanged check produces
//! an unchanged digest and the cache (VER-06) holds.

use std::sync::Arc;
use std::time::{Duration, Instant};

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::ids::Fingerprint;
use liyasa_core::verify::{CheckInput, CheckOutcome, CheckResult, CheckSpec, Expectation, Runner};

use super::scrub::Scrubber;

pub mod http;
pub mod mermaid;
pub mod regex;
pub mod schema;

/// The in-process runners, in the order a language is offered to them.
pub fn in_process() -> Registry {
    Registry::new(vec![
        Arc::new(schema::SchemaRunner),
        Arc::new(mermaid::MermaidRunner),
        Arc::new(regex::RegexRunner),
    ])
}

/// The runner registry VER-02 describes: languages to runners, first match
/// wins, unknown languages skipped with a warning.
#[derive(Clone, Default)]
pub struct Registry {
    runners: Vec<Arc<dyn Runner>>,
}

impl Registry {
    pub fn new(runners: Vec<Arc<dyn Runner>>) -> Self {
        Self { runners }
    }

    pub fn register(&mut self, runner: Arc<dyn Runner>) {
        self.runners.push(runner);
    }

    pub fn for_language(&self, lang: &str) -> Option<&dyn Runner> {
        let lang = lang.trim().to_ascii_lowercase();
        self.runners
            .iter()
            .find(|r| r.languages().contains(&lang.as_str()))
            .map(Arc::as_ref)
    }

    pub fn by_id(&self, id: &str) -> Option<&dyn Runner> {
        self.runners.iter().find(|r| r.id() == id).map(Arc::as_ref)
    }

    pub fn languages(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = self
            .runners
            .iter()
            .flat_map(|r| r.languages().iter().copied())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    pub fn is_empty(&self) -> bool {
        self.runners.is_empty()
    }
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field(
                "runners",
                &self.runners.iter().map(|r| r.id()).collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// `E0602`: the language has no runner. VER-02 calls this a warning and a
/// skip, not a failure, so the caller decides which it is from policy.
pub fn no_runner(lang: &str) -> Diagnostic {
    Diagnostic::new(
        code::E0602,
        format!("no runner claims the language `{lang}`"),
    )
    .help("declare one in `verify.runners.custom`, or mark the block `verify=skip`")
}

/// The `(lang, source)` of a code check, or `None` for another input shape.
pub(crate) fn code_of(spec: &CheckSpec) -> Option<(&str, &str)> {
    match &spec.input {
        CheckInput::Code { lang, source, .. } => Some((lang, source)),
        CheckInput::Schema { lang, source, .. } => Some((lang, source)),
        _ => None,
    }
}

/// The `expect="…"` strings a block declared.
pub(crate) fn expected_text(spec: &CheckSpec) -> Vec<&str> {
    spec.expect
        .iter()
        .filter_map(|e| match e {
            Expectation::Stdout(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// A skip carrying the reason the report will print.
pub(crate) fn skip(reason: impl Into<String>) -> CheckOutcome {
    CheckOutcome::Skip {
        reason: reason.into(),
    }
}

/// A failure whose excerpt is scrubbed and capped before it is stored
/// (§30.2.4).
pub(crate) fn fail(scrubber: &Scrubber, excerpt: impl AsRef<str>) -> CheckOutcome {
    CheckOutcome::Fail {
        excerpt: scrubber.excerpt(excerpt.as_ref()),
    }
}

/// The scrubber a check's own secrets, and nothing else, are registered in.
pub(crate) fn scrubber_for(
    spec: &CheckSpec,
    secrets: &dyn liyasa_core::verify::SecretSource,
) -> Scrubber {
    Scrubber::from_secret_source(secrets, &spec.needs_secrets)
}

pub(crate) fn finish(
    spec: &CheckSpec,
    runner_id: &str,
    outcome: CheckOutcome,
    started: Instant,
) -> CheckResult {
    CheckResult {
        digest: digest(spec, runner_id, &outcome),
        id: spec.id.clone(),
        outcome,
        duration: started.elapsed(),
    }
}

/// The check, what it asserted, and what came out — never the clock, or
/// VER-06's cache key would change on every run.
fn digest(spec: &CheckSpec, runner_id: &str, outcome: &CheckOutcome) -> Fingerprint {
    let input = serde_json::to_vec(&spec.input).unwrap_or_default();
    let expect = serde_json::to_vec(&spec.expect).unwrap_or_default();
    let (tag, detail) = match outcome {
        CheckOutcome::Pass => ("pass", String::new()),
        CheckOutcome::Fail { excerpt } => ("fail", excerpt.clone()),
        CheckOutcome::Skip { reason } => ("skip", reason.clone()),
        CheckOutcome::Error(diagnostic) => (
            "error",
            format!("{} {}", diagnostic.code, diagnostic.message),
        ),
    };
    Fingerprint::of_parts([
        runner_id.as_bytes(),
        spec.id.as_str().as_bytes(),
        input.as_slice(),
        expect.as_slice(),
        tag.as_bytes(),
        detail.as_bytes(),
    ])
}

/// Every in-process runner is synchronous; this keeps the `BoxFut` in the
/// contract from spreading `async` through code that never awaits.
pub(crate) fn ready<T: Send + 'static>(value: T) -> liyasa_core::net::BoxFut<'static, T> {
    Box::pin(std::future::ready(value))
}

/// What a runner reports when a check outruns `CheckSpec::timeout`. No
/// in-process runner can be interrupted mid-call, so this is the shape the
/// sandboxed runners and the link sweep report a timeout in.
pub fn timed_out(limit: Duration) -> CheckOutcome {
    CheckOutcome::Error(Diagnostic::new(
        code::E0603,
        format!("the check did not finish within {} ms", limit.as_millis()),
    ))
}

#[cfg(test)]
pub(crate) mod testing;

#[cfg(test)]
mod tests;
