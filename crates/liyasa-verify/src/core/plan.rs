//! Phase one of a verification run: what the walk found, before anything ran
//! (RFC 1309, defect 154).
//!
//! `CheckSpec` is frozen and carries no `env`, `setup` or `fixture`, so VER-01's
//! three attributes travel in a [`Bindings`] table keyed by `CheckId` and are
//! read by the runner out of its own state (RFC 2103). `Runner::run` is frozen
//! too and takes no bindings parameter, so the table cannot be passed per call:
//! it is registry state, built by `runners::sandboxed_with`, and a registry can
//! only be built once the walk knows every binding.
//!
//! That is what forces two phases rather than one, and it is a consequence of
//! the contracts rather than a preference. Planning is separated from running so
//! the table exists before the registry does.
//!
//! The defect this closes was silent in a specific way worth recording, because
//! it is the reason the fix has to live here. `runners/code.rs` reads
//!
//! ```text
//! let binding = self.bindings.get(&spec.id).unwrap_or(&default);
//! ```
//!
//! so a missing binding is indistinguishable from an empty one. At that line the
//! information needed to tell them apart no longer exists, which is why no
//! amount of care in the runner could have caught it.

use std::collections::BTreeMap;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, Severity, code};
use liyasa_core::document::{Block, BlockKind, FenceAttrs};
use liyasa_core::verify::{CheckInput, CheckOutcome, CheckSpec, Expectation};
use liyasa_core::vfs::{Vfs, VfsPath};

use crate::core::config::VerifyConfig;
use crate::core::runners::{Registry, no_runner};
use crate::runners::attrs::{self, Mode, Site};
use crate::runners::code::{Binding, Bindings};

/// One check the walk found.
pub struct Planned {
    pub spec: CheckSpec,
    /// The language the fence declared, for the runner lookup.
    pub lang: String,
    /// Set when the walk decided the outcome without running anything: a
    /// documented `verify=skip`, a language no runner claims, a declaration it
    /// could not honour. `None` means run it.
    pub settled: Option<CheckOutcome>,
}

/// Everything phase two needs, and nothing that has run.
#[derive(Default)]
pub struct Plan {
    pub checks: Vec<Planned>,
    /// What `runners::sandboxed_with` must be given, so VER-01's `env=`,
    /// `setup=` and `fixture=` reach the job.
    pub bindings: Bindings,
    pub problems: Diagnostics,
}

impl Plan {
    /// How many checks phase two will actually run.
    pub fn runnable(&self) -> usize {
        self.checks.iter().filter(|c| c.settled.is_none()).count()
    }
}

