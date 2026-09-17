//! The runner every sandboxed language shares (VER-02, VER-03, VER-04).
//!
//! One type, one `Language`, one container per check. Everything that must be
//! the same for every language — the image pin, the timeout, what a timeout
//! reports, what an excerpt may contain, how the digest is computed — is here
//! and cannot drift between runners.
//!
//! `CheckSpec` carries no `env`, `setup` or `fixture`, and VER-01 gives a
//! fence all three. The contract is frozen, so they arrive beside the spec in
//! a `Bindings` table keyed by `CheckId` — the orchestrator that built the
//! spec from the fence holds both (RFC 2103).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::ids::CheckId;
use liyasa_core::net::BoxFut;
use liyasa_core::verify::{
    CheckOutcome, CheckResult, CheckSpec, Expectation, Isolation, Runner, Sandbox, SandboxError,
    SandboxJob, SandboxOutput, SecretSource,
};
use liyasa_core::vfs::{Bytes, VfsPath};

use super::attrs::{BlockVerify, Mode};
use super::hidden;
use super::image::Images;
use super::lang::{Job, Language, Source};
use crate::core::runners::{code_of, fail, finish, scrubber_for, skip, timed_out};
use crate::core::scrub::Scrubber;

/// What a fence declared that `CheckSpec` has nowhere to carry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Binding {
    pub mode: Mode,
    pub env: Vec<(String, String)>,
    /// `setup="snippet-name"`, resolved to the snippet's text.
    pub setup: Option<String>,
    /// The fence's remaining attributes, for the languages that read one.
    pub attrs: BTreeMap<String, String>,
    /// `fixture="path"`, resolved to bytes and staged beside the sample.
    pub fixtures: Vec<(VfsPath, Bytes)>,
    /// `expect-file="path"`, resolved to bytes.
    pub expected: BTreeMap<VfsPath, Bytes>,
}

impl Binding {
    /// The half of a fence that can be read without a filesystem. The caller
    /// fills `setup`, `fixtures` and `expected`, which need one.
    pub fn from_block(block: &BlockVerify, attrs: BTreeMap<String, String>) -> Self {
        Self {
            mode: block.mode.clone(),
            env: block.env.clone(),
            setup: None,
            attrs,
            fixtures: Vec::new(),
            expected: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Bindings(BTreeMap<CheckId, Binding>);

impl Bindings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, id: CheckId, binding: Binding) {
        self.0.insert(id, binding);
    }

    #[must_use]
    pub fn with(mut self, id: CheckId, binding: Binding) -> Self {
        self.set(id, binding);
        self
    }

    pub fn get(&self, id: &CheckId) -> Option<&Binding> {
        self.0.get(id)
    }
}

pub struct SandboxRunner {
    language: Arc<dyn Language>,
    images: Images,
    bindings: Bindings,
    hide_prefix: String,
}

impl SandboxRunner {
    pub fn new(language: Arc<dyn Language>, images: Images) -> Self {
        Self {
            language,
            images,
            bindings: Bindings::new(),
            hide_prefix: hidden::DEFAULT_PREFIX.to_owned(),
        }
    }

    #[must_use]
    pub fn with_bindings(mut self, bindings: Bindings) -> Self {
        self.bindings = bindings;
        self
    }

    #[must_use]
    pub fn with_hide_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.hide_prefix = prefix.into();
        self
    }

    /// The three a chain needs to build a runner of its own from this one.
    pub fn language(&self) -> Arc<dyn Language> {
        Arc::clone(&self.language)
    }

    pub fn images(&self) -> &Images {
        &self.images
    }

    pub fn hide_prefix(&self) -> &str {
        &self.hide_prefix
    }

    fn claims(&self, lang: &str) -> bool {
        let lang = lang.trim().to_ascii_lowercase();
        self.language.languages().contains(&lang.as_str())
    }

    async fn execute(
        &self,
        spec: &CheckSpec,
        sandbox: &dyn Sandbox,
        scrubber: &Scrubber,
    ) -> CheckOutcome {
        let Some((lang, _)) = code_of(spec) else {
            return skip(format!(
                "the `{}` runner reads code blocks only",
                self.language.id()
            ));
        };
        if !self.claims(lang) {
            return skip(format!(
                "the `{}` runner does not claim `{lang}`",
                self.language.id()
            ));
        }
        let default = Binding::default();
        let binding = self.bindings.get(&spec.id).unwrap_or(&default);
        if let Mode::Skip(reason) = &binding.mode {
            return skip(reason.reason());
        }
        let Some(source) = hidden::executed(&spec.input, &self.hide_prefix) else {
            return skip("the block carries no source to run");
        };

        let pin = match self.images.pin_any(&[lang, self.language.id()]) {
            Ok(pin) => pin,
            Err(problem) => return CheckOutcome::Error(problem),
        };
        let job = match self.language.job(&Source {
            lang,
            code: &source,
            setup: binding.setup.as_deref(),
            mode: binding.mode.clone(),
            attrs: &binding.attrs,
        }) {
            Ok(job) => job,
            Err(problem) => return CheckOutcome::Error(problem),
        };

        let output = sandbox.exec(sandbox_job(&pin, job, binding, spec)).await;
        match output {
            Ok(output) => assert_all(spec, binding, &output, scrubber),
            Err(SandboxError::Timeout) => timed_out(spec.timeout),
            Err(SandboxError::Unavailable) => CheckOutcome::Error(
                Diagnostic::new(
                    code::E0004,
                    "this check needs a sandbox and none is available",
                )
                .help("install Podman or Docker, or set `verify.runners.sandbox` to `remote`"),
            ),
            Err(SandboxError::Image(image)) => CheckOutcome::Error(Diagnostic::new(
                code::E0610,
                format!("the sandbox refused the image `{image}`"),
            )),
            Err(SandboxError::Io(detail)) => CheckOutcome::Error(Diagnostic::new(
                code::E0612,
                format!("the sandbox could not run the check: {detail}"),
            )),
            // `SandboxError` is `#[non_exhaustive]`; a variant added after
            // this was written is reported as a broken check rather than
            // silently becoming a pass.
            Err(other) => CheckOutcome::Error(Diagnostic::new(
                code::E0612,
                format!("the sandbox could not run the check: {other}"),
            )),
        }
    }
}

