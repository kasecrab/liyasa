//! Handing the rest of the rules to the Vale binary (VER-61).
//!
//! Liyasa runs the seven rule types VER-61 names; anything else parses into
//! [`RuleKind::Unsupported`](super::rule::RuleKind::Unsupported) and
//! [`Linter::delegated`](super::Linter::delegated) lists it. This module is
//! what a caller does with that list: build a [`SandboxJob`] that runs `vale
//! --output=JSON` in the companion runtime, and read the result back as
//! [`Finding`]s.
//!
//! The rule RFC 1305 is built around holds here too. A rule that did not run
//! must never look like a rule that ran and found nothing, so every way this
//! can go wrong — no companion runtime, a binary that failed, output that will
//! not parse — produces `W0636` naming the rules, and never an empty `Ok`.
//!
//! No I/O happens here. The caller reads the rule files through `Vfs` and
//! executes the job through `Sandbox`; this module only builds the one and
//! reads the other.

use std::time::Duration;

use liyasa_core::diagnostics::{Diagnostic, Severity, code};
use liyasa_core::ids::BlockId;
use liyasa_core::span::Span;
use liyasa_core::verify::{SandboxJob, SandboxOutput};
use liyasa_core::vfs::{Bytes, VfsPath};
use serde::Deserialize;

use super::{Delegated, Finding};
use crate::core::scrub::Scrubber;

/// Where the job's files go, which is what the generated `.vale.ini` points
/// `StylesPath` at.
const STYLES: &str = "styles";
const CONFIG: &str = ".vale.ini";

/// Vale exits 0 with no findings and 1 with findings; anything else means it
/// did not lint.
const LINTED: &[i32] = &[0, 1];

/// The companion runtime's Vale image (VER-76 `runners.images`).
#[derive(Debug, Clone)]
pub struct Companion {
    pub image: String,
    pub digest: String,
    pub timeout: Duration,
    pub cpu_millis: u32,
    pub mem_bytes: u64,
}

/// One document, with everything the binary needs to lint it.
#[derive(Debug, Clone)]
pub struct Request {
    /// The `.vale.ini` the run uses, as text.
    pub config: String,
    /// `(path under StylesPath, bytes)` for every rule file that must go in.
    /// The caller reads these through `Vfs`.
    pub styles: Vec<(VfsPath, Bytes)>,
    /// The path Vale reports findings against.
    pub path: String,
    pub text: String,
}

impl Companion {
    pub fn new(image: impl Into<String>, digest: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            digest: digest.into(),
            timeout: Duration::from_secs(30),
            cpu_millis: 2_000,
            mem_bytes: 512 * 1024 * 1024,
        }
    }

    pub fn job(&self, request: &Request) -> SandboxJob {
        let mut files = Vec::with_capacity(request.styles.len() + 2);
        files.push((
            VfsPath::new(CONFIG),
            Bytes::copy_from_slice(request.config.as_bytes()),
        ));
        for (path, bytes) in &request.styles {
            files.push((VfsPath::new(STYLES).join(path.as_str()), bytes.clone()));
        }
        files.push((
            VfsPath::new(&request.path),
            Bytes::copy_from_slice(request.text.as_bytes()),
        ));

        SandboxJob {
            image: self.image.clone(),
            digest: self.digest.clone(),
            cmd: vec![
                "vale".to_owned(),
                "--output=JSON".to_owned(),
                "--no-exit".to_owned(),
                format!("--config={CONFIG}"),
                request.path.clone(),
            ],
            files,
            env: Vec::new(),
            timeout: self.timeout,
            network: false,
            cpu_millis: self.cpu_millis,
            mem_bytes: self.mem_bytes,
        }
    }
}

/// Vale's JSON alert. Only the fields a finding needs are read.
#[derive(Debug, Deserialize)]
struct Alert {
    #[serde(rename = "Check")]
    check: String,
    #[serde(rename = "Message")]
    message: String,
    #[serde(rename = "Severity")]
    severity: String,
    #[serde(rename = "Line")]
    line: usize,
    #[serde(rename = "Span")]
    span: [usize; 2],
    #[serde(rename = "Match", default)]
    matched: String,
    #[serde(rename = "Link", default)]
    link: String,
}

