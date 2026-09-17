//! `verify-chain` (VER-05).
//!
//! "Blocks in a `code-group` or `tabs` are verified independently; `steps`
//! blocks may declare `verify-chain` so each step runs in the same sandbox in
//! order." Independent is the default and needs no code: a block is a check.
//!
//! A chain is one job. The steps are concatenated in order and run once, so
//! step two sees the file step one wrote and the variable step one exported —
//! which is the whole point, and is not reproducible by running three
//! containers. Each step still gets its own `CheckResult`, because the report
//! and the graph are per block.
//!
//! What a chain cannot do is attribute output: one process wrote one stream,
//! so a step's `expect` is evaluated against the run's whole output rather
//! than a slice of it (RFC 2103). A chain that fails names the chain.

use std::time::Instant;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::ids::CheckId;
use liyasa_core::verify::{
    CheckInput, CheckOutcome, CheckResult, CheckSpec, Sandbox, SecretSource,
};

use super::code::{Binding, Bindings, SandboxRunner};

/// One step of a chain: the spec the report will carry, and what its fence
/// declared.
pub struct Step {
    pub spec: CheckSpec,
    pub binding: Binding,
}

/// Runs `steps` as a single job and returns one result per step.
///
/// The steps must share a language; a chain that changes language mid-way is
/// `E0614` rather than a run of whichever language came first.
pub async fn run(
    runner: &SandboxRunner,
    steps: &[Step],
    sandbox: &dyn Sandbox,
    secrets: &dyn SecretSource,
) -> Vec<CheckResult> {
    if steps.is_empty() {
        return Vec::new();
    }
    let started = Instant::now();
    let joined = match join(steps) {
        Ok(joined) => joined,
        Err(problem) => return steps.iter().map(|s| broken(s, &problem, started)).collect(),
    };
    let chained = SandboxRunner::new(runner.language(), runner.images().clone())
        .with_hide_prefix(runner.hide_prefix())
        .with_bindings(Bindings::new().with(joined.spec.id.clone(), joined.binding));
    let result = chained.run(&joined.spec, sandbox, secrets).await;
    steps
        .iter()
        .map(|step| CheckResult {
            id: step.spec.id.clone(),
            outcome: result.outcome.clone(),
            duration: result.duration,
            digest: result.digest,
        })
        .collect()
}

struct Joined {
    spec: CheckSpec,
    binding: Binding,
}

/// The chain as one check: the steps' sources in order, every step's
/// expectations, the sum of their timeouts, and the union of what they stage.
fn join(steps: &[Step]) -> Result<Joined, Diagnostic> {
    let first = &steps[0];
    let Some(lang) = language(&first.spec) else {
        return Err(Diagnostic::new(
            code::E0614,
            "a `verify-chain` step is not a code block",
        ));
    };
    let mut source = String::new();
    let mut expect = Vec::new();
    let mut hidden = Vec::new();
    let mut timeout = std::time::Duration::ZERO;
    let mut binding = first.binding.clone();
    binding.fixtures.clear();
    binding.expected.clear();
    binding.env.clear();

    for step in steps {
        match language(&step.spec) {
            Some(other) if other.eq_ignore_ascii_case(lang) => {}
            Some(other) => {
                return Err(Diagnostic::new(
                    code::E0614,
                    format!(
                        "a `verify-chain` runs one sandbox, and its steps are `{lang}` and `{other}`"
                    ),
                )
                .help("split the chain, or give every step the same language"));
            }
            None => {
                return Err(Diagnostic::new(
                    code::E0614,
                    "a `verify-chain` step is not a code block",
                ));
            }
        }
        let (body, lines) = body_of(&step.spec);
        let offset = source.lines().count() as u32;
        hidden.extend(lines.iter().map(|line| line + offset));
        source.push_str(&body);
        if !source.ends_with('\n') {
            source.push('\n');
        }
        expect.extend(step.spec.expect.iter().cloned());
        timeout += step.spec.timeout;
        binding.env.extend(step.binding.env.iter().cloned());
        binding
            .fixtures
            .extend(step.binding.fixtures.iter().cloned());
        binding.expected.extend(
            step.binding
                .expected
                .iter()
                .map(|(k, v)| (k.clone(), v.clone())),
        );
    }

    // One `Exit` expectation: the chain's, which is the last step's, because
    // the run is one process.
    let exits: Vec<_> = expect
        .iter()
        .filter(|e| matches!(e, liyasa_core::verify::Expectation::Exit(_)))
        .cloned()
        .collect();
    expect.retain(|e| !matches!(e, liyasa_core::verify::Expectation::Exit(_)));
    if let Some(last) = exits.last() {
        expect.push(last.clone());
    }

    Ok(Joined {
        spec: CheckSpec {
            id: CheckId::new(format!("{}#chain", first.spec.id.as_str())),
            input: CheckInput::Code {
                lang: lang.to_owned(),
                source,
                hidden_lines: hidden,
            },
            expect,
            timeout,
            needs_network: steps.iter().any(|s| s.spec.needs_network),
            needs_secrets: steps
                .iter()
                .flat_map(|s| s.spec.needs_secrets.iter().cloned())
                .collect(),
            ..first.spec.clone()
        },
        binding,
    })
}

fn language(spec: &CheckSpec) -> Option<&str> {
    match &spec.input {
        CheckInput::Code { lang, .. } => Some(lang),
        _ => None,
    }
}

fn body_of(spec: &CheckSpec) -> (String, Vec<u32>) {
    match &spec.input {
        CheckInput::Code {
            source,
            hidden_lines,
            ..
        } => (source.clone(), hidden_lines.clone()),
        _ => (String::new(), Vec::new()),
    }
}

fn broken(step: &Step, problem: &Diagnostic, started: Instant) -> CheckResult {
    CheckResult {
        id: step.spec.id.clone(),
        outcome: CheckOutcome::Error(problem.clone()),
        duration: started.elapsed(),
        digest: liyasa_core::ids::Fingerprint::of_parts([
            step.spec.id.as_str().as_bytes(),
            problem.code.as_str().as_bytes(),
            problem.message.as_bytes(),
        ]),
    }
}

#[cfg(test)]
mod tests;
