//! The research phase, driven end to end through the composed library (AGT-02,
//! AGT-03, AGT-05).
//!
//! `tests/server/agt_02_research.rs` drives the first half: `Run::authorise`
//! permits a call and `routes::research::execute` answers it. It stops there,
//! and the half it stops before is where this package's work was missing — the
//! result has to come BACK into the run, wrapped as a delimited data block, or
//! the write phase is asked to rewrite a page having read nothing.
//!
//! So these drive the whole loop: authorise, execute, `record_result`, then a
//! model turn that has to be able to see what was found. Every assertion is on
//! the composed path and not on a unit called directly, which is the distinction
//! that made `ConfigArea::from_key` and `model::result_block` both exist with
//! green suites and no caller.
//!
//! **What is NOT asserted, and the reason matters.** `research::execute` has one
//! caller in the whole workspace and it is a test — no HTTP route reaches it, and
//! `routes/mod.rs` names it once, on its `pub mod` line. So this proves the two
//! libraries compose, and says nothing about a running server serving the
//! research phase. That gap is not pinned here: a test asserting "no route serves
//! this" would go red on the commit that fixes it, which is somebody else's
//! commit to make.

use liyasa_agent::record::Phase;
use liyasa_agent::tools;
use liyasa_ai::index::ChunkQuery;
use liyasa_core::ai::{ChatEvent, TrustLevel};
use liyasa_server::routes::research;
use liyasa_tests::server::Harness;
use serde_json::json;

use crate::agent_support as support;

/// This instance's read tools, as the assistant builds them.
async fn instance_tools() -> (liyasa_server::routes::tools::ServerTools, ChunkQuery) {
    let (harness, _site) = Harness::serving("agt02-seam").await;
    let bundle = harness.state.bundle.clone().expect("the fixture bundle");
    let filter = ChunkQuery::default();
    (
        liyasa_server::routes::tools::ServerTools::new(bundle, filter.clone()),
        filter,
    )
}

#[tokio::test]
async fn a_search_result_comes_back_into_the_run_and_reaches_the_model() {
    let (tools_impl, filter) = instance_tools().await;
    let mut run = support::start(support::request(
        support::drift_trigger(),
        "the pricing page disagrees with the plan table",
    ));
    run.enter(Phase::Research).expect("research");

    let input = json!({ "query": "pricing" });
    run.authorise(tools::SEARCH_DOCS, &input)
        .expect("a drift run may search");
    let answer = research::execute(&tools_impl, &filter, tools::SEARCH_DOCS, &input)
        .await
        .expect("this instance serves search_docs");
    run.record_result(tools::SEARCH_DOCS, "pricing", &answer);

    // The record has the call AND the result.
    assert_eq!(run.record().calls().count(), 1);
    let (tool, _, trust) = run.record().results().next().expect("a result entry");
    assert_eq!(tool, tools::SEARCH_DOCS);
    assert_eq!(trust, TrustLevel::Member);

    // And the write phase can see it.
    let pages = support::pages();
    let model = liyasa_agent::testing::ScriptedModel::new([vec![ChatEvent::Done]]);
    run.enter(Phase::Plan).expect("plan");
    run.enter(Phase::Write).expect("write");
    run.write_turn(&model, &pages, "reconcile the two pages")
        .await
        .expect("answered");

    let seen = model.seen();
    assert_eq!(seen.len(), 1);
    let found = seen[0]
        .data
        .iter()
        .find(|b| b.label.starts_with(tools::SEARCH_DOCS))
        .expect("the finding did not reach the model");
    assert_eq!(found.trust, TrustLevel::Member);
    assert!(found.content.contains("passages"), "{}", found.content);
}