/// Walks every page into ONE plan.
///
/// One plan rather than one per page because [`Bindings`] is a table the whole
/// registry is built from and exposes no iterator to merge two of them — so the
/// walk accumulates into a single table instead of producing tables that would
/// have to be combined.
///
/// `probe` answers only "does some runner claim this language", which decides
/// whether an untagged block under `default: "all"` is a request at all. It
/// needs no bindings, which is why it can exist before the table does.
pub fn site(
    pages: &[super::orchestrate::Page<'_>],
    config: &VerifyConfig,
    probe: &Registry,
    vfs: &dyn Vfs,
) -> Plan {
    let mut out = Plan::default();
    for page in pages {
        for (nth, block) in code_blocks(page.root).into_iter().enumerate() {
            plan_block(&mut out, page, block, nth as u32, config, probe, vfs);
        }
    }
    out
}

/// One page, for a caller driving this per page from a job queue (RFC 1404).
pub fn page(
    page: &super::orchestrate::Page<'_>,
    config: &VerifyConfig,
    probe: &Registry,
    vfs: &dyn Vfs,
) -> Plan {
    site(std::slice::from_ref(page), config, probe, vfs)
}

fn plan_block(
    out: &mut Plan,
    at_page: &super::orchestrate::Page<'_>,
    block: &Block,
    nth: u32,
    config: &VerifyConfig,
    probe: &Registry,
    vfs: &dyn Vfs,
) {
    let BlockKind::CodeBlock { lang, attrs, .. } = &block.kind else {
        return;
    };
    let lang = lang.as_deref().unwrap_or_default();
    let claimed = probe.for_language(lang).is_some();
    let (verify, problems) = attrs::read(attrs, config, claimed);
    out.problems.extend(problems);

    // No `verify` under `tagged`, or an unclaimed language under `all`: nobody
    // asked, so there is nothing to report.
    let Some(verify) = verify else {
        return;
    };

    let site = Site {
        page: &at_page.route,
        block: block.id,
        nth,
        runner: probe.for_language(lang).map_or(lang, |r| r.id()),
        lang,
    };
    let spec = verify.spec(&site, &source_of(block), Vec::new());

    // Everything from here reports the block. The author asked for it to run,
    // so a report that omits it is the defect this module closes.
    if let Mode::Skip(skip) = &verify.mode {
        out.push(
            spec,
            lang,
            Some(CheckOutcome::Skip {
                reason: skip.reason(),
            }),
        );
        return;
    }

    if verify.chain {
        out.push(
            spec,
            lang,
            Some(CheckOutcome::Skip {
                reason: CHAIN_REASON.to_owned(),
            }),
        );
        return;
    }

    if probe.for_language(lang).is_none() {
        // VER-02: "unknown languages are skipped with a warning". `E0602` is
        // registered at error severity because a failing code check is an
        // error, and `verify.policy` grades failures — a skip is not one, so
        // the severity is set here rather than taken from the code's default
        // (RFC 1309 §2).
        out.problems
            .push(no_runner(lang).with_severity(Severity::Warning));
        out.push(
            spec,
            lang,
            Some(CheckOutcome::Skip {
                reason: format!("no runner claims the language `{lang}`"),
            }),
        );
        return;
    }

    // VER-01's three attributes. `Binding::from_block` takes the half that
    // needs no filesystem; the rest is resolved here.
    let mut binding = Binding::from_block(&verify, kv_of(attrs));
    let mut unresolved = Vec::new();

    for path in &verify.fixtures {
        match read_bytes(vfs, path) {
            Some(bytes) => binding.fixtures.push((path.clone(), bytes)),
            None => unresolved.push(unreadable("fixture", path)),
        }
    }
    for path in expect_files(&spec) {
        match read_bytes(vfs, &path) {
            Some(bytes) => {
                binding.expected.insert(path, bytes);
            }
            None => unresolved.push(unreadable("expect-file", &path)),
        }
    }
    // `setup="login"` means `snippets/login.md` (RFC 2105; the same mapping
    // `liyasa-markdown` uses at `filters.rs:470` and `expand.rs:887`, and the
    // reason VER-01 calls it *hidden* — a block with an `{#id}` on the page is
    // visible, which is what the attribute exists to avoid).
    //
    // TODO(rfc-1309): what is NOT settled is who parses it. The setup is the
    // source of the snippet's fenced code blocks in the language the runner
    // claims, and this crate has no Markdown parser — `liyasa-verify` does not
    // depend on `liyasa-markdown`. The three ways out are a new crate edge, a
    // second fence scanner here, or a resolver the caller fills, and each is a
    // different bet; a second scanner in particular is how a project ends up
    // with two Markdown parsers that disagree. Until that is decided a declared
    // setup is REPORTED rather than run without it, which RFC 2105 says is the
    // right behaviour for an unresolvable setup permanently and not only now.
    if let Some(name) = &verify.setup {
        unresolved.push(setup_unresolved(name));
    }

    if !unresolved.is_empty() {
        let reason = unresolved
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("; ");
        out.problems.extend(unresolved);
        out.push(spec, lang, Some(CheckOutcome::Skip { reason }));
        return;
    }

    out.bindings.set(spec.id.clone(), binding);
    out.push(spec, lang, None);
}

impl Plan {
    fn push(&mut self, spec: CheckSpec, lang: &str, settled: Option<CheckOutcome>) {
        self.checks.push(Planned {
            spec,
            lang: lang.to_owned(),
            settled,
        });
    }
}

/// What a declared chain carries until `chain::run` is wired into the walk.
pub const CHAIN_REASON: &str = "`verify-chain` runs the steps in one sandbox, which this run cannot do yet; \
     running them independently would answer a different question";

fn read_bytes(vfs: &dyn Vfs, path: &VfsPath) -> Option<liyasa_core::vfs::Bytes> {
    vfs.read(path).ok()
}

fn unreadable(attribute: &str, path: &VfsPath) -> Diagnostic {
    Diagnostic::new(
        code::E0638,
        format!("`{attribute}` names `{path}`, which could not be read"),
    )
    .help("check the path is relative to the project root and the file is committed")
}

fn setup_unresolved(name: &str) -> Diagnostic {
    Diagnostic::new(
        code::E0638,
        format!("`setup` names the snippet `{name}`, which was not read"),
    )
    .help(
        "`setup=\"name\"` is `snippets/name.md` (RFC 2105); this run cannot extract \
         its code yet, so the check is reported rather than run without its setup — \
         running it would answer a different question from the one the fence asked",
    )
}

fn expect_files(spec: &CheckSpec) -> Vec<VfsPath> {
    spec.expect
        .iter()
        .filter_map(|e| match e {
            Expectation::StdoutFile(path) => Some(path.clone()),
            _ => None,
        })
        .collect()
}

fn kv_of(attrs: &FenceAttrs) -> BTreeMap<String, String> {
    attrs.kv.clone()
}

/// Every code block under a root, in document order.
fn code_blocks(root: &Block) -> Vec<&Block> {
    let mut out = Vec::new();
    collect(root, &mut out);
    out
}

fn collect<'a>(block: &'a Block, out: &mut Vec<&'a Block>) {
    if matches!(block.kind, BlockKind::CodeBlock { .. }) {
        out.push(block);
    }
    for child in &block.children {
        if let liyasa_core::document::Node::Block(inner) = child {
            collect(inner, out);
        }
    }
}

/// A fence's body, verbatim: leading whitespace is part of the program.
fn source_of(block: &Block) -> String {
    let mut out = String::new();
    for child in &block.children {
        if let liyasa_core::document::Node::Inline(liyasa_core::document::Inline::Text(text)) =
            child
        {
            out.push_str(text);
        }
    }
    out
}

/// The input shape a planned check carries, for a caller that needs the source
/// back without re-walking.
pub fn code_of(spec: &CheckSpec) -> Option<(&str, &str)> {
    match &spec.input {
        CheckInput::Code { lang, source, .. } => Some((lang, source)),
        _ => None,
    }
}
