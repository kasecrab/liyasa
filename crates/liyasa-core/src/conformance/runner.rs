//! What every verification `Runner` must do (PRD §14, §34.9).
//!
//! There is no reference `Runner` in `liyasa-core`: every runner executes
//! code, which needs a sandbox. WP-21's runners are the first implementations
//! this kit runs against.

use std::time::Duration;

use super::{block_on, require};
use crate::ids::{BlockId, CheckId, Route};
use crate::verify::{
    CheckInput, CheckOutcome, CheckSpec, Expectation, Runner, Sandbox, SecretSource,
};

/// A check the runner is expected to pass and one it is expected to fail, in
/// the runner's own language.
pub struct Fixture {
    pub passing: CheckInput,
    pub failing: CheckInput,
    pub expect: Vec<Expectation>,
}

fn spec(id: &str, input: CheckInput, expect: Vec<Expectation>) -> CheckSpec {
    CheckSpec {
        id: CheckId::new(id),
        page: Route::new("/conformance"),
        block: BlockId::explicit("conformance"),
        runner: String::new(),
        input,
        expect,
        timeout: Duration::from_secs(30),
        needs_network: false,
        needs_secrets: Vec::new(),
    }
}

pub fn check(
    runner: &dyn Runner,
    sandbox: &dyn Sandbox,
    secrets: &dyn SecretSource,
    fixture: &Fixture,
) {
    require!(!runner.id().is_empty(), "a runner must have an id");
    require!(
        !runner.languages().is_empty(),
        "a runner must declare at least one language"
    );
    require!(
        runner
            .languages()
            .iter()
            .all(|l| l.chars().all(|c| !c.is_ascii_uppercase())),
        "language names are lowercase so a fence info string matches without folding"
    );

    let passing = spec(
        "/conformance#pass#0",
        fixture.passing.clone(),
        fixture.expect.clone(),
    );
    let result = block_on(runner.run(&passing, sandbox, secrets));
    require!(
        result.id == passing.id,
        "a result carries the id of the spec it ran"
    );
    require!(
        matches!(result.outcome, CheckOutcome::Pass),
        "the passing fixture produced {:?}",
        result.outcome
    );

    let repeat = block_on(runner.run(&passing, sandbox, secrets));
    require!(
        repeat.digest == result.digest,
        "the digest must depend on the check and its output, not on when it ran"
    );

    let failing = spec(
        "/conformance#fail#0",
        fixture.failing.clone(),
        fixture.expect.clone(),
    );
    let result = block_on(runner.run(&failing, sandbox, secrets));
    match &result.outcome {
        CheckOutcome::Fail { excerpt } => {
            require!(
                excerpt.len() <= 512,
                "a failure excerpt is capped at 512 bytes; this one is {}",
                excerpt.len()
            );
        }
        other => panic!("contract violated: the failing fixture produced {other:?}, not Fail"),
    }

    let unsupported = spec(
        "/conformance#skip#0",
        CheckInput::Code {
            lang: "definitely-not-a-language".to_owned(),
            source: String::new(),
            hidden_lines: Vec::new(),
        },
        Vec::new(),
    );
    let result = block_on(runner.run(&unsupported, sandbox, secrets));
    require!(
        matches!(
            result.outcome,
            CheckOutcome::Skip { .. } | CheckOutcome::Error(_)
        ),
        "a language the runner does not handle is a Skip or an Error, never a silent Pass"
    );
}
