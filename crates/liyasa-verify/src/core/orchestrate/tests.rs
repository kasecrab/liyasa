//! These tests build a page and let the orchestrator find the checks in it.
//!
//! That is the whole point. Every existing runner test constructs the
//! `CheckSpec` it then runs, which is the family the fleet named on
//! 2026-09-18: *the test constructs the thing under test itself, rather than
//! obtaining it the way production does*. A suite of those is exactly
//! compatible with nothing ever calling the runners, which is how this gap
//! survived to become one of the two largest in the ledger.

use std::sync::Mutex;

use liyasa_core::conformance::block_on;
use liyasa_core::conformance::fixtures::MemoryVfs;
use liyasa_core::document::{Block, BlockKind, FenceAttrs, Inline, Node, Origin};
use liyasa_core::ids::BlockId;
use liyasa_core::span::{SourceId, Span};
use liyasa_core::verify::SandboxError;
use liyasa_core::vfs::Vfs;

use super::*;
use crate::core::config::{VerifyConfig, VerifyDefault};
use crate::core::runners::testing::{NoSandbox, Secrets};

fn span() -> Span {
    Span::new(SourceId(0), 0, 0)
}

fn fence(id: &str, lang: &str, attrs: &[(&str, &str)], flags: &[&str], body: &str) -> Block {
    let mut kv = std::collections::BTreeMap::new();
    for (k, v) in attrs {
        kv.insert((*k).to_owned(), (*v).to_owned());
    }
    Block {
        id: BlockId::explicit(id),
        explicit_id: None,
        kind: BlockKind::CodeBlock {
            lang: Some(lang.to_owned()),
            attrs: FenceAttrs {
                flags: flags.iter().map(|f| (*f).to_owned()).collect(),
                kv,
                highlight: Vec::new(),
            },
            highlighted: None,
        },
        origin: Origin::at(span()),
        children: vec![Node::Inline(Inline::Text(body.to_owned()))],
    }
}

fn document(blocks: Vec<Block>) -> Block {
    Block {
        id: BlockId::explicit("doc"),
        explicit_id: None,
        kind: BlockKind::Document,
        origin: Origin::at(span()),
        children: blocks.into_iter().map(Node::Block).collect(),
    }
}

/// A paragraph, so the walk has something it must not pick up.
fn prose(text: &str) -> Block {
    Block {
        id: BlockId::explicit("p"),
        explicit_id: None,
        kind: BlockKind::Paragraph,
        origin: Origin::at(span()),
        children: vec![Node::Inline(Inline::Text(text.to_owned()))],
    }
}

fn run(root: &Block, config: &VerifyConfig) -> Run {
    verify_page(root, config, &NoSandbox, &MemoryVfs::new())
}

/// A config whose `shell` image is pinned, so the runner builds a job.
///
/// VER-03 refuses to run an unpinned image (`E0610`) rather than run against
/// whatever `latest` is today, so a default config never reaches a
/// `SandboxJob` — and a test that asserted on the job without pinning would
/// pass on an error it had not noticed.
fn pinned() -> VerifyConfig {
    let mut config = VerifyConfig::default();
    config.runners.images.insert(
        "shell".to_owned(),
        "busybox@sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .to_owned(),
    );
    config
}

/// The same, with a budget the caller chooses.
fn with_budget(root: &Block, mut budget: Budget) -> Run {
    let config = VerifyConfig::default();
    let orchestrator = Orchestrator {
        config: &config,
        sandbox: &NoSandbox,
        secrets: &Secrets::default(),
        vfs: &MemoryVfs::new(),
    };
    let page = Page {
        route: Route::new("/api"),
        root,
    };
    block_on(orchestrator.verify(std::slice::from_ref(&page), &mut budget))
}

