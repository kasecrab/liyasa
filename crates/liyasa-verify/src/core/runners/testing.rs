//! Test doubles for the seams an in-process runner is handed and never uses.

use std::time::Duration;

use liyasa_core::ids::{BlockId, CheckId, Route};
use liyasa_core::net::BoxFut;
use liyasa_core::verify::{
    CheckInput, CheckSpec, Expectation, Sandbox, SandboxError, SandboxJob, SandboxOutput,
    SecretSource,
};

pub struct NoSandbox;

impl Sandbox for NoSandbox {
    fn exec<'a>(&'a self, _job: SandboxJob) -> BoxFut<'a, Result<SandboxOutput, SandboxError>> {
        Box::pin(std::future::ready(Err(SandboxError::Unavailable)))
    }
}

#[derive(Default)]
pub struct Secrets(pub Vec<(String, String)>);

impl SecretSource for Secrets {
    fn get(&self, name: &str) -> Option<zeroize::Zeroizing<String>> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| zeroize::Zeroizing::new(value.clone()))
    }
}

pub fn spec(id: &str, input: CheckInput, expect: Vec<Expectation>) -> CheckSpec {
    CheckSpec {
        id: CheckId::new(id),
        page: Route::new("/test"),
        block: BlockId::explicit(id),
        runner: String::new(),
        input,
        expect,
        timeout: Duration::from_secs(30),
        needs_network: false,
        needs_secrets: Vec::new(),
    }
}

pub fn code(lang: &str, source: &str) -> CheckInput {
    CheckInput::Code {
        lang: lang.to_owned(),
        source: source.to_owned(),
        hidden_lines: Vec::new(),
    }
}

pub fn expect_text(text: &str) -> Expectation {
    Expectation::Stdout(text.to_owned())
}
