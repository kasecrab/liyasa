//! Fixtures the WP-25 acceptance tests share.
//!
//! One place rather than one per file: nine acceptance tests need the same
//! project — a layout, a content tree, an `AGENTS.md`, a host allow list — and nine
//! copies of it drift. What each test varies, it varies explicitly.

#![allow(dead_code)]

use liyasa_agent::agents_md::{self, AgentsMd};
use liyasa_agent::config::{AgentConfig, ContextRepo};
use liyasa_agent::hosts::KnownHosts;
use liyasa_agent::proposal::{Attribution, Section, Summary};
use liyasa_agent::record::{GraphAccess, RunId};
use liyasa_agent::run::{Request, Run, Surface};
use liyasa_agent::scope::Layout;
use liyasa_agent::testing::MemoryPages;
use liyasa_agent::trust::{Input, InputKind, Trigger, TriggerKind, TrustLevel};
use liyasa_core::ids::Route;

/// A page with front matter, so the access-field checks have something to read.
pub fn page(body: &str) -> String {
    format!("---\ntitle: A page\n---\n\n{body}\n")
}

pub const AGENTS_MD: &str = "\
# Writing for Acme

Write in the present tense and address the reader as `you`.

## Forbidden phrases

- simply
- click here

## Product naming

- Write **Acme Cloud**, not Acmecloud
";

pub fn layout() -> Layout {
    Layout::default()
}

pub fn agents_md() -> AgentsMd {
    agents_md::parse(AGENTS_MD)
}

/// The hosts the site already links to.
pub fn known_hosts() -> KnownHosts {
    KnownHosts::new(["docs.acme.example", "acme.example"])
}

pub fn pages() -> MemoryPages {
    MemoryPages::new(layout())
        .with("guides/install.md", page("Download the installer."))
        .with("guides/upgrade.md", page("Back up first."))
        .with("pricing.md", page("Ten seats."))
}

/// A project with two context repositories and a fetch allow list.
pub fn config() -> AgentConfig {
    AgentConfig {
        context_repos: vec![
            ContextRepo {
                name: "acme/api".to_owned(),
                allow: vec!["src".to_owned(), "openapi.yaml".to_owned()],
                deny: vec!["src/secrets".to_owned()],
            },
            ContextRepo {
                name: "acme/cli".to_owned(),
                allow: vec!["src".to_owned()],
                deny: Vec::new(),
            },
        ],
        fetch_hosts: KnownHosts::new(["docs.acme.example"]),
        ..AgentConfig::default()
    }
}

/// AGT-01's input list, at member trust.
pub fn member_inputs() -> Vec<Input> {
    vec![
        Input::new(InputKind::ContentTree, "the site", TrustLevel::Member),
        Input::new(InputKind::TruthGraph, "the truth graph", TrustLevel::Member),
        Input::new(
            InputKind::VerificationResults,
            "last night's verify",
            TrustLevel::Member,
        ),
        Input::new(InputKind::StyleGuide, "Vale", TrustLevel::Member),
        Input::new(InputKind::AgentsMd, "AGENTS.md", TrustLevel::Operator),
        Input::new(InputKind::ContextRepo, "acme/api", TrustLevel::Member),
    ]
}

/// A drift-triggered run: the signal is the project's own, so `member`.
pub fn drift_trigger() -> Trigger {
    Trigger::new(TriggerKind::Drift, TrustLevel::Member).about([Route::new("/pricing")])
}

/// A feedback-triggered run: a stranger wrote the trigger.
pub fn feedback_trigger() -> Trigger {
    Trigger::new(TriggerKind::Feedback, TrustLevel::Anonymous)
        .about([Route::new("/guides/install")])
}

pub fn request(trigger: Trigger, task: &str) -> Request {
    let mut inputs = member_inputs();
    inputs.push(Input::new(InputKind::Task, "the trigger", trigger.trust));
    Request {
        run: RunId::new("run-acceptance"),
        surface: Surface::Cli,
        trigger,
        task: task.to_owned(),
        inputs,
        policy: liyasa_agent::policy::Policy::Proposal,
        attribution: Attribution::person("Ada Lovelace", "ada@acme.example"),
        content_tree: Some("blake3:9f2c4e".to_owned()),
        graph: GraphAccess::ReadAndPropose,
        retention_days: 90,
        branch: "agent/run-acceptance".to_owned(),
    }
}

pub fn start(request: Request) -> Run {
    liyasa_agent::run::start(request, config(), layout(), agents_md(), known_hosts())
}

pub fn summary(task: &str) -> Summary {
    Summary {
        sections: vec![Section {
            task: task.to_owned(),
            ..Section::default()
        }],
        ..Summary::default()
    }
}

/// Walks a run to the write phase.
pub fn to_write(run: &mut Run) {
    use liyasa_agent::record::Phase;
    run.enter(Phase::Research).expect("research");
    run.enter(Phase::Plan).expect("plan");
    run.enter(Phase::Write).expect("write");
}
