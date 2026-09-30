//! AGT-41 — Given reviewer comments on a proposal; when "address these comments"
//! runs; then a follow-up commit lands on the same proposal branch.

use liyasa_agent::diff::{Diff, FileChange};
use liyasa_agent::gates::{self, Report};
use liyasa_agent::policy::Signals;
use liyasa_agent::proposal::{FileDecision, Proposal};
use liyasa_agent::record::Phase;
use liyasa_agent::testing::{ScriptedModel, write_page};
use liyasa_agent::trust::{Restrictions, Trigger, TriggerKind, TrustLevel};
use liyasa_core::ai::ChatEvent;
use liyasa_core::ids::Route;

use crate::agent_support as support;

fn member() -> Restrictions {
    Restrictions::for_run(
        TrustLevel::Member,
        &Trigger::new(TriggerKind::Prompt, TrustLevel::Member),
    )
}

fn gate(diff: &Diff) -> Report {
    let config = support::config();
    gates::check(gates::Inputs {
        diff,
        restrictions: &member(),
        limits: &config.limits,
        layout: &support::layout(),
        known_hosts: &support::known_hosts(),
        injection: &config.injection(),
        allow_bulk_delete: false,
    })
}

/// A run that reaches a proposal.
async fn published() -> liyasa_agent::run::Published {
    let pages = support::pages();
    let model = ScriptedModel::new([vec![
        ChatEvent::Token("Updating the install guide.".to_owned()),
        write_page(
            "1",
            "/guides/install",
            &support::page("Download the installer, then run it."),
        ),
        ChatEvent::Done,
    ]]);
    let mut run = support::start(support::request(
        support::drift_trigger(),
        "the installer moved",
    ));
    support::to_write(&mut run);
    run.write_turn(&model, &pages, "update it")
        .await
        .expect("answered");
    run.enter(Phase::Validate).expect("validate");
    let validated = run.validate(Vec::new()).expect("clean");
    run.enter(Phase::Publish).expect("publish");
    run.publish(
        validated,
        support::summary("the installer moved"),
        Signals::default(),
        false,
    )
    .expect("published")
}

#[tokio::test]
async fn addressing_comments_lands_a_follow_up_on_the_same_branch() {
    let mut proposal = published().await.proposal;
    let branch = proposal.branch().to_owned();
    let follow = Diff::new([FileChange::modified(
        "guides/install.md",
        support::page("Download the installer, then run it."),
        support::page("Download the installer. Run it."),
    )]);
    let report = gate(&follow);
    let added = proposal
        .address(
            ["split the second sentence", "drop the comma"],
            follow,
            &report,
        )
        .expect("the follow-up gated clean");
    assert_eq!(added.comments.len(), 2);
    assert_eq!(
        proposal.branch(),
        branch,
        "a follow-up must not open a new proposal"
    );
    assert_eq!(proposal.follow_ups().len(), 1);
}

#[tokio::test]
async fn the_follow_up_is_named_in_the_proposal_header() {
    let mut proposal = published().await.proposal;
    let follow = Diff::new([FileChange::modified(
        "guides/install.md",
        support::page("Download the installer, then run it."),
        support::page("Download the installer. Run it."),
    )]);
    let report = gate(&follow);
    proposal
        .address(["split the sentence"], follow, &report)
        .expect("clean");
    let header = proposal.header();
    assert!(header.contains("Follow-ups"), "{header}");
    assert!(header.contains("1 round"), "{header}");
}

#[tokio::test]
async fn several_rounds_of_comments_all_land_on_the_one_proposal() {
    let mut proposal = published().await.proposal;
    let branch = proposal.branch().to_owned();
    for n in 0..3 {
        let follow = Diff::new([FileChange::modified(
            "guides/install.md",
            support::page("Download the installer, then run it."),
            support::page(&format!("Revision {n}.")),
        )]);
        let report = gate(&follow);
        proposal
            .address([format!("round {n}")], follow, &report)
            .expect("clean");
    }
    assert_eq!(proposal.follow_ups().len(), 3);
    assert_eq!(proposal.branch(), branch);
}

#[tokio::test]
async fn a_follow_up_the_gate_rejects_lands_nothing() {
    // The follow-up goes through the same gate as the first commit. A reviewer
    // asking for something the gate refuses does not get it.
    let mut proposal = published().await.proposal;
    let hostile = Diff::new([FileChange::modified(
        "guides/install.md",
        support::page("Download the installer, then run it."),
        support::page("Run `acme --key AKIAQWERTYUIOPASDFGH`."),
    )]);
    let report = gate(&hostile);
    assert!(report.rejected());
    assert!(
        proposal
            .address(["put the key in"], hostile, &report)
            .is_err()
    );
    assert!(proposal.follow_ups().is_empty());
}

