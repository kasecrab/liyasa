//! VER-02.1: shell blocks that pass, fail, and time out.

use std::sync::Arc;
use std::time::Duration;

use liyasa_core::conformance::block_on;
use liyasa_core::diagnostics::code;
use liyasa_core::ids::{BlockId, CheckId, Route};
use liyasa_core::net::BoxFut;
use liyasa_core::verify::{
    CheckInput, CheckOutcome, CheckSpec, Expectation, Isolation, Runner, Sandbox, SandboxError,
    SandboxJob, SandboxOutput, SecretSource,
};
use liyasa_core::vfs::Bytes;
use liyasa_verify::core::config::RunnersConfig;
use liyasa_verify::runners::{Images, SandboxRunner, Shell};

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Runs the two-command shell a sample needs: `echo` and `exit`. A block whose
/// first line is `sleep` never returns, the way a real one would not.
struct Tiny;

impl Sandbox for Tiny {
    fn exec<'a>(&'a self, job: SandboxJob) -> BoxFut<'a, Result<SandboxOutput, SandboxError>> {
        let script = job
            .files
            .iter()
            .find(|(p, _)| p.as_str() == "main.sh")
            .map(|(_, b)| String::from_utf8_lossy(b.as_ref()).to_string())
            .unwrap_or_default();
        if script.lines().any(|l| l.trim().starts_with("sleep ")) {
            return Box::pin(std::future::ready(Err(SandboxError::Timeout)));
        }
        let mut stdout = String::new();
        let mut exit = 0;
        for line in script.lines() {
            let line = line.trim();
            if let Some(text) = line.strip_prefix("echo ") {
                stdout.push_str(text.trim_matches('"'));
                stdout.push('\n');
            } else if let Some(status) = line.strip_prefix("exit ") {
                exit = status.trim().parse().unwrap_or(0);
                break;
            }
        }
        Box::pin(std::future::ready(Ok(SandboxOutput {
            exit,
            stdout: Bytes::from(stdout.into_bytes()),
            stderr: Bytes::from(b"connecting with token s3cret-t0ken-value".to_vec()),
            duration: Duration::from_millis(2),
        })))
    }
}

struct Store;

impl SecretSource for Store {
    fn get(&self, name: &str) -> Option<zeroize::Zeroizing<String>> {
        // At least `MIN_SECRET_LEN` characters, or the scrubber ignores it on
        // purpose: a short value would redact ordinary prose out of every
        // excerpt on the site.
        (name == "token").then(|| zeroize::Zeroizing::new("s3cret-t0ken-value".to_owned()))
    }
}

fn runner() -> SandboxRunner {
    SandboxRunner::new(
        Arc::new(Shell),
        Images::new(&RunnersConfig {
            images: [("shell".to_owned(), format!("busybox@{DIGEST}"))]
                .into_iter()
                .collect(),
            ..RunnersConfig::default()
        }),
    )
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
        needs_secrets: vec!["token".to_owned()],
    }
}

fn outcome(source: &str, expect: Vec<Expectation>) -> CheckOutcome {
    block_on(runner().run(&spec(source, expect), &Tiny, &Store)).outcome
}

#[test]
fn a_shell_block_that_does_what_it_claims_passes() {
    assert_eq!(
        outcome(
            "echo hello\n",
            vec![Expectation::Stdout("hello".to_owned())]
        ),
        CheckOutcome::Pass
    );
}

#[test]
fn a_shell_block_that_does_not_fails_with_a_scrubbed_excerpt() {
    let outcome = outcome(
        "echo goodbye\n",
        vec![Expectation::Stdout("hello".to_owned())],
    );
    let CheckOutcome::Fail { excerpt } = outcome else {
        panic!("not a failure: {outcome:?}");
    };
    assert!(excerpt.len() <= 512, "{} bytes", excerpt.len());
    assert!(
        !excerpt.contains("s3cret-t0ken-value"),
        "a declared secret reached the excerpt: {excerpt}"
    );
    assert!(
        excerpt.contains(liyasa_verify::core::REDACTED),
        "the secret was in the stderr and should have been replaced: {excerpt}"
    );
}

#[test]
fn a_shell_block_that_outruns_its_timeout_is_e0603() {
    let outcome = outcome("sleep 300\n", Vec::new());
    let CheckOutcome::Error(problem) = outcome else {
        panic!("not an error: {outcome:?}");
    };
    assert_eq!(problem.code, code::E0603);
}

#[test]
fn the_shell_runner_needs_a_sandbox_and_claims_the_shells_ver_02_1_names() {
    let runner = runner();
    assert_eq!(runner.isolation(), Isolation::Sandbox);
    for shell in ["bash", "sh", "zsh", "fish", "powershell"] {
        assert!(runner.languages().contains(&shell), "{shell}");
    }
}
