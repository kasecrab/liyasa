//! AGT-02 — Given the golden drift scenario set; when runs execute against the
//! pinned model; then each run passes through research, plan, write, validate, and
//! publish phases, and validation failures block the proposal.
//!
//! The packet marks this row *nightly, needs keys*, and it is the only one of WP-25's
//! seventeen that does. The live half is escalated in `NEEDS-INPUT.md` (2026-09-30):
//! it needs a provider key and a decision on which model id the golden set is pinned
//! to.
//!
//! What is here is the half that does not need one, and it is not a placeholder: the
//! phase sequence, the refusal to skip a phase, and "validation failures block the
//! proposal" are all properties of this crate rather than of a model, and they are
//! driven by [`ScriptedModel`]. The live replay is one `#[ignore]`d test below, which
//! **fails rather than passes** when it is enabled without keys, because a test that
//! checks a precondition and returns early is counted as passed and its output is
//! suppressed.

use liyasa_agent::policy::{Outcome, Signals};
use liyasa_agent::record::Phase;
use liyasa_agent::run::PhaseError;
use liyasa_agent::testing::{ScriptedModel, write_page};
use liyasa_core::ai::ChatEvent;

use crate::agent_support as support;

/// One scenario of the golden set, as far as a mock can carry it.
struct Scenario {
    task: &'static str,
    route: &'static str,
    path: &'static str,
    markdown: &'static str,
    /// Whether the writing breaks a rule in `AGENTS.md`.
    blocks_validation: bool,
}

const GOLDEN: &[Scenario] = &[
    Scenario {
        task: "the seat limit changed from 5 to 10",
        route: "/pricing",
        path: "pricing.md",
        markdown: "Ten seats on every plan.",
        blocks_validation: false,
    },
    Scenario {
        task: "the installer moved",
        route: "/guides/install",
        path: "guides/install.md",
        markdown: "Download the installer from the releases page.",
        blocks_validation: false,
    },
    Scenario {
        task: "the upgrade notes are out of date",
        route: "/guides/upgrade",
        path: "guides/upgrade.md",
        // `simply` is forbidden by AGENTS.md, so this one must not publish.
        markdown: "Simply back up first, then upgrade.",
        blocks_validation: true,
    },
];

#[tokio::test]
async fn every_golden_scenario_walks_the_five_phases_and_only_the_clean_ones_publish() {
    let mut published = 0usize;
    let mut blocked = 0usize;
    for scenario in GOLDEN {
        let pages = support::pages();
        let model = ScriptedModel::new([vec![
            ChatEvent::Token(format!("Handling: {}", scenario.task)),
            write_page("1", scenario.route, &support::page(scenario.markdown)),
            ChatEvent::Done,
        ]]);
        let mut run = support::start(support::request(support::drift_trigger(), scenario.task));

        run.enter(Phase::Research).expect("research");
        run.enter(Phase::Plan).expect("plan");
        run.enter(Phase::Write).expect("write");
        run.write_turn(&model, &pages, "do the task")
            .await
            .expect("the scripted model answers");
        assert_eq!(
            run.diff().files_changed(),
            1,
            "`{}` wrote nothing",
            scenario.task
        );
        assert_eq!(run.diff().files[0].path, scenario.path);

        run.enter(Phase::Validate).expect("validate");
        match run.validate(Vec::new()) {
            Err(validation) => {
                assert!(
                    scenario.blocks_validation,
                    "`{}` failed validation unexpectedly: {}",
                    scenario.task,
                    validation.reason()
                );
                blocked += 1;
                // And there is no `Validated` to publish with, which is the block.
                continue;
            }
            Ok(validated) => {
                assert!(
                    !scenario.blocks_validation,
                    "`{}` should have failed validation",
                    scenario.task
                );
                run.enter(Phase::Publish).expect("publish");
                let result = run
                    .publish(
                        validated,
                        support::summary(scenario.task),
                        Signals::default(),
                        false,
                    )
                    .expect("published");
                assert_eq!(result.decision.outcome, Outcome::Proposal);
                assert_eq!(run.record().phases(), Phase::ALL);
                published += 1;
            }
        }
    }
    assert_eq!(published, 2, "the clean scenarios did not publish");
    assert_eq!(blocked, 1, "the failing scenario was not blocked");
}

#[tokio::test]
async fn a_run_cannot_reach_publish_without_passing_through_the_others() {
    let mut run = support::start(support::request(support::drift_trigger(), "a task"));
    assert_eq!(
        run.enter(Phase::Publish),
        Err(PhaseError::NotFirst {
            asked: Phase::Publish
        })
    );
    run.enter(Phase::Research).expect("research");
    assert_eq!(
        run.enter(Phase::Publish),
        Err(PhaseError::OutOfOrder {
            current: Phase::Research,
            asked: Phase::Publish
        })
    );
}

#[tokio::test]
async fn the_record_of_a_completed_run_shows_all_five_phases_in_order() {
    let pages = support::pages();
    let model = ScriptedModel::new([vec![
        write_page("1", "/pricing", &support::page("Ten seats.")),
        ChatEvent::Done,
    ]]);
    let mut run = support::start(support::request(support::drift_trigger(), "a task"));
    for phase in [Phase::Research, Phase::Plan, Phase::Write] {
        run.enter(phase).expect("phase");
    }
    run.write_turn(&model, &pages, "do it")
        .await
        .expect("answered");
    run.enter(Phase::Validate).expect("validate");
    let validated = run.validate(Vec::new()).expect("clean");
    run.enter(Phase::Publish).expect("publish");
    run.publish(
        validated,
        support::summary("a task"),
        Signals::default(),
        false,
    )
    .expect("published");
    assert_eq!(run.record().phases(), Phase::ALL);
}

/// The live half. `#[ignore]`d until a key exists (`NEEDS-INPUT.md`, 2026-09-30).
///
/// It FAILS rather than skips when run without one. A test that checks a
/// precondition and returns early is counted as passed and its output suppressed,
/// which is how an unreachable test stays green for a week.
#[tokio::test]
#[ignore = "needs a provider key and a pinned model id: see NEEDS-INPUT.md, WP-25, 2026-09-30"]
async fn the_golden_set_replays_against_the_pinned_model() {
    let key = std::env::var("LIYASA_AGENT_TEST_MODEL_KEY").unwrap_or_default();
    let model = std::env::var("LIYASA_AGENT_TEST_MODEL").unwrap_or_default();
    assert!(
        !key.is_empty() && !model.is_empty(),
        "this test was enabled without LIYASA_AGENT_TEST_MODEL_KEY and \
         LIYASA_AGENT_TEST_MODEL. It is `#[ignore]`d for exactly that reason, and it \
         fails rather than skipping so that enabling it without the keys is visible."
    );
    panic!(
        "the golden replay against `{model}` is not built: the run loop needs the \
         research phase's read tools, which are served by WP-14's routes and WP-18's \
         index rather than by this crate. What exists is the deterministic half, in \
         the tests above."
    );
}
