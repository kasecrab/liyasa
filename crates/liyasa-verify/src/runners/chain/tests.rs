use std::sync::Mutex;
use std::time::Duration;

use liyasa_core::conformance::block_on;
use liyasa_core::ids::{BlockId, Route};
use liyasa_core::net::BoxFut;
use liyasa_core::verify::{Expectation, SandboxError, SandboxJob, SandboxOutput, SecretSource};
use liyasa_core::vfs::Bytes;

use super::*;
use crate::core::config::RunnersConfig;
use crate::runners::image::Images;
use crate::runners::lang::Shell;

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[derive(Default)]
struct Echoing(Mutex<Vec<SandboxJob>>);

impl Sandbox for Echoing {
    fn exec<'a>(&'a self, job: SandboxJob) -> BoxFut<'a, Result<SandboxOutput, SandboxError>> {
        let script = job
            .files
            .iter()
            .find(|(p, _)| p.as_str() == "main.sh")
            .map(|(_, b)| String::from_utf8_lossy(b.as_ref()).to_string())
            .unwrap_or_default();
        self.0.lock().expect("not poisoned").push(job);
        let stdout: String = script
            .lines()
            .filter_map(|l| l.trim().strip_prefix("echo ").map(|t| format!("{t}\n")))
            .collect();
        Box::pin(std::future::ready(Ok(SandboxOutput {
            exit: 0,
            stdout: Bytes::from(stdout.into_bytes()),
            stderr: Bytes::default(),
            duration: Duration::from_millis(1),
        })))
    }
}

struct NoSecrets;

impl SecretSource for NoSecrets {
    fn get(&self, _name: &str) -> Option<zeroize::Zeroizing<String>> {
        None
    }
}

fn runner() -> SandboxRunner {
    SandboxRunner::new(
        std::sync::Arc::new(Shell),
        Images::new(&RunnersConfig {
            images: [("shell".to_owned(), format!("busybox@{DIGEST}"))]
                .into_iter()
                .collect(),
            ..RunnersConfig::default()
        }),
    )
}

fn step(n: u32, lang: &str, source: &str, expect: Vec<Expectation>) -> Step {
    Step {
        spec: CheckSpec {
            id: CheckId::new(format!("/guide#block#{n}")),
            page: Route::new("/guide"),
            block: BlockId::explicit("block"),
            runner: "shell".to_owned(),
            input: CheckInput::Code {
                lang: lang.to_owned(),
                source: source.to_owned(),
                hidden_lines: Vec::new(),
            },
            expect,
            timeout: Duration::from_secs(10),
            needs_network: false,
            needs_secrets: Vec::new(),
        },
        binding: Binding::default(),
    }
}

#[test]
fn a_chain_runs_its_steps_in_one_sandbox_in_order() {
    let sandbox = Echoing::default();
    let steps = [
        step(0, "bash", "export TOKEN=abc\n", Vec::new()),
        step(1, "bash", "echo second\n", Vec::new()),
        step(2, "bash", "echo third\n", Vec::new()),
    ];
    let results = block_on(run(&runner(), &steps, &sandbox, &NoSecrets));
    assert_eq!(results.len(), 3);
    let jobs = sandbox.0.lock().expect("not poisoned");
    assert_eq!(jobs.len(), 1, "one chain is one container");
    let script = String::from_utf8_lossy(jobs[0].files[0].1.as_ref()).to_string();
    assert_eq!(script, "export TOKEN=abc\necho second\necho third\n");
}

#[test]
fn every_step_gets_a_result_under_its_own_id() {
    let steps = [
        step(0, "bash", "echo one\n", Vec::new()),
        step(1, "bash", "echo two\n", Vec::new()),
    ];
    let results = block_on(run(&runner(), &steps, &Echoing::default(), &NoSecrets));
    assert_eq!(
        results.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec!["/guide#block#0", "/guide#block#1"]
    );
}

#[test]
fn a_step_that_asserts_on_a_later_steps_output_passes_because_it_is_one_run() {
    let steps = [
        step(
            0,
            "bash",
            "echo one\n",
            vec![Expectation::Stdout("one".to_owned())],
        ),
        step(
            1,
            "bash",
            "echo two\n",
            vec![Expectation::Stdout("two".to_owned())],
        ),
    ];
    let results = block_on(run(&runner(), &steps, &Echoing::default(), &NoSecrets));
    assert!(
        results.iter().all(|r| r.outcome == CheckOutcome::Pass),
        "{results:?}"
    );
}

#[test]
fn a_chain_gets_the_sum_of_its_steps_timeouts() {
    let sandbox = Echoing::default();
    let steps = [
        step(0, "bash", "echo one\n", Vec::new()),
        step(1, "bash", "echo two\n", Vec::new()),
    ];
    block_on(run(&runner(), &steps, &sandbox, &NoSecrets));
    let jobs = sandbox.0.lock().expect("not poisoned");
    assert_eq!(jobs[0].timeout, Duration::from_secs(20));
}

#[test]
fn a_chain_whose_steps_change_language_is_e0614_on_every_step() {
    let steps = [
        step(0, "bash", "echo one\n", Vec::new()),
        step(1, "python", "print('two')\n", Vec::new()),
    ];
    let results = block_on(run(&runner(), &steps, &Echoing::default(), &NoSecrets));
    assert_eq!(results.len(), 2);
    for result in &results {
        assert!(
            matches!(&result.outcome, CheckOutcome::Error(p) if p.code == code::E0614),
            "{result:?}"
        );
    }
}

#[test]
fn a_broken_chain_still_names_every_step_it_was_asked_about() {
    let steps = [
        step(0, "bash", "echo one\n", Vec::new()),
        step(1, "python", "print(1)\n", Vec::new()),
    ];
    let results = block_on(run(&runner(), &steps, &Echoing::default(), &NoSecrets));
    assert_eq!(
        results.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec!["/guide#block#0", "/guide#block#1"]
    );
}

#[test]
fn an_empty_chain_is_no_results_rather_than_a_panic() {
    assert!(block_on(run(&runner(), &[], &Echoing::default(), &NoSecrets)).is_empty());
}

#[test]
fn only_the_last_steps_exit_status_is_asserted() {
    // One process has one exit status, and it is the last step's.
    let sandbox = Echoing::default();
    let steps = [
        step(0, "bash", "echo one\n", vec![Expectation::Exit(0)]),
        step(1, "bash", "echo two\n", vec![Expectation::Exit(0)]),
    ];
    let results = block_on(run(&runner(), &steps, &sandbox, &NoSecrets));
    assert!(results.iter().all(|r| r.outcome == CheckOutcome::Pass));
}

#[test]
fn hidden_lines_keep_their_place_when_the_steps_are_joined() {
    let mut first = step(0, "bash", "one\ntwo\n", Vec::new());
    first.spec.input = CheckInput::Code {
        lang: "bash".to_owned(),
        source: "one\ntwo\n".to_owned(),
        hidden_lines: vec![1],
    };
    let mut second = step(1, "bash", "three\n", Vec::new());
    second.spec.input = CheckInput::Code {
        lang: "bash".to_owned(),
        source: "three\n".to_owned(),
        hidden_lines: vec![1],
    };
    let joined = join(&[first, second]).expect("a chain");
    let CheckInput::Code { hidden_lines, .. } = &joined.spec.input else {
        panic!("not code");
    };
    assert_eq!(hidden_lines, &[1, 3]);
}
