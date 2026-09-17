use std::sync::Mutex;
use std::time::Duration;

use liyasa_core::conformance::block_on;
use liyasa_core::ids::{BlockId, Route};
use liyasa_core::verify::CheckInput;

use super::*;
use crate::core::config::RunnersConfig;
use crate::core::policy::Skip;
use crate::runners::lang::{Node, Python, Rust, Shell};

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A sandbox that runs a two-command shell: `echo <text>` and `exit <n>`.
/// Enough to drive every assertion without a container, and it reads the job
/// the runner actually built rather than a script the test also wrote.
#[derive(Default)]
struct TinyShell {
    jobs: Mutex<Vec<SandboxJob>>,
    refuse: Option<SandboxError>,
}

impl TinyShell {
    fn refusing(error: SandboxError) -> Self {
        Self {
            refuse: Some(error),
            ..Self::default()
        }
    }

    fn script(job: &SandboxJob) -> String {
        job.files
            .iter()
            .find(|(path, _)| path.as_str().starts_with("main"))
            .map(|(_, bytes)| String::from_utf8_lossy(bytes.as_ref()).to_string())
            .unwrap_or_default()
    }
}

impl Sandbox for TinyShell {
    fn exec<'a>(&'a self, job: SandboxJob) -> BoxFut<'a, Result<SandboxOutput, SandboxError>> {
        if let Some(error) = &self.refuse {
            return Box::pin(std::future::ready(Err(error.clone())));
        }
        let script = Self::script(&job);
        self.jobs.lock().expect("not poisoned").push(job);
        let mut stdout = String::new();
        let mut exit = 0;
        for line in script.lines() {
            let line = line.trim();
            if let Some(text) = line.strip_prefix("echo ") {
                stdout.push_str(text.trim_matches('"'));
                stdout.push('\n');
            } else if let Some(code) = line.strip_prefix("exit ") {
                exit = code.trim().parse().unwrap_or(0);
                break;
            }
        }
        Box::pin(std::future::ready(Ok(SandboxOutput {
            exit,
            stdout: Bytes::from(stdout.into_bytes()),
            stderr: Bytes::from(b"a line of stderr".to_vec()),
            duration: Duration::from_millis(3),
        })))
    }
}

struct NoSecrets;

impl SecretSource for NoSecrets {
    fn get(&self, _name: &str) -> Option<zeroize::Zeroizing<String>> {
        None
    }
}

fn images() -> Images {
    Images::new(&RunnersConfig {
        images: [("shell".to_owned(), format!("busybox@{DIGEST}"))]
            .into_iter()
            .collect(),
        ..RunnersConfig::default()
    })
}

fn spec(source: &str, expect: Vec<Expectation>) -> CheckSpec {
    CheckSpec {
        id: CheckId::new("/guide#block#0"),
        page: Route::new("/guide"),
        block: BlockId::explicit("block"),
        runner: "shell".to_owned(),
        input: CheckInput::Code {
            lang: "bash".to_owned(),
            source: source.to_owned(),
            hidden_lines: Vec::new(),
        },
        expect,
        timeout: Duration::from_secs(30),
        needs_network: false,
        needs_secrets: Vec::new(),
    }
}

fn runner() -> SandboxRunner {
    SandboxRunner::new(Arc::new(Shell), images())
}

fn run(runner: &SandboxRunner, spec: &CheckSpec, sandbox: &dyn Sandbox) -> CheckOutcome {
    block_on(runner.run(spec, sandbox, &NoSecrets)).outcome
}

#[test]
fn a_shell_block_that_does_what_it_says_passes() {
    let outcome = run(
        &runner(),
        &spec(
            "echo hello\n",
            vec![Expectation::Stdout("hello".to_owned())],
        ),
        &TinyShell::default(),
    );
    assert_eq!(outcome, CheckOutcome::Pass);
}

#[test]
fn a_shell_block_whose_output_is_not_what_it_claims_fails_with_an_excerpt() {
    let outcome = run(
        &runner(),
        &spec(
            "echo goodbye\n",
            vec![Expectation::Stdout("hello".to_owned())],
        ),
        &TinyShell::default(),
    );
    let CheckOutcome::Fail { excerpt } = outcome else {
        panic!("not a failure: {outcome:?}");
    };
    assert!(excerpt.contains("hello"), "{excerpt}");
    assert!(excerpt.len() <= 512);
}

#[test]
fn a_command_that_errors_fails_even_when_the_block_said_nothing_about_exit() {
    let outcome = run(
        &runner(),
        &spec(
            "echo hello\nexit 3\n",
            vec![Expectation::Stdout("hello".to_owned())],
        ),
        &TinyShell::default(),
    );
    assert!(
        matches!(&outcome, CheckOutcome::Fail { excerpt } if excerpt.contains("exited 3")),
        "{outcome:?}"
    );
}

#[test]
fn a_block_that_expects_a_failure_gets_one() {
    let outcome = run(
        &runner(),
        &spec("exit 1\n", vec![Expectation::Exit(1)]),
        &TinyShell::default(),
    );
    assert_eq!(outcome, CheckOutcome::Pass);
}