/// Plans and runs one page. `verify` is the entry point that builds the
/// registry from the plan's bindings, which is the whole point of defect 154 —
/// a caller that builds its own registry gets default bindings and silently
/// drops every `env=` on the page.
fn verify_page(root: &Block, config: &VerifyConfig, sandbox: &dyn Sandbox, vfs: &dyn Vfs) -> Run {
    let orchestrator = Orchestrator {
        config,
        sandbox,
        secrets: &Secrets::default(),
        vfs,
    };
    let page = Page {
        route: Route::new("/api/limits"),
        root,
    };
    let mut budget = Budget::unbounded();
    block_on(orchestrator.verify(std::slice::from_ref(&page), &mut budget))
}

#[test]
fn a_verified_block_in_a_page_runs_without_anyone_building_a_checkspec() {
    let root = document(vec![
        prose("The pattern below must compile."),
        fence(
            "b1",
            "regex",
            &[("expect", "2026-09-19")],
            &["verify"],
            r"^\d{4}-\d{2}-\d{2}$",
        ),
    ]);

    let out = run(&root, &VerifyConfig::default());

    assert_eq!(out.results.len(), 1, "{:?}", out.results);
    assert_eq!(out.passed(), 1, "{:?}", out.results[0].outcome);
    assert_eq!(
        out.results[0].id.as_str(),
        format!("/api/limits#{}#0", BlockId::explicit("b1").to_hex()),
        "the id is the one the report and the graph address"
    );
}

#[test]
fn a_failing_block_fails_the_run_rather_than_being_absent_from_it() {
    let root = document(vec![fence(
        "b1",
        "regex",
        &[("expect", "not-a-date")],
        &["verify"],
        r"^\d{4}-\d{2}-\d{2}$",
    )]);

    let out = run(&root, &VerifyConfig::default());

    assert_eq!(out.results.len(), 1);
    assert_eq!(out.failed(), 1, "{:?}", out.results[0].outcome);
}

#[test]
fn prose_and_unmarked_fences_are_not_checks() {
    let root = document(vec![
        prose("A paragraph mentioning regex."),
        fence("b1", "regex", &[], &[], r"^ok$"),
    ]);

    let out = run(&root, &VerifyConfig::default());

    assert!(out.results.is_empty(), "{:?}", out.results);
    assert!(out.problems.is_empty(), "{:?}", out.problems);
}

#[test]
fn the_walk_reaches_a_fence_nested_inside_another_block() {
    let inner = fence("b1", "regex", &[], &["verify"], r"^ok$");
    let container = Block {
        id: BlockId::explicit("note"),
        explicit_id: None,
        kind: BlockKind::BlockQuote,
        origin: Origin::at(span()),
        children: vec![Node::Block(inner)],
    };

    let out = run(&document(vec![container]), &VerifyConfig::default());

    assert_eq!(
        out.results.len(),
        1,
        "a fence in a container is still a check"
    );
}

/// The heart of RFC 1308 §2: the block is reported, not dropped.
#[test]
fn a_language_no_runner_claims_is_reported_rather_than_silently_skipped() {
    let root = document(vec![fence(
        "b1",
        "brainfuck",
        &[],
        &["verify"],
        "++++[>++++<-]>.",
    )]);

    let out = run(&root, &VerifyConfig::default());

    assert_eq!(out.results.len(), 1, "the block must appear in the report");
    let CheckOutcome::Skip { reason } = &out.results[0].outcome else {
        panic!(
            "a missing runner is a skip, not a pass: {:?}",
            out.results[0]
        );
    };
    assert!(reason.contains("brainfuck"), "{reason}");
    assert_eq!(out.problems.as_slice()[0].code, code::E0602);
}

/// The same condition with nobody asking. Under `default: "all"` a language no
/// runner claims is not a request, so there is nothing to report — which is
/// what stops the previous test's rule from filling a report with noise.
#[test]
fn an_unclaimed_language_under_default_all_is_not_a_request() {
    let config = VerifyConfig {
        default: VerifyDefault::All,
        ..VerifyConfig::default()
    };
    let root = document(vec![fence("b1", "brainfuck", &[], &[], "++++.")]);

    let out = run(&root, &config);

    assert!(out.results.is_empty(), "{:?}", out.results);
    assert!(out.problems.is_empty(), "{:?}", out.problems);
}

