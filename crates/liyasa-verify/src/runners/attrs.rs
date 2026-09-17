//! The `verify` fence attributes (VER-01).
//!
//! A fence declares what running it means: which runner mode, what its output
//! must contain, what it may exit with, how long it gets, what environment and
//! fixtures it needs, and which hidden block runs first. `verify.default:
//! "all"` flips the opening question from "did the author ask for this block
//! to run" to "did the author ask for it not to".
//!
//! Every attribute Liyasa cannot read is `E0609` on that attribute alone and
//! the rest of the block still runs, for the reason `core::config` reads the
//! `verify` object one key at a time: one malformed value should not quietly
//! turn a verified block into an unverified one.

use std::time::Duration;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::document::FenceAttrs;
use liyasa_core::ids::{BlockId, CheckId, Route};
use liyasa_core::verify::{CheckInput, CheckSpec, Expectation};
use liyasa_core::vfs::VfsPath;

use crate::core::config::{VerifyConfig, VerifyDefault};
use crate::core::duration::DurationSetting;
use crate::core::policy::{Skip, block_skip};

/// VER-01's default when a block names no `timeout`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// What `verify` on the fence asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Mode {
    /// `verify`: run it with the language's default runner.
    #[default]
    Run,
    /// `verify=compile`: compile only, do not execute.
    Compile,
    /// `verify=skip`: a documented exclusion.
    Skip(Skip),
}

/// One block's verification, read off its fence.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockVerify {
    pub mode: Mode,
    pub expect: Vec<Expectation>,
    pub timeout: Duration,
    /// The same value, or `None` when the fence named no `timeout=` and this
    /// is the site default. A runner's own declared default (VER-02) applies
    /// only in that case.
    pub declared_timeout: Option<Duration>,
    pub env: Vec<(String, String)>,
    /// `setup="snippet-name"`: a hidden block run in the same sandbox first.
    pub setup: Option<String>,
    pub fixtures: Vec<VfsPath>,
    /// `verify-chain` on a `steps` block (VER-05).
    pub chain: bool,
}

impl BlockVerify {
    fn new(mode: Mode, timeout: Duration) -> Self {
        Self {
            mode,
            expect: Vec::new(),
            timeout,
            declared_timeout: None,
            env: Vec::new(),
            setup: None,
            fixtures: Vec::new(),
            chain: false,
        }
    }

    /// The `CheckSpec` a runner is handed. `hidden_lines` comes from the
    /// split the renderer already did (VER-04); a caller that has not split
    /// passes an empty list and the runner splits on its way to the sandbox.
    pub fn spec(&self, at: &Site<'_>, source: &str, hidden_lines: Vec<u32>) -> CheckSpec {
        CheckSpec {
            id: check_id(at.page, &at.block, at.nth),
            page: at.page.clone(),
            block: at.block,
            runner: at.runner.to_owned(),
            input: CheckInput::Code {
                lang: at.lang.to_owned(),
                source: source.to_owned(),
                hidden_lines,
            },
            expect: self.expect.clone(),
            timeout: self.timeout,
            needs_network: false,
            needs_secrets: Vec::new(),
        }
    }
}

/// Where a check sits. `nth` is its ordinal on the block, which `CheckId`
/// spells `<route>#<block>#<n>`: one block yields several checks when a chain
/// steps or a target runs twice.
pub struct Site<'a> {
    pub page: &'a Route,
    pub block: BlockId,
    pub nth: u32,
    pub runner: &'a str,
    pub lang: &'a str,
}

/// §34.9's `<page route>#<block id>#<n>`.
pub fn check_id(page: &Route, block: &BlockId, nth: u32) -> CheckId {
    CheckId::new(format!("{}#{}#{nth}", page.as_str(), block.to_hex()))
}