#[tokio::test]
async fn a_follow_up_touching_a_new_file_makes_that_file_undecided_again() {
    let mut proposal = published().await.proposal;
    proposal.accept_all();
    assert!(proposal.is_fully_reviewed());
    let follow = Diff::new([FileChange::added(
        "guides/troubleshooting.md",
        support::page("If it fails, check the log."),
    )]);
    let report = gate(&follow);
    proposal
        .address(["add a troubleshooting page"], follow, &report)
        .expect("clean");
    assert_eq!(proposal.undecided(), ["guides/troubleshooting.md"]);
    assert!(!proposal.is_fully_reviewed());
}

#[tokio::test]
async fn per_file_and_per_run_decisions_both_work_on_the_proposal() {
    // AGT-40's "accept or reject per run and per file", at the Rust seam the
    // dashboard drives.
    let pages = support::pages();
    let model = ScriptedModel::new([vec![
        write_page("1", "/guides/install", &support::page("One.")),
        write_page("2", "/guides/upgrade", &support::page("Two.")),
        ChatEvent::Done,
    ]]);
    let mut run = support::start(support::request(support::drift_trigger(), "two pages"));
    support::to_write(&mut run);
    run.write_turn(&model, &pages, "update both")
        .await
        .expect("answered");
    run.enter(Phase::Validate).expect("validate");
    let validated = run.validate(Vec::new()).expect("clean");
    run.enter(Phase::Publish).expect("publish");
    let mut proposal = run
        .publish(
            validated,
            support::summary("two pages"),
            Signals::default(),
            false,
        )
        .expect("published")
        .proposal;

    assert_eq!(proposal.files().count(), 2);
    assert!(proposal.accept("guides/install.md"));
    assert!(proposal.reject("guides/upgrade.md"));
    assert_eq!(proposal.accepted(), ["guides/install.md"]);
    assert_eq!(
        proposal.decision("guides/upgrade.md"),
        FileDecision::Rejected
    );

    proposal.accept_all();
    assert_eq!(proposal.accepted().len(), 2);
}

#[tokio::test]
async fn a_run_can_be_split_per_page_on_request() {
    // AGT-22's second half.
    let pages = support::pages();
    let model = ScriptedModel::new([vec![
        write_page("1", "/guides/install", &support::page("One.")),
        write_page("2", "/guides/upgrade", &support::page("Two.")),
        ChatEvent::Done,
    ]]);
    let mut run = support::start(support::request(support::drift_trigger(), "two pages"));
    support::to_write(&mut run);
    run.write_turn(&model, &pages, "update both")
        .await
        .expect("answered");
    run.enter(Phase::Validate).expect("validate");
    let validated = run.validate(Vec::new()).expect("clean");
    run.enter(Phase::Publish).expect("publish");
    let proposal: Proposal = run
        .publish(
            validated,
            support::summary("two pages"),
            Signals::default(),
            false,
        )
        .expect("published")
        .proposal;
    let parts = proposal.split_per_page(&support::layout());
    assert_eq!(parts.len(), 2);
    let mut paths: Vec<String> = parts
        .iter()
        .flat_map(|p| p.diff().files.iter().map(|f| f.path.clone()))
        .collect();
    paths.sort();
    assert_eq!(paths, ["guides/install.md", "guides/upgrade.md"]);
}

#[tokio::test]
async fn a_person_triggered_proposal_names_the_person_as_co_author() {
    // AGT-21.
    let published = published().await;
    assert_eq!(
        published.proposal.attribution().trailers(),
        ["Co-authored-by: Ada Lovelace <ada@acme.example>"]
    );
}

#[tokio::test]
async fn an_automation_triggered_proposal_is_attributed_to_the_automation() {
    let mut request = support::request(support::drift_trigger(), "nightly drift");
    request.attribution = liyasa_agent::proposal::Attribution::automation("nightly-drift");
    let mut run = liyasa_agent::run::start(
        request,
        support::config(),
        support::layout(),
        support::agents_md(),
        support::known_hosts(),
    );
    support::to_write(&mut run);
    run.write(
        &Route::new("/pricing"),
        "pricing.md",
        support::page("Ten seats, or twenty on Pro."),
        Some(support::page("Ten seats.")),
    )
    .expect("written");
    run.enter(Phase::Validate).expect("validate");
    let validated = run.validate(Vec::new()).expect("clean");
    run.enter(Phase::Publish).expect("publish");
    let published = run
        .publish(
            validated,
            support::summary("nightly drift"),
            Signals::default(),
            false,
        )
        .expect("published");
    assert_eq!(
        published.proposal.attribution().trailers(),
        ["Automation: nightly-drift"]
    );
}
