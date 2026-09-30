//! AGT-05 — Given a completed run; when its record is read; then every tool call and
//! model exchange is present with secrets redacted (scrubber corpus).

use liyasa_agent::record::{CallOutcome, Entry, Phase};
use liyasa_agent::secrets;
use liyasa_agent::testing::{MemoryPages, ScriptedModel, write_page};
use liyasa_agent::tools;
use liyasa_core::ai::ChatEvent;
use serde_json::json;

use crate::agent_support as support;

/// One synthetic credential per pattern in the corpus. Built to the published
/// shapes; none is a real key.
const SCRUBBER_CORPUS: &[&str] = &[
    "AKIAQWERTYUIOPASDFGH",
    "sk-ant-api03-QWERtyuiOPasdfGHjklZXCVbnmQWERtyui",
    "ghp_QWERtyuiOPasdfGHjklZXCVbnmQWERtyuiOP",
    "AIzaQWERtyuiOPasdfGHjklZXCVbnmQWERtyuiO",
    "xoxb-QWERtyuiOP-asdfGHjklZXC",
    "sk_live_QWERtyuiOPasdfGHjkl",
    "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZ2VudCJ9.QWERtyuiOPasdfGHjkl",
];

#[tokio::test]
async fn every_tool_call_and_model_exchange_is_in_the_record() {
    let pages = support::pages();
    let model = ScriptedModel::new([vec![
        ChatEvent::Token("Raising the seat limit.".to_owned()),
        write_page(
            "1",
            "/pricing",
            &support::page("Ten seats, or twenty on Pro."),
        ),
        ChatEvent::Usage {
            input: 1200,
            output: 60,
        },
        ChatEvent::Done,
    ]])
    .named("anthropic:claude-opus-5-5");

    let mut run = support::start(support::request(
        support::drift_trigger(),
        "the seat limit changed",
    ));
    run.enter(Phase::Research).expect("research");
    run.authorise(tools::SEARCH_DOCS, &json!({ "query": "seats" }))
        .expect("a search");
    run.authorise(tools::GET_FACT, &json!({ "fact": "plan.seats" }))
        .expect("a fact");
    run.enter(Phase::Plan).expect("plan");
    run.enter(Phase::Write).expect("write");
    run.write_turn(&model, &pages, "update the pricing page")
        .await
        .expect("the model answered");

    let calls: Vec<&str> = run.record().calls().map(|(name, _, _)| name).collect();
    assert_eq!(
        calls,
        [tools::SEARCH_DOCS, tools::GET_FACT, tools::WRITE_PAGE]
    );
    assert_eq!(run.record().exchanges().count(), 1);
    assert!(
        run.record()
            .calls()
            .all(|(_, _, outcome)| *outcome == CallOutcome::Ok)
    );
}

#[test]
fn a_refused_call_is_in_the_record_too() {
    // "Every tool call", not every successful one. A run's refusals are the part a
    // reviewer most wants to see.
    let mut run = support::start(support::request(support::feedback_trigger(), "wrong"));
    run.enter(Phase::Research).expect("research");
    run.authorise(
        tools::WEB_FETCH,
        &json!({ "url": "https://docs.acme.example/x" }),
    )
    .expect_err("below min_trust");
    let (name, _, outcome) = run.record().calls().next().expect("an entry");
    assert_eq!(name, tools::WEB_FETCH);
    assert!(
        matches!(outcome, CallOutcome::Rejected { .. }),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn no_value_in_the_scrubber_corpus_survives_into_the_record() {
    for credential in SCRUBBER_CORPUS {
        // One credential, in every place a run can put one: the trigger text, a
        // tool input, the model's output, and a note.
        let pages =
            MemoryPages::new(support::layout()).with("guides/install.md", support::page("Old."));
        let model = ScriptedModel::new([vec![
            ChatEvent::Token(format!("the key is {credential}")),
            write_page(
                "1",
                "/guides/install",
                &support::page(&format!("Run `acme --key {credential}`.")),
            ),
            ChatEvent::Done,
        ]]);
        let mut run = support::start(support::request(
            support::drift_trigger(),
            &format!("a reader reported that {credential} does not work"),
        ));
        support::to_write(&mut run);
        run.write_turn(&model, &pages, "fix the sample")
            .await
            .expect("answered");
        run.record_mut()
            .note(&format!("the repository contains {credential}"));

        let text = serde_json::to_string(run.record()).expect("serializes");
        assert!(
            !text.contains(credential),
            "`{credential}` survived into the record"
        );
        assert!(text.contains(secrets::REDACTION), "nothing was redacted");
    }
}

#[test]
fn a_secret_in_a_nested_tool_input_is_redacted() {
    let credential = SCRUBBER_CORPUS[0];
    let mut run = support::start(support::request(support::drift_trigger(), "a task"));
    run.enter(Phase::Research).expect("research");
    run.authorise(
        tools::PROPOSE_FACT_UPDATE,
        &json!({
            "fact": "plan.token",
            "value": { "nested": [{ "deeper": credential }] },
            "evidence": "the API returned it"
        }),
    )
    .expect("a member may propose");
    let text = serde_json::to_string(run.record()).expect("serializes");
    assert!(!text.contains(credential), "{text}");
}

#[test]
fn the_record_carries_its_retention_so_a_sweep_knows_what_to_keep() {
    let run = support::start(support::request(support::drift_trigger(), "a task"));
    assert_eq!(run.record().header().retention_days, 90);
}

#[test]
fn the_entries_are_ordered_so_a_run_can_be_replayed_in_sequence() {
    let mut run = support::start(support::request(support::drift_trigger(), "a task"));
    run.enter(Phase::Research).expect("research");
    run.authorise(tools::LIST_DRIFT, &json!({ "open": true }))
        .expect("a drift list");
    run.enter(Phase::Plan).expect("plan");
    let seqs: Vec<u32> = run.record().entries().iter().map(Entry::seq).collect();
    assert_eq!(seqs, (0..seqs.len() as u32).collect::<Vec<_>>());
}