#[test]
fn a_timeout_is_e0603_rather_than_a_failing_sample() {
    let outcome = run(
        &runner(),
        &spec("echo hello\n", Vec::new()),
        &TinyShell::refusing(SandboxError::Timeout),
    );
    let CheckOutcome::Error(problem) = outcome else {
        panic!("not an error: {outcome:?}");
    };
    assert_eq!(problem.code, code::E0603);
}

#[test]
fn no_sandbox_at_all_is_e0004_and_not_a_failing_sample() {
    let outcome = run(
        &runner(),
        &spec("echo hello\n", Vec::new()),
        &TinyShell::refusing(SandboxError::Unavailable),
    );
    assert!(
        matches!(&outcome, CheckOutcome::Error(p) if p.code == code::E0004),
        "{outcome:?}"
    );
}

#[test]
fn an_engine_that_could_not_start_the_job_is_e0612() {
    let outcome = run(
        &runner(),
        &spec("echo hello\n", Vec::new()),
        &TinyShell::refusing(SandboxError::Io("no space left on device".to_owned())),
    );
    assert!(
        matches!(&outcome, CheckOutcome::Error(p) if p.code == code::E0612),
        "{outcome:?}"
    );
}

#[test]
fn an_image_with_no_digest_is_e0610_and_the_sandbox_is_never_asked() {
    let bare = SandboxRunner::new(Arc::new(Shell), Images::default());
    let sandbox = TinyShell::default();
    let outcome = run(&bare, &spec("echo hello\n", Vec::new()), &sandbox);
    assert!(
        matches!(&outcome, CheckOutcome::Error(p) if p.code == code::E0610),
        "{outcome:?}"
    );
    assert!(sandbox.jobs.lock().expect("not poisoned").is_empty());
}

#[test]
fn the_job_carries_the_pin_the_timeout_and_no_network() {
    let sandbox = TinyShell::default();
    run(&runner(), &spec("echo hello\n", Vec::new()), &sandbox);
    let jobs = sandbox.jobs.lock().expect("not poisoned");
    assert_eq!(jobs[0].image, "busybox");
    assert_eq!(jobs[0].digest, DIGEST);
    assert_eq!(jobs[0].timeout, Duration::from_secs(30));
    assert!(!jobs[0].network);
}

#[test]
fn a_language_the_runner_does_not_claim_is_skipped_not_run() {
    let mut spec = spec("print(1)\n", Vec::new());
    spec.input = CheckInput::Code {
        lang: "python".to_owned(),
        source: "print(1)\n".to_owned(),
        hidden_lines: Vec::new(),
    };
    let sandbox = TinyShell::default();
    assert!(matches!(
        run(&runner(), &spec, &sandbox),
        CheckOutcome::Skip { .. }
    ));
    assert!(sandbox.jobs.lock().expect("not poisoned").is_empty());
}

#[test]
fn a_block_marked_skip_never_reaches_the_sandbox() {
    let spec = spec("echo hello\n", Vec::new());
    let runner = runner().with_bindings(Bindings::new().with(
        spec.id.clone(),
        Binding {
            mode: Mode::Skip(Skip {
                reason: Some("needs a live cluster".to_owned()),
            }),
            ..Binding::default()
        },
    ));
    let sandbox = TinyShell::default();
    let outcome = run(&runner, &spec, &sandbox);
    assert!(
        matches!(&outcome, CheckOutcome::Skip { reason } if reason == "needs a live cluster"),
        "{outcome:?}"
    );
    assert!(sandbox.jobs.lock().expect("not poisoned").is_empty());
}

#[test]
fn hidden_lines_run_and_are_not_in_what_the_block_showed() {
    let mut spec = spec("", Vec::new());
    spec.input = CheckInput::Code {
        lang: "rust".to_owned(),
        source: "# let hidden = 41;\nassert_eq!(hidden + 1, 42);\n".to_owned(),
        hidden_lines: Vec::new(),
    };
    spec.runner = "rust".to_owned();
    let rust = SandboxRunner::new(
        Arc::new(Rust),
        Images::new(&RunnersConfig {
            images: [("rust".to_owned(), format!("rust@{DIGEST}"))]
                .into_iter()
                .collect(),
            ..RunnersConfig::default()
        }),
    );
    let sandbox = TinyShell::default();
    run(&rust, &spec, &sandbox);
    let jobs = sandbox.jobs.lock().expect("not poisoned");
    let (_, main) = jobs[0]
        .files
        .iter()
        .find(|(p, _)| p.as_str() == "src/main.rs")
        .expect("a program");
    let main = String::from_utf8_lossy(main.as_ref());
    assert!(main.contains("let hidden = 41;"), "{main}");
    assert!(!main.contains("# let"), "{main}");
}

