//! These tests build a page and let the orchestrator find the checks in it.
//!
//! That is the whole point. Every existing runner test constructs the
//! `CheckSpec` it then runs, which is the family the fleet named on
//! 2026-09-18: *the test constructs the thing under test itself, rather than
//! obtaining it the way production does*. A suite of those is exactly
//! compatible with nothing ever calling the runners, which is how this gap
//! survived to become one of the two largest in the ledger.

use liyasa_core::conformance::block_on;
use liyasa_core::document::{Block, BlockKind, FenceAttrs, Inline, Node, Origin};
use liyasa_core::ids::BlockId;
use liyasa_core::span::{SourceId, Span};

use super::*;
use crate::core::config::{VerifyConfig, VerifyDefault};
use crate::core::runners::in_process;
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
    let registry = in_process();
    let orchestrator = Orchestrator {
        registry: &registry,
        config,
        sandbox: &NoSandbox,
        secrets: &Secrets::default(),
    };
    let page = Page {
        route: Route::new("/api/limits"),
        root,
    };
    let mut budget = Budget::unbounded();
    block_on(orchestrator.page(&page, &mut budget))
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
    let registry = in_process();
    let config = VerifyConfig::default();
    let orchestrator = Orchestrator {
        registry: &registry,
        config: &config,
        sandbox: &NoSandbox,
        secrets: &Secrets::default(),
    };
    let page = Page {
        route: Route::new("/api"),
        root: &root,
    };
    // Already spent before the first check.
    let mut budget = Budget::of(Duration::ZERO);

    let out = block_on(orchestrator.site(std::slice::from_ref(&page), &mut budget));

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
    let registry = in_process();
    let config = VerifyConfig::default();
    let orchestrator = Orchestrator {
        registry: &registry,
        config: &config,
        sandbox: &NoSandbox,
        secrets: &Secrets::default(),
    };
    let page = Page {
        route: Route::new("/api"),
        root: &root,
    };
    let mut budget = Budget::of(Duration::from_secs(60));

    let out = block_on(orchestrator.site(std::slice::from_ref(&page), &mut budget));

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
    let orchestrator = Orchestrator {
        registry: &registry,
        config: &config,
        sandbox: &NoSandbox,
        secrets: &Secrets::default(),
    };
    let page = Page {
        route: Route::new("/api/limits"),
        root: &root,
    };
    let mut budget = Budget::unbounded();

    let out = block_on(orchestrator.page(&page, &mut budget));

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
    let (registry, _) = crate::runners::sandboxed(&config);
    let root = document(vec![fence("b1", "shell", &[], &["verify"], "echo hi")]);
    let orchestrator = Orchestrator {
        registry: &registry,
        config: &config,
        sandbox: &NoSandbox,
        secrets: &Secrets::default(),
    };
    let page = Page {
        route: Route::new("/guide"),
        root: &root,
    };
    let mut budget = Budget::unbounded();

    let out = block_on(orchestrator.page(&page, &mut budget));

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