/// Reads what the companion runtime printed.
///
/// `text` is the document that was linted, because Vale reports a line and a
/// character column and a [`Finding`] carries a byte offset.
pub fn findings(
    output: &SandboxOutput,
    text: &str,
    block: BlockId,
    span: Option<Span>,
    scrubber: &Scrubber,
) -> Result<Vec<Finding>, Box<Diagnostic>> {
    if !LINTED.contains(&output.exit) {
        return Err(failed(
            format!("the Vale binary exited {}", output.exit),
            &printed(output, scrubber),
        ));
    }
    let stdout = std::str::from_utf8(&output.stdout)
        .map_err(|_| failed("the Vale binary printed bytes that are not UTF-8", ""))?;
    let by_file: std::collections::BTreeMap<String, Vec<Alert>> = serde_json::from_str(stdout)
        .map_err(|error| {
            failed(
                format!("the Vale binary printed output Liyasa cannot read: {error}"),
                &scrubber.excerpt(stdout),
            )
        })?;

    Ok(by_file
        .into_values()
        .flatten()
        .map(|alert| Finding {
            rule: alert.check,
            severity: severity(&alert.severity),
            message: alert.message,
            block,
            span,
            at: offset(text, alert.line, alert.span[0]),
            found: alert.matched,
            link: (!alert.link.is_empty()).then_some(alert.link),
        })
        .collect())
}

/// `W0636` for rules that were never run, or `None` when there were none.
///
/// This is what a caller reports when there is no companion runtime: the
/// alternative — saying nothing — is a linter that looks like it passed.
pub fn not_run(rules: &[Delegated], reason: &str) -> Option<Diagnostic> {
    if rules.is_empty() {
        return None;
    }
    let names: Vec<&str> = rules.iter().map(|rule| rule.rule.as_str()).collect();
    Some(
        Diagnostic::new(
            code::W0636,
            format!(
                "{} prose {} did not run: {}",
                names.len(),
                if names.len() == 1 { "rule" } else { "rules" },
                names.join(", ")
            ),
        )
        .help(format!(
            "{reason}; these rule types are not among the seven Liyasa implements, \
             so they need the Vale binary in the companion runtime"
        )),
    )
}

fn failed(what: impl Into<String>, detail: &str) -> Box<Diagnostic> {
    let what = what.into();
    let message = if detail.is_empty() {
        what
    } else {
        format!("{what}: {detail}")
    };
    Box::new(Diagnostic::new(code::W0636, message).help(
        "the rules Liyasa cannot run were not checked; \
         fix the companion runtime or accept that they are unchecked",
    ))
}

/// Whatever the binary said, capped and with any known secret gone.
fn printed(output: &SandboxOutput, scrubber: &Scrubber) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = stderr.trim();
    if text.is_empty() {
        return String::new();
    }
    scrubber.excerpt(text)
}

fn severity(name: &str) -> Severity {
    match name {
        "error" => Severity::Error,
        "suggestion" => Severity::Hint,
        _ => Severity::Warning,
    }
}

/// Vale counts lines from 1 and columns in characters from 1. A position the
/// text does not have lands at the start rather than panicking: a wrong offset
/// in a diagnostic is a nuisance, a panic on a linter's output is a build that
/// dies on someone else's bug.
fn offset(text: &str, line: usize, column: usize) -> usize {
    let mut start = 0;
    for _ in 1..line.max(1) {
        match text[start..].find('\n') {
            Some(at) => start += at + 1,
            None => return 0,
        }
    }
    let rest = &text[start..];
    let end = rest.find('\n').unwrap_or(rest.len());
    match rest[..end].char_indices().nth(column.saturating_sub(1)) {
        Some((at, _)) => start + at,
        None if column == 1 => start,
        None => 0,
    }
}

#[cfg(test)]
mod tests;
