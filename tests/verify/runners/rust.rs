//! VER-04 and VER-02.6: a Rust block's `# ` lines are executed and are not in
//! what the reader is shown, and `edition` and `deps` reach the crate.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use liyasa_core::conformance::block_on;
use liyasa_core::ids::{BlockId, CheckId, Route};
use liyasa_core::net::BoxFut;
use liyasa_core::verify::{
    CheckInput, CheckSpec, Runner, Sandbox, SandboxError, SandboxJob, SandboxOutput, SecretSource,
};
use liyasa_core::vfs::Bytes;
use liyasa_verify::core::config::RunnersConfig;
use liyasa_verify::runners::hidden;
use liyasa_verify::runners::{Binding, Bindings, Images, Rust, SandboxRunner};

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[derive(Default)]
struct Capture(Mutex<Vec<SandboxJob>>);

impl Sandbox for Capture {
    fn exec<'a>(&'a self, job: SandboxJob) -> BoxFut<'a, Result<SandboxOutput, SandboxError>> {
        self.0.lock().expect("not poisoned").push(job);
        Box::pin(std::future::ready(Ok(SandboxOutput {
            exit: 0,
            stdout: Bytes::default(),
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
        Arc::new(Rust),
        Images::new(&RunnersConfig {
            images: [("rust".to_owned(), format!("rust@{DIGEST}"))]
                .into_iter()
                .collect(),
            ..RunnersConfig::default()
        }),
    )
}

fn spec(source: &str) -> CheckSpec {
    CheckSpec {
        id: CheckId::new("/guide#block#0"),
        page: Route::new("/guide"),
        block: BlockId::explicit("block"),
        runner: "rust".to_owned(),
        input: CheckInput::Code {
            lang: "rust".to_owned(),
            source: source.to_owned(),
            hidden_lines: Vec::new(),
        },
        expect: Vec::new(),
        timeout: Duration::from_secs(30),
        needs_network: false,
        needs_secrets: Vec::new(),
    }
}

fn staged(runner: &SandboxRunner, spec: &CheckSpec, path: &str) -> String {
    let sandbox = Capture::default();
    block_on(runner.run(spec, &sandbox, &NoSecrets));
    let jobs = sandbox.0.lock().expect("not poisoned");
    let (_, bytes) = jobs[0]
        .files
        .iter()
        .find(|(p, _)| p.as_str() == path)
        .unwrap_or_else(|| panic!("{path} is not staged"));
    String::from_utf8_lossy(bytes.as_ref()).to_string()
}

const SAMPLE: &str = "# let hidden = 41;\nassert_eq!(hidden + 1, 42);\n";

#[test]
fn a_hidden_line_is_executed() {
    let program = staged(&runner(), &spec(SAMPLE), "src/main.rs");
    assert!(program.contains("let hidden = 41;"), "{program}");
    assert!(!program.contains("# let hidden"), "{program}");
}

#[test]
fn a_hidden_line_is_not_in_what_the_reader_sees() {
    let split = hidden::split("rust", SAMPLE, hidden::DEFAULT_PREFIX);
    assert_eq!(split.visible, "assert_eq!(hidden + 1, 42);\n");
    assert_eq!(split.hidden_lines, vec![1]);
}

#[test]
fn a_block_that_is_a_body_becomes_a_program() {
    let program = staged(&runner(), &spec(SAMPLE), "src/main.rs");
    assert!(program.starts_with("fn main() {"), "{program}");
}

#[test]
fn edition_and_deps_reach_the_generated_crate() {
    let spec = spec("fn main() {}\n");
    let runner = runner().with_bindings(Bindings::new().with(
        spec.id.clone(),
        Binding {
            attrs: [
                ("edition".to_owned(), "2021".to_owned()),
                ("deps".to_owned(), "serde=1".to_owned()),
            ]
            .into_iter()
            .collect(),
            ..Binding::default()
        },
    ));
    let manifest = staged(&runner, &spec, "Cargo.toml");
    assert!(manifest.contains("edition = \"2021\""), "{manifest}");
    assert!(manifest.contains("serde = \"1\""), "{manifest}");
}

#[test]
fn a_setup_block_is_compiled_in_front_of_the_sample() {
    let spec = spec("assert_eq!(hidden + 1, 42);\n");
    let runner = runner().with_bindings(Bindings::new().with(
        spec.id.clone(),
        Binding {
            setup: Some("let hidden = 41;".to_owned()),
            ..Binding::default()
        },
    ));
    let program = staged(&runner, &spec, "src/main.rs");
    assert!(program.contains("let hidden = 41;"), "{program}");
    assert!(
        program.find("let hidden").expect("setup") < program.find("assert_eq").expect("sample"),
        "{program}"
    );
}
