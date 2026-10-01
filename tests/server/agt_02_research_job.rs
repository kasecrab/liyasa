//! The research phase, run by the worker (AGT-02).
//!
//! `routes/research.rs` dispatched the read tools and nothing reached it —
//! a `pub mod` and a passing test. What is asserted here is composition: the
//! registry the binary runs carries a job that executes those tools, so the
//! answer is "the product does this" rather than "the dispatch exists".

use liyasa_server::routes::{agent, work};
use liyasa_tests::server::Harness;
use serde_json::json;

async fn job_result(harness: &Harness, name: &str) -> serde_json::Value {
    let store = harness.state.store.clone().expect("a store");
    let rows = store
        .jobs_typed()
        .list(&Default::default(), liyasa_core::store::Page::default())
        .await
        .expect("the jobs are listable");
    rows.iter()
        .find(|job| job.name == name)
        .expect("the job ran")
        .result
        .clone()
        .unwrap_or_default()
}

#[test]
fn the_research_job_is_registered_and_enqueued_by_its_caller() {
    let kind = work::kinds()
        .iter()
        .find(|kind| kind.name == agent::RESEARCH_JOB)
        .expect("a job nothing registers never runs");
    assert!(
        matches!(kind.trigger, work::Trigger::Caller),
        "the worker's timer does not start a research pass; whoever asks for one does"
    );
}

#[tokio::test]
async fn a_research_pass_answers_the_tools_this_instance_serves() {
    let (harness, _site) = Harness::serving("agt02-job").await;
    let store = harness.state.store.clone().expect("a store");
    store
        .jobs_typed()
        .enqueue(&liyasa_store::jobs::Enqueue {
            payload: json!({
                "task": "does the install guide still match the CLI",
                "calls": [{ "tool": "search_docs", "input": { "query": "install" } }]
            }),
            ..liyasa_store::jobs::Enqueue::new(agent::RESEARCH_JOB, "run:1")
        })
        .await
        .expect("a row");

    let ran = work::run_once(&harness.state, work::kinds(), "replica-a")
        .await
        .expect("a pass");
    assert_eq!(ran, 1);

    let result = job_result(&harness, agent::RESEARCH_JOB).await;
    let calls = result["calls"].as_array().expect("the calls are recorded");
    assert_eq!(calls.len(), 1, "{result}");
    assert_eq!(calls[0]["tool"], "search_docs");
    assert!(
        calls[0]["answer"]["passages"].is_array(),
        "search_docs answers passages: {result}"
    );
}

/// WP-25's ask, and the reason it matters: an empty drift list and a server
/// that cannot see drift are the same JSON. A reviewer reading a run that
/// concluded "no drift" has to be able to tell whether it looked.
#[tokio::test]
async fn a_tool_this_instance_cannot_serve_is_recorded_rather_than_dropped() {
    let (harness, _site) = Harness::serving("agt02-unavailable").await;
    let store = harness.state.store.clone().expect("a store");
    store
        .jobs_typed()
        .enqueue(&liyasa_store::jobs::Enqueue {
            payload: json!({
                "task": "what has drifted",
                "calls": [{ "tool": "list_drift", "input": {} }]
            }),
            ..liyasa_store::jobs::Enqueue::new(agent::RESEARCH_JOB, "run:2")
        })
        .await
        .expect("a row");

    work::run_once(&harness.state, work::kinds(), "replica-a")
        .await
        .expect("a pass");

    let result = job_result(&harness, agent::RESEARCH_JOB).await;
    let answer = &result["calls"][0]["answer"];
    assert_eq!(
        answer["unavailable"], "list_drift",
        "the refusal is in the record, not an empty list: {result}"
    );
    assert!(
        answer["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("WP-20c")),
        "the refusal names whose data is missing: {result}"
    );
}

/// A payload this handler cannot read is a producer defect, not a missing
/// precondition — it fails and is recorded rather than skipping quietly.
#[tokio::test]
async fn a_payload_the_handler_cannot_read_fails_rather_than_skipping() {
    let (harness, _site) = Harness::serving("agt02-badpayload").await;
    let store = harness.state.store.clone().expect("a store");
    store
        .jobs_typed()
        .enqueue(&liyasa_store::jobs::Enqueue {
            payload: json!({ "task": "no calls key" }),
            ..liyasa_store::jobs::Enqueue::new(agent::RESEARCH_JOB, "run:3")
        })
        .await
        .expect("a row");

    work::run_once(&harness.state, work::kinds(), "replica-a")
        .await
        .expect("a pass");

    let rows = store
        .jobs_typed()
        .list(&Default::default(), liyasa_core::store::Page::default())
        .await
        .expect("the jobs are listable");
    let job = rows
        .iter()
        .find(|job| job.name == agent::RESEARCH_JOB)
        .expect("the job ran");
    assert_ne!(
        job.state,
        liyasa_core::store::JobState::Done,
        "a payload that does not read is a failure, not a success"
    );
}
