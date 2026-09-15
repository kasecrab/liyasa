//! The `regex` runner (VER-02.5): the pattern compiles and matches `expect`.
//!
//! A `regex` block is a pattern, not a program, so `expect="…"` names the
//! subject the pattern has to match rather than what the block prints.

use std::time::Instant;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::net::BoxFut;
use liyasa_core::verify::{
    CheckOutcome, CheckResult, CheckSpec, Isolation, Runner, Sandbox, SecretSource,
};

use super::{code_of, expected_text, fail, finish, ready, scrubber_for, skip};

/// A rule package or a docs page is operator-supplied content, so a pattern
/// that backtracks badly must cost a bounded amount of memory rather than the
/// process (RFC 1300).
const SIZE_LIMIT: usize = 1 << 20;
const DFA_SIZE_LIMIT: usize = 1 << 20;

pub struct RegexRunner;

impl RegexRunner {
    pub const ID: &'static str = "regex";
}

impl Runner for RegexRunner {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn languages(&self) -> &'static [&'static str] {
        &["regex", "regexp"]
    }

    fn isolation(&self) -> Isolation {
        Isolation::InProcess
    }

    fn run<'a>(
        &'a self,
        spec: &'a CheckSpec,
        _sandbox: &'a dyn Sandbox,
        secrets: &'a dyn SecretSource,
    ) -> BoxFut<'a, CheckResult> {
        let started = Instant::now();
        let scrubber = scrubber_for(spec, secrets);
        let outcome = match code_of(spec) {
            Some((lang, source))
                if self
                    .languages()
                    .contains(&lang.to_ascii_lowercase().as_str()) =>
            {
                check(source, &expected_text(spec), &scrubber)
            }
            Some((lang, _)) => skip(format!("the `regex` runner does not claim `{lang}`")),
            None => skip("the `regex` runner reads code blocks only"),
        };
        ready(finish(spec, Self::ID, outcome, started))
    }
}

fn check(source: &str, expected: &[&str], scrubber: &crate::core::scrub::Scrubber) -> CheckOutcome {
    let pattern = source.trim();
    if pattern.is_empty() {
        return CheckOutcome::Error(Diagnostic::new(
            code::E0601,
            "a `regex` block is empty, so there is no pattern to compile",
        ));
    }
    let compiled = match ::regex::RegexBuilder::new(pattern)
        .size_limit(SIZE_LIMIT)
        .dfa_size_limit(DFA_SIZE_LIMIT)
        .build()
    {
        Ok(compiled) => compiled,
        Err(error) => return fail(scrubber, format!("the pattern does not compile: {error}")),
    };
    // VER-02.5 is two assertions: it compiles, and it matches what the block
    // says it matches. A block with no `expect` has made only the first.
    let misses: Vec<&&str> = expected
        .iter()
        .filter(|subject| !compiled.is_match(subject))
        .collect();
    if misses.is_empty() {
        CheckOutcome::Pass
    } else {
        fail(
            scrubber,
            format!(
                "`{pattern}` does not match {}",
                misses
                    .iter()
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
    }
}
