use std::sync::Mutex;
use std::time::Duration;

use liyasa_core::conformance::block_on;
use liyasa_core::vfs::Bytes;

use super::*;

#[derive(Default)]
struct Fake {
    seen: Mutex<Vec<Invocation>>,
    programs: Vec<String>,
}

impl Exec for Fake {
    fn run(
        &self,
        invocation: Invocation,
        _timeout: Duration,
    ) -> Result<SandboxOutput, SandboxError> {
        self.seen.lock().expect("not poisoned").push(invocation);
        Ok(SandboxOutput {
            exit: 0,
            stdout: Bytes::default(),
            stderr: Bytes::default(),
            duration: Duration::ZERO,
        })
    }

    fn available(&self, program: &str) -> bool {
        self.programs.iter().any(|p| p == program)
    }
}

fn with(programs: &[&str]) -> Arc<Fake> {
    Arc::new(Fake {
        programs: programs.iter().map(|p| (*p).to_owned()).collect(),
        ..Fake::default()
    })
}

fn config(sandbox: SandboxKind) -> RunnersConfig {
    RunnersConfig {
        sandbox,
        ..RunnersConfig::default()
    }
}

#[test]
fn the_server_refuses_the_local_sandbox() {
    let problem = allowed(SandboxKind::Local, Host::Server).expect_err("refused");
    assert_eq!(problem.code, code::E0620);
}

#[test]
fn the_cli_accepts_the_local_sandbox() {
    assert!(allowed(SandboxKind::Local, Host::Cli).is_ok());
}

#[test]
fn the_server_accepts_the_two_isolated_sandboxes() {
    assert!(allowed(SandboxKind::Container, Host::Server).is_ok());
    assert!(allowed(SandboxKind::Remote, Host::Server).is_ok());
}

#[test]
fn building_a_local_sandbox_under_the_server_is_e0620_and_builds_nothing() {
    let problem = Builder::new(Host::Server)
        .with_exec(with(&["docker"]))
        .build(&config(SandboxKind::Local))
        .err()
        .expect("refused");
    assert_eq!(problem.code, code::E0620);
}

#[test]
fn the_container_sandbox_needs_an_engine() {
    let problem = Builder::new(Host::Cli)
        .with_exec(with(&[]))
        .build(&config(SandboxKind::Container))
        .err()
        .expect("no engine");
    // RFC 0908: the code registered for exactly this and waiting for a runner
    // that could reach it.
    assert_eq!(problem.code, code::E0004);
}

#[test]
fn the_container_sandbox_is_built_when_an_engine_is_installed() {
    assert!(
        Builder::new(Host::Server)
            .with_exec(with(&["podman"]))
            .build(&config(SandboxKind::Container))
            .is_ok()
    );
}

#[test]
fn the_remote_sandbox_needs_a_service() {
    let problem = Builder::new(Host::Server)
        .with_exec(with(&["docker"]))
        .build(&config(SandboxKind::Remote))
        .err()
        .expect("no service");
    assert_eq!(problem.code, code::E0611);
}

#[test]
fn a_runner_handed_no_sandbox_is_refused_rather_than_run_on_the_host() {
    let error = block_on(Unavailable.exec(SandboxJob {
        image: "x".to_owned(),
        digest: "sha256:0".to_owned(),
        cmd: vec!["sh".to_owned()],
        files: Vec::new(),
        env: Vec::new(),
        timeout: Duration::from_secs(1),
        network: false,
        cpu_millis: 0,
        mem_bytes: 0,
    }))
    .expect_err("no sandbox");
    assert_eq!(error, SandboxError::Unavailable);
}