#[test]
fn a_block_marked_verify_skip_is_reported_as_a_skip_with_its_reason() {
    let root = document(vec![fence(
        "b1",
        "regex",
        &[("verify", "skip"), ("reason", "needs a live tenant")],
        &[],
        r"^ok$",
    )]);

    let out = run(&root, &VerifyConfig::default());

    assert_eq!(out.results.len(), 1);
    let CheckOutcome::Skip { reason } = &out.results[0].outcome else {
        panic!("{:?}", out.results[0].outcome);
    };
    assert!(reason.contains("needs a live tenant"), "{reason}");
}

/// RFC 1308 §3. A spent budget must not make checks disappear.
#[test]
fn a_spent_budget_reports_every_check_it_did_not_start() {
    let root = document(vec![
        fence("b1", "regex", &[], &["verify"], r"^ok$"),
        fence("b2", "regex", &[], &["verify"], r"^ok$"),
    ]);
    // Already spent before the first check.
    let out = with_budget(&root, Budget::of(Duration::ZERO));

    assert_eq!(out.results.len(), 2, "both blocks are still in the report");
    assert_eq!(out.skipped(), 2);
    assert_eq!(out.passed(), 0);
    assert_eq!(
        out.problems.as_slice()[0].code,
        code::W0622,
        "and the run says the budget is why"
    );
    assert!(
        out.problems.as_slice()[0].message.contains('2'),
        "naming how many did not start: {}",
        out.problems.as_slice()[0].message
    );
}

#[test]
fn a_budget_that_is_not_spent_runs_everything_and_raises_nothing() {
    let root = document(vec![
        fence("b1", "regex", &[], &["verify"], r"^ok$"),
        fence("b2", "regex", &[], &["verify"], r"^ok$"),
    ]);
    let out = with_budget(&root, Budget::of(Duration::from_secs(60)));

    assert_eq!(out.passed(), 2, "{:?}", out.results);
    assert!(out.problems.is_empty(), "{:?}", out.problems);
}

#[test]
fn every_check_on_a_page_gets_its_own_ordinal() {
    let root = document(vec![
        fence("b1", "regex", &[], &["verify"], r"^a$"),
        fence("b2", "regex", &[], &["verify"], r"^b$"),
    ]);

    let out = run(&root, &VerifyConfig::default());

    let ids: Vec<&str> = out.results.iter().map(|r| r.id.as_str()).collect();
    assert!(ids[0].ends_with("#0"), "{ids:?}");
    assert!(ids[1].ends_with("#1"), "{ids:?}");
    assert_ne!(ids[0], ids[1], "two checks must not share an id");
}

// ---- the packet's actual deliverable ----
//
// Everything above uses the in-process registry. The reason this component was
// assigned is that WP-21's SANDBOXED runners were complete, tested, and had
// never run against a real page. Asserting they are "now reachable" because
// `Orchestrator` takes a `&Registry` would be reasoning, not measuring. These
// two measure it.

#[test]
fn a_sandboxed_runner_is_reached_from_a_page_like_any_other() {
    let config = VerifyConfig::default();
    let (registry, problems) = crate::runners::sandboxed(&config);
    assert!(problems.is_empty(), "{problems:?}");
    assert!(
        registry.for_language("python").is_some(),
        "the sandboxed registry claims python: {registry:?}"
    );

    let root = document(vec![fence(
        "b1",
        "python",
        &[("expect", "600")],
        &["verify"],
        "print(600)",
    )]);
    let out = verify_page(&root, &config, &NoSandbox, &MemoryVfs::new());

    // There is no container runtime here, so the check cannot pass. What is
    // being measured is that the walk REACHED the runner: before this module
    // existed, nothing on any path could produce a result for this block at
    // all.
    assert_eq!(out.results.len(), 1, "the block produced a check");
    assert!(
        !matches!(out.results[0].outcome, CheckOutcome::Skip { .. }),
        "reaching the runner and being refused by the sandbox is not a skip: {:?}",
        out.results[0].outcome
    );
}

