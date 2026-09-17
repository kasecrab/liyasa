//! VER-03: `local` is refused by the server, and a container job has no
//! network, a read-only root, and the pinned digest.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use liyasa_core::diagnostics::code;
use liyasa_core::verify::{SandboxError, SandboxJob, SandboxOutput};
use liyasa_verify::core::config::{RunnersConfig, SandboxKind};
use liyasa_verify::runners::sandbox::{
    self, Builder, Engine, Exec, Host, Invocation, Limits, container,
};

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[derive(Default)]
struct Installed {
    programs: Vec<String>,
    seen: Mutex<Vec<Invocation>>,
}

impl Exec for Installed {
    fn run(
        &self,
        invocation: Invocation,
        _timeout: Duration,
    ) -> Result<SandboxOutput, SandboxError> {
        self.seen.lock().expect("not poisoned").push(invocation);
        Ok(SandboxOutput {
            exit: 0,
            stdout: liyasa_core::vfs::Bytes::default(),
            stderr: liyasa_core::vfs::Bytes::default(),
            duration: Duration::ZERO,
        })
    }

    fn available(&self, program: &str) -> bool {
        self.programs.iter().any(|p| p == program)
    }
}

fn exec(programs: &[&str]) -> Arc<Installed> {
    Arc::new(Installed {
        programs: programs.iter().map(|p| (*p).to_owned()).collect(),
        seen: Mutex::new(Vec::new()),
    })
}

fn config(sandbox: SandboxKind) -> RunnersConfig {
    RunnersConfig {
        sandbox,
        ..RunnersConfig::default()
    }
}

#[test]
fn the_server_refuses_the_local_sandbox_with_e0620() {
    let problem = sandbox::allowed(SandboxKind::Local, Host::Server).expect_err("refused");
    assert_eq!(problem.code, code::E0620);

    let problem = Builder::new(Host::Server)
        .with_exec(exec(&["podman"]))
        .build(&config(SandboxKind::Local))
        .err()
        .expect("refused");
    assert_eq!(problem.code, code::E0620);
}

#[test]
fn a_developer_machine_may_use_it() {
    assert!(sandbox::allowed(SandboxKind::Local, Host::Cli).is_ok());
    assert!(
        Builder::new(Host::Cli)
            .with_exec(exec(&[]))
            .build(&config(SandboxKind::Local))
            .is_ok()
    );
}

#[test]
fn the_container_sandbox_cuts_the_network_and_pins_the_digest() {
    let job = SandboxJob {
        image: "docker.io/library/rust".to_owned(),
        digest: DIGEST.to_owned(),
        cmd: vec!["cargo".to_owned(), "run".to_owned()],
        files: Vec::new(),
        env: Vec::new(),
        timeout: Duration::from_secs(30),
        network: false,
        cpu_millis: 0,
        mem_bytes: 0,
    };
    let argv = container::argv(
        Engine::Docker,
        &job,
        &Limits::default(),
        Path::new("/stage"),
    );
    let line = argv.join(" ");

    assert!(line.contains("--network=none"), "{line}");
    assert!(line.contains("--read-only"), "{line}");
    assert!(
        line.contains(&format!("docker.io/library/rust@{DIGEST}")),
        "{line}"
    );
    assert!(line.contains("--cap-drop=ALL"), "{line}");
    assert!(line.contains("--security-opt=no-new-privileges"), "{line}");
    assert!(line.contains("--cpus="), "{line}");
    assert!(line.contains("--memory="), "{line}");
}

#[test]
fn a_container_sandbox_needs_an_engine_and_says_so_with_the_code_that_was_waiting() {
    let problem = Builder::new(Host::Cli)
        .with_exec(exec(&[]))
        .build(&config(SandboxKind::Container))
        .err()
        .expect("no engine");
    // RFC 0908: E0004 was registered and unraised because nothing could reach
    // it until a sandboxed runner existed.
    assert_eq!(problem.code, code::E0004);
}