#[test]
fn the_declared_environment_reaches_the_job() {
    let spec = spec("echo hello\n", Vec::new());
    let runner = runner().with_bindings(Bindings::new().with(
        spec.id.clone(),
        Binding {
            env: vec![("TOKEN".to_owned(), "abc".to_owned())],
            ..Binding::default()
        },
    ));
    let sandbox = TinyShell::default();
    run(&runner, &spec, &sandbox);
    let jobs = sandbox.jobs.lock().expect("not poisoned");
    assert_eq!(jobs[0].env, vec![("TOKEN".to_owned(), "abc".to_owned())]);
}

#[test]
fn a_fixture_is_staged_beside_the_sample() {
    let spec = spec("echo hello\n", Vec::new());
    let runner = runner().with_bindings(Bindings::new().with(
        spec.id.clone(),
        Binding {
            fixtures: vec![(VfsPath::new("data/users.json"), Bytes::from(b"[]".to_vec()))],
            ..Binding::default()
        },
    ));
    let sandbox = TinyShell::default();
    run(&runner, &spec, &sandbox);
    let jobs = sandbox.jobs.lock().expect("not poisoned");
    assert!(
        jobs[0]
            .files
            .iter()
            .any(|(p, _)| p.as_str() == "data/users.json")
    );
}

#[test]
fn expect_file_compares_against_the_file_the_caller_read() {
    let spec_ok = spec(
        "echo hello\n",
        vec![Expectation::StdoutFile(VfsPath::new("out.txt"))],
    );
    let binding = Binding {
        expected: [(
            VfsPath::new("out.txt"),
            Bytes::from(b"hello   \n\n".to_vec()),
        )]
        .into_iter()
        .collect(),
        ..Binding::default()
    };
    let runner = runner().with_bindings(Bindings::new().with(spec_ok.id.clone(), binding));
    assert_eq!(
        run(&runner, &spec_ok, &TinyShell::default()),
        CheckOutcome::Pass,
        "trailing whitespace is not what a sample is about"
    );
}

#[test]
fn expect_file_that_was_never_read_is_a_failure_rather_than_a_silent_pass() {
    let spec = spec(
        "echo hello\n",
        vec![Expectation::StdoutFile(VfsPath::new("out.txt"))],
    );
    assert!(matches!(
        run(&runner(), &spec, &TinyShell::default()),
        CheckOutcome::Fail { .. }
    ));
}

#[test]
fn compile_only_asserts_nothing_about_output() {
    let spec = spec(
        "echo goodbye\n",
        vec![Expectation::Stdout("hello".to_owned())],
    );
    let runner = runner().with_bindings(Bindings::new().with(
        spec.id.clone(),
        Binding {
            mode: Mode::Compile,
            ..Binding::default()
        },
    ));
    let sandbox = TinyShell::default();
    assert_eq!(run(&runner, &spec, &sandbox), CheckOutcome::Pass);
    let jobs = sandbox.jobs.lock().expect("not poisoned");
    assert!(jobs[0].cmd.contains(&"-n".to_owned()), "{:?}", jobs[0].cmd);
}

#[test]
fn a_sandboxed_runner_says_so() {
    assert_eq!(runner().isolation(), Isolation::Sandbox);
    assert_eq!(runner().id(), "shell");
    assert!(runner().languages().contains(&"bash"));
}

#[test]
fn the_digest_does_not_move_between_runs_of_the_same_check() {
    let spec = spec(
        "echo hello\n",
        vec![Expectation::Stdout("hello".to_owned())],
    );
    let runner = runner();
    let sandbox = TinyShell::default();
    let first = block_on(runner.run(&spec, &sandbox, &NoSecrets)).digest;
    let second = block_on(runner.run(&spec, &sandbox, &NoSecrets)).digest;
    assert_eq!(first, second);
}

#[test]
fn every_built_in_runner_satisfies_the_runner_contract() {
    // The kit `liyasa-core` ships for exactly this (§34.9). Each language's
    // fixture is a block that passes and one that does not, in its own syntax,
    // driven by the same sandbox.
    let languages: [(Arc<dyn Language>, &str); 3] = [
        (Arc::new(Shell), "shell"),
        (Arc::new(Python), "python"),
        (Arc::new(Node), "node"),
    ];
    for (language, image) in languages {
        let (pass, boom) = ("echo hello\n", "echo hello\nexit 1\n");
        let lang = language.languages()[0];
        let runner = SandboxRunner::new(
            Arc::clone(&language),
            Images::new(&RunnersConfig {
                images: [(image.to_owned(), format!("{image}@{DIGEST}"))]
                    .into_iter()
                    .collect(),
                ..RunnersConfig::default()
            }),
        );
        liyasa_core::conformance::runner::check(
            &runner,
            &TinyShell::default(),
            &NoSecrets,
            &liyasa_core::conformance::runner::Fixture {
                passing: CheckInput::Code {
                    lang: lang.to_owned(),
                    source: pass.to_owned(),
                    hidden_lines: Vec::new(),
                },
                failing: CheckInput::Code {
                    lang: lang.to_owned(),
                    source: boom.to_owned(),
                    hidden_lines: Vec::new(),
                },
                expect: vec![Expectation::Stdout("hello".to_owned())],
            },
        );
    }
}