/// The complement, and the one that would catch the walk silently doing
/// nothing: with no sandbox the run must report a failure, not an empty report.
#[test]
fn a_sandboxed_check_with_no_sandbox_reports_rather_than_disappearing() {
    let config = VerifyConfig::default();
    let root = document(vec![fence("b1", "shell", &[], &["verify"], "echo hi")]);
    let out = verify_page(&root, &config, &NoSandbox, &MemoryVfs::new());

    assert_eq!(out.results.len(), 1);
    assert_eq!(
        out.failed(),
        1,
        "a sandbox that cannot run must fail the check: {:?}",
        out.results[0].outcome
    );
}

/// VER-05: a declared chain shares one sandbox. This walk does not call
/// `chain::run` yet, and running the steps independently would answer a
/// different question — step two without step one's side effects. So the block
/// is reported as not run, which is the honest answer, rather than run wrongly.
#[test]
fn a_declared_chain_is_reported_rather_than_run_as_independent_steps() {
    let root = document(vec![fence(
        "b1",
        "shell",
        &[],
        &["verify", "verify-chain"],
        "export TOKEN=1",
    )]);

    let out = run(&root, &VerifyConfig::default());

    assert_eq!(
        out.results.len(),
        1,
        "the block still appears in the report"
    );
    let CheckOutcome::Skip { reason } = &out.results[0].outcome else {
        panic!(
            "a chain this run cannot honour must not report a result as if it had: {:?}",
            out.results[0].outcome
        );
    };
    assert!(reason.contains("verify-chain"), "{reason}");
}

// ---- defect 154: the walk dropped `env=`, `setup=` and `fixture=` ----
//
// `CheckSpec` is frozen and carries none of them (RFC 2103), so they travel in
// a `code::Bindings` table keyed by `CheckId`. The walk built specs and never
// built that table, and `runners/code.rs:178` reads
//
//     self.bindings.get(&spec.id).unwrap_or(&default)
//
// so a missing binding is indistinguishable from an empty one: the job ran with
// no environment and the check reported pass or fail as if it had one.
//
// These assert on the SandboxJob the runner builds, which is where `binding.env`
// lands (code.rs:254). That is the real path, and it needs no container.

/// Records the job it is handed and refuses to run it. Refusing is fine: what
/// is under test is what the runner PUT in the job, not what a container would
/// do with it.
#[derive(Default)]
struct Recorder {
    jobs: Mutex<Vec<liyasa_core::verify::SandboxJob>>,
}

impl Sandbox for Recorder {
    fn exec<'a>(
        &'a self,
        job: liyasa_core::verify::SandboxJob,
    ) -> liyasa_core::net::BoxFut<'a, Result<liyasa_core::verify::SandboxOutput, SandboxError>>
    {
        self.jobs.lock().expect("lock").push(job);
        Box::pin(std::future::ready(Err(SandboxError::Unavailable)))
    }
}

impl Recorder {
    fn only_job(&self) -> liyasa_core::verify::SandboxJob {
        let jobs = self.jobs.lock().expect("lock");
        assert_eq!(jobs.len(), 1, "exactly one job should have been built");
        jobs[0].clone()
    }
}

#[test]
fn an_env_attribute_on_a_fence_reaches_the_sandbox_job() {
    let root = document(vec![fence(
        "b1",
        "shell",
        &[("env", "TOKEN=abc")],
        &["verify"],
        "test -n \"$TOKEN\"",
    )]);
    let recorder = Recorder::default();
    let config = pinned();

    let out = verify_page(&root, &config, &recorder, &MemoryVfs::new());

    assert_eq!(out.results.len(), 1, "{:?}", out.results);
    let job = recorder.only_job();
    assert!(
        job.env.iter().any(|(k, v)| k == "TOKEN" && v == "abc"),
        "the declared environment must reach the job; before defect 154 was \
         fixed this was empty and the check still reported a result: {:?}",
        job.env
    );
}