fn sandbox_job(
    pin: &super::image::ImagePin,
    job: Job,
    binding: &Binding,
    spec: &CheckSpec,
) -> SandboxJob {
    let mut files = job.files;
    // The fixtures last, so a fixture named like a generated file is the one
    // the author put there rather than the one Liyasa wrote.
    files.extend(binding.fixtures.iter().cloned());
    SandboxJob {
        image: pin.image.clone(),
        digest: pin.digest.clone(),
        cmd: job.cmd,
        files,
        env: binding.env.clone(),
        timeout: spec.timeout,
        network: spec.needs_network,
        cpu_millis: 0,
        mem_bytes: 0,
    }
}

impl Runner for SandboxRunner {
    fn id(&self) -> &'static str {
        self.language.id()
    }

    fn languages(&self) -> &'static [&'static str] {
        self.language.languages()
    }

    fn isolation(&self) -> Isolation {
        Isolation::Sandbox
    }

    fn run<'a>(
        &'a self,
        spec: &'a CheckSpec,
        sandbox: &'a dyn Sandbox,
        secrets: &'a dyn SecretSource,
    ) -> BoxFut<'a, CheckResult> {
        Box::pin(async move {
            let started = Instant::now();
            let scrubber = scrubber_for(spec, secrets);
            let outcome = self.execute(spec, sandbox, &scrubber).await;
            finish(spec, self.language.id(), outcome, started)
        })
    }
}

/// Every expectation a code block can carry. The ones that belong to the
/// `http` runner are another runner's and are not asserted here; a variant
/// added after this was written is reported rather than passed silently.
fn assert_all(
    spec: &CheckSpec,
    binding: &Binding,
    output: &SandboxOutput,
    scrubber: &Scrubber,
) -> CheckOutcome {
    let compiling = binding.mode == Mode::Compile;
    let stdout = String::from_utf8_lossy(output.stdout.as_ref());
    let mut problems = Vec::new();
    let mut wanted_exit = false;
    for expectation in &spec.expect {
        match expectation {
            Expectation::Exit(want) => {
                wanted_exit = true;
                if output.exit != *want {
                    problems.push(format!("exited {}, not {want}", output.exit));
                }
            }
            // Nothing ran, so there is no output to assert on.
            Expectation::Stdout(_) | Expectation::StdoutFile(_) if compiling => {}
            Expectation::Stdout(text) => {
                if !stdout.contains(text.as_str()) {
                    problems.push(format!("the output does not contain `{text}`"));
                }
            }
            Expectation::StdoutFile(path) => match binding.expected.get(path) {
                Some(bytes) => {
                    let want = String::from_utf8_lossy(bytes.as_ref());
                    if !contains_normalized(&stdout, &want) {
                        problems.push(format!(
                            "the output does not contain the contents of `{}`",
                            path.as_str()
                        ));
                    }
                }
                None => {
                    problems.push(format!(
                        "`expect-file` names `{}`, which was not read",
                        path.as_str()
                    ));
                }
            },
            Expectation::Status(_)
            | Expectation::JsonPath { .. }
            | Expectation::Header { .. }
            | Expectation::ResponseSchema { .. }
            | Expectation::Equals(_)
            | Expectation::Tolerance(_) => {}
            other => problems.push(format!("this runner does not evaluate {other:?}")),
        }
    }
    // A block that declared no `exit` still fails when the command does:
    // `attrs` implies `exit=0`, and a spec built another way gets the same
    // reading rather than a pass on a command that errored.
    if !wanted_exit && output.exit != 0 {
        problems.push(format!("exited {}, not 0", output.exit));
    }
    if problems.is_empty() {
        return CheckOutcome::Pass;
    }
    let stderr = String::from_utf8_lossy(output.stderr.as_ref());
    let mut excerpt = problems.join("\n");
    if !stderr.trim().is_empty() {
        excerpt.push('\n');
        excerpt.push_str(stderr.trim_end());
    }
    fail(scrubber, excerpt)
}

/// Trailing whitespace is what a text editor leaves behind, not what a sample
/// is about; an `expect-file` that differs only there is a match.
fn contains_normalized(haystack: &str, needle: &str) -> bool {
    normalize(haystack).contains(&normalize(needle))
}

fn normalize(text: &str) -> String {
    text.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_matches('\n')
        .to_owned()
}

#[cfg(test)]
mod tests;