/// Reads a fence. `None` means the block is not verified at all: no `verify`
/// attribute under `default: "tagged"`, or a language no runner claims under
/// `default: "all"`.
///
/// `claimed` is whether some runner in the registry offers this language; it
/// is the caller's, because the registry is assembled from config.
pub fn read(
    attrs: &FenceAttrs,
    config: &VerifyConfig,
    claimed: bool,
) -> (Option<BlockVerify>, Vec<Diagnostic>) {
    let mut problems = Vec::new();
    let Some(mode) = mode(attrs, config, claimed, &mut problems) else {
        return (None, problems);
    };
    let mut out = BlockVerify::new(mode, default_timeout(config));

    if let Mode::Skip(_) = out.mode {
        // A skipped block's other attributes are inert; reading them would
        // only produce diagnostics about a block that is not going to run.
        return (Some(out), problems);
    }

    if let Some(text) = value(attrs, "expect") {
        out.expect.push(Expectation::Stdout(text.to_owned()));
    }
    if let Some(text) = value(attrs, "expect-file") {
        out.expect.push(Expectation::StdoutFile(VfsPath::new(text)));
    }
    match value(attrs, "exit") {
        Some(text) => match text.trim().parse::<i32>() {
            Ok(exit) => out.expect.push(Expectation::Exit(exit)),
            Err(_) => problems.push(bad("exit", text, "a whole number")),
        },
        // A block that says nothing about its exit status still fails when the
        // command does; VER-01 spells the attribute `exit=0` because 0 is what
        // it would have been anyway.
        None => out.expect.push(Expectation::Exit(0)),
    }
    if let Some(text) = value(attrs, "timeout") {
        match DurationSetting::parse(text) {
            Ok(setting) => {
                out.timeout = setting.as_duration();
                out.declared_timeout = Some(out.timeout);
            }
            Err(error) => problems.push(bad("timeout", text, &error.to_string())),
        }
    }
    if let Some(text) = value(attrs, "env") {
        let (pairs, mut found) = env(text);
        out.env = pairs;
        problems.append(&mut found);
    }
    out.setup = value(attrs, "setup")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    if let Some(text) = value(attrs, "fixture") {
        out.fixtures = list(text).into_iter().map(VfsPath::new).collect();
    }
    out.chain = attrs.flags.contains("verify-chain")
        || attrs.kv.get("verify-chain").map(String::as_str) == Some("true");

    (Some(out), problems)
}

fn mode(
    attrs: &FenceAttrs,
    config: &VerifyConfig,
    claimed: bool,
    problems: &mut Vec<Diagnostic>,
) -> Option<Mode> {
    if let Some(skip) = block_skip(attrs) {
        return Some(Mode::Skip(skip));
    }
    match attrs.kv.get("verify").map(String::as_str) {
        Some("compile") => Some(Mode::Compile),
        Some("run" | "true" | "") => Some(Mode::Run),
        Some("false") => None,
        Some(other) => {
            problems.push(bad("verify", other, "`compile`, `skip`, or no value"));
            Some(Mode::Run)
        }
        None if attrs.flags.contains("verify") => Some(Mode::Run),
        // Untagged. `default: "all"` runs it if some runner claims the
        // language; VER-02's unknown-language warning covers the rest.
        None => match config.default {
            VerifyDefault::All if claimed => Some(Mode::Run),
            _ => None,
        },
    }
}

fn default_timeout(config: &VerifyConfig) -> Duration {
    config
        .budget
        .per_check
        .map_or(DEFAULT_TIMEOUT, DurationSetting::as_duration)
}

/// `env="KEY=value"`, and a comma-separated list of them, because a fence's
/// attributes are a map and a block that needs two variables has nowhere else
/// to put the second (RFC 2101).
fn env(text: &str) -> (Vec<(String, String)>, Vec<Diagnostic>) {
    let mut pairs = Vec::new();
    let mut problems = Vec::new();
    for item in list(text) {
        match item.split_once('=') {
            Some((name, value)) if !name.trim().is_empty() => {
                pairs.push((name.trim().to_owned(), value.to_owned()));
            }
            _ => problems.push(bad("env", &item, "`KEY=value`")),
        }
    }
    (pairs, problems)
}

fn list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

fn value<'a>(attrs: &'a FenceAttrs, key: &str) -> Option<&'a str> {
    attrs.kv.get(key).map(String::as_str)
}

fn bad(attribute: &str, found: &str, wanted: &str) -> Diagnostic {
    Diagnostic::new(
        code::E0609,
        format!("the fence attribute `{attribute}={found}` is not {wanted}"),
    )
    .help("the attribute is ignored and the rest of the block still runs")
}

#[cfg(test)]
mod tests;