#[tokio::test]
async fn an_unavailable_tool_is_recorded_as_a_refusal_rather_than_a_gap() {
    // `get_fact` and `list_drift` are not served by this instance. The agent has
    // to be able to tell "there is no drift" from "this server cannot see drift",
    // and the record has to show which.
    let (tools_impl, filter) = instance_tools().await;
    let mut run = support::start(support::request(support::drift_trigger(), "a task"));
    run.enter(Phase::Research).expect("research");

    for tool in [tools::GET_FACT, tools::LIST_DRIFT] {
        let input = if tool == tools::GET_FACT {
            json!({ "fact": "plan.seats" })
        } else {
            json!({ "open": true })
        };
        run.authorise(tool, &input)
            .unwrap_or_else(|e| panic!("`{tool}` should be permitted: {e}"));
        let unavailable = research::execute(&tools_impl, &filter, tool, &input)
            .await
            .expect_err("this instance holds no reader for it");
        // The refusal goes into the run as the result, so the transcript shows it.
        run.record_result(tool, "unavailable", &unavailable.as_json());
    }

    let recorded: Vec<&str> = run.record().results().map(|(tool, _, _)| tool).collect();
    assert_eq!(recorded, [tools::GET_FACT, tools::LIST_DRIFT]);
    for (_, value, _) in run.record().results() {
        assert!(
            value.get("unavailable").is_some(),
            "a refusal was stored as an ordinary result: {value}"
        );
    }
}

#[tokio::test]
async fn an_unavailable_result_does_not_read_as_an_empty_answer_to_the_model() {
    // The distinction has to survive the trip into the prompt, not only into the
    // record: a block saying `{"passages":[]}` and one saying
    // `{"unavailable":"list_drift"}` lead the model to opposite conclusions.
    let (tools_impl, filter) = instance_tools().await;
    let mut run = support::start(support::request(support::drift_trigger(), "a task"));
    run.enter(Phase::Research).expect("research");
    let input = json!({ "open": true });
    run.authorise(tools::LIST_DRIFT, &input).expect("permitted");
    let unavailable = research::execute(&tools_impl, &filter, tools::LIST_DRIFT, &input)
        .await
        .expect_err("not served");
    run.record_result(tools::LIST_DRIFT, "open drift", &unavailable.as_json());

    let pages = support::pages();
    let model = liyasa_agent::testing::ScriptedModel::new([vec![ChatEvent::Done]]);
    run.enter(Phase::Plan).expect("plan");
    run.enter(Phase::Write).expect("write");
    run.write_turn(&model, &pages, "do what you can")
        .await
        .expect("answered");

    let block = model.seen()[0]
        .data
        .iter()
        .find(|b| b.label.starts_with(tools::LIST_DRIFT))
        .expect("the refusal did not reach the model")
        .clone();
    assert!(block.content.contains("unavailable"), "{}", block.content);
    assert!(
        block.content.contains("WP-20c"),
        "the model is not told whose data is missing: {}",
        block.content
    );
}

#[tokio::test]
async fn a_served_openapi_operation_comes_back_as_member_data() {
    let (tools_impl, filter) = instance_tools().await;
    let mut run = support::start(support::request(support::drift_trigger(), "a task"));
    run.enter(Phase::Research).expect("research");
    let input = json!({ "operation": "GET /seats" });
    run.authorise(tools::GET_OPENAPI, &input)
        .expect("permitted");
    let answer = research::execute(&tools_impl, &filter, tools::GET_OPENAPI, &input)
        .await
        .expect("this instance serves get_openapi");
    let block = run
        .record_result(tools::GET_OPENAPI, "GET /seats", &answer)
        .clone();
    // A spec this site does not publish answers `found: false`, which is a true
    // answer and still the site's own data.
    assert_eq!(block.trust, TrustLevel::Member);
    assert!(block.label.starts_with(tools::GET_OPENAPI));
}

#[tokio::test]
async fn a_research_phase_that_found_nothing_still_sends_the_task() {
    // Falsifies the tests above: they assert a block arrives, so this asserts the
    // request is not empty when none does — otherwise a `write_turn` that sent
    // nothing at all would pass them by vacuous absence.
    let pages = support::pages();
    let model = liyasa_agent::testing::ScriptedModel::new([vec![ChatEvent::Done]]);
    let mut run = support::start(support::request(
        support::drift_trigger(),
        "the seat limit changed",
    ));
    support::to_write(&mut run);
    run.write_turn(&model, &pages, "do it")
        .await
        .expect("answered");

    let seen = model.seen();
    assert_eq!(seen[0].data.len(), 1, "only the task should be there");
    assert_eq!(seen[0].data[0].label, "the task");
    assert!(seen[0].data[0].content.contains("seat limit"));
    assert!(run.findings().is_empty());
}