#[test]
fn a_fence_declaring_no_environment_still_gets_an_empty_one() {
    let root = document(vec![fence("b1", "shell", &[], &["verify"], "echo hi")]);
    let recorder = Recorder::default();
    let config = pinned();

    let out = verify_page(&root, &config, &recorder, &MemoryVfs::new());

    assert_eq!(out.results.len(), 1);
    assert!(
        recorder.only_job().env.is_empty(),
        "no declaration means no environment, which must stay distinguishable \
         from a dropped one only by the author's intent"
    );
}

/// `setup="login"` is `snippets/login.md` (RFC 2105). Extracting its code needs
/// a Markdown parse this crate has no dependency for, so the check is reported
/// rather than run without its setup — RFC 2105 says that is the right answer
/// for an unresolvable setup permanently, not only until the wiring lands.
#[test]
fn a_declared_setup_is_reported_rather_than_run_without_it() {
    let root = document(vec![fence(
        "b1",
        "shell",
        &[("setup", "login")],
        &["verify"],
        "whoami",
    )]);
    let recorder = Recorder::default();
    let config = VerifyConfig::default();

    let out = verify_page(&root, &config, &recorder, &MemoryVfs::new());

    assert_eq!(
        out.results.len(),
        1,
        "the block still appears in the report"
    );
    let CheckOutcome::Skip { reason } = &out.results[0].outcome else {
        panic!(
            "a setup that cannot be honoured must not produce a run result: {:?}",
            out.results[0].outcome
        );
    };
    assert!(
        reason.contains("login"),
        "the reason names the snippet: {reason}"
    );
    assert!(
        recorder.jobs.lock().expect("lock").is_empty(),
        "and nothing was sent to the sandbox"
    );
}

#[test]
fn a_fixture_attribute_is_resolved_through_the_vfs_and_staged() {
    let root = document(vec![fence(
        "b1",
        "shell",
        &[("fixture", "data/input.csv")],
        &["verify"],
        "cat data/input.csv",
    )]);
    let vfs = MemoryVfs::new().with("data/input.csv", "id,name\n1,alice\n");
    let recorder = Recorder::default();
    let config = pinned();

    let out = verify_page(&root, &config, &recorder, &vfs);

    assert_eq!(out.results.len(), 1, "{:?}", out.results);
    let job = recorder.only_job();
    assert!(
        job.files
            .iter()
            .any(|(path, bytes)| path.as_str().ends_with("input.csv")
                && String::from_utf8_lossy(bytes).contains("alice")),
        "the fixture must be staged beside the sample: {:?}",
        job.files
            .iter()
            .map(|(p, _)| p.to_string())
            .collect::<Vec<_>>()
    );
}

/// A declaration that cannot be resolved must not become a silent default —
/// that is the defect one layer down.
#[test]
fn a_fixture_the_vfs_cannot_supply_is_reported_rather_than_dropped() {
    let root = document(vec![fence(
        "b1",
        "shell",
        &[("fixture", "data/missing.csv")],
        &["verify"],
        "cat data/missing.csv",
    )]);
    let recorder = Recorder::default();
    let config = VerifyConfig::default();

    let out = verify_page(&root, &config, &recorder, &MemoryVfs::new());

    assert_eq!(
        out.results.len(),
        1,
        "the block still appears in the report"
    );
    assert!(
        !matches!(out.results[0].outcome, CheckOutcome::Pass),
        "a check whose fixture is missing must not pass: {:?}",
        out.results[0].outcome
    );
    assert!(
        out.problems
            .iter()
            .any(|d| d.message.contains("missing.csv")),
        "and the run must name the file: {:?}",
        out.problems
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
    );
}
