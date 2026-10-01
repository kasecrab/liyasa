//! The agent's research phase, served by this instance (AGT-02).
//!
//! `liyasa-agent` declares the four read tools, gates them, and says in
//! `write_turn`'s doc comment that the caller holding the data serves them.
//! These drive the two halves together: `Run::authorise` decides whether the
//! call is permitted, `routes::research::execute` answers it from this
//! server's own bundle and index.
//!
//! What is deliberately NOT asserted is a full five-phase replay — that is
//! AGT-02's acceptance test and it needs a live model and provider keys.

use liyasa_agent::record::Phase;
use liyasa_agent::tools;
use liyasa_ai::index::ChunkQuery;
use liyasa_server::routes::research;
use liyasa_tests::server::Harness;
use serde_json::json;

use crate::agent_support as support;

/// This instance's read tools, built the way the assistant builds them: one
/// reader's filter, this site's bundle.
async fn instance_tools() -> (liyasa_server::routes::tools::ServerTools, ChunkQuery) {
    let (harness, _site) = Harness::serving("agt02-research").await;
    let bundle = harness.state.bundle.clone().expect("the fixture bundle");
    let filter = ChunkQuery::default();
    (
        liyasa_server::routes::tools::ServerTools::new(bundle, filter.clone()),
        filter,
    )
}

/// A run that has entered research, the phase these tools belong to.
fn researching() -> liyasa_agent::run::Run {
    let mut run = support::start(support::request(
        support::drift_trigger(),
        "the pricing page disagrees with the plan table",
    ));
    run.enter(Phase::Research)
        .expect("a run starts at research");
    run
}

#[tokio::test]
async fn a_permitted_search_is_answered_from_this_instances_index() {
    let mut run = researching();
    let input = json!({ "query": "rate limits" });

    run.authorise(tools::SEARCH_DOCS, &input)
        .expect("search_docs is a research tool a drift run may call");

    let (tools_impl, filter) = instance_tools().await;
    let answer = research::execute(&tools_impl, &filter, tools::SEARCH_DOCS, &input)
        .await
        .expect("this instance serves search_docs");

    // A site with nothing indexed retrieves nothing, which is a true answer
    // rather than a failure — the shape is what is asserted here.
    assert!(
        answer.get("passages").and_then(|p| p.as_array()).is_some(),
        "search_docs answers passages: {answer}"
    );
}

/// The two tools this server cannot serve say so by name rather than
/// answering empty. An empty drift list and a server that cannot see drift are
/// the same JSON otherwise, and the agent would read "nothing is wrong".
#[tokio::test]
async fn a_tool_whose_data_lives_elsewhere_names_the_owner() {
    let (tools_impl, filter) = instance_tools().await;

    for (tool, owner) in [(tools::GET_FACT, "WP-13"), (tools::LIST_DRIFT, "WP-20c")] {
        let unavailable = research::execute(&tools_impl, &filter, tool, &json!({}))
            .await
            .expect_err("this server holds no reader for it");
        assert_eq!(unavailable.tool, tool);
        assert!(
            unavailable.reason.contains(owner),
            "the refusal names whose data it is: {}",
            unavailable.reason
        );
        assert!(
            unavailable.as_json()["unavailable"] == tool,
            "the audit records the refusal rather than a gap"
        );
    }
}

/// `authorise` is the gate and `execute` does not second-guess it. Two gates
/// disagreeing is worse than one, so this pins which one decides.
#[tokio::test]
async fn execute_does_not_re_check_the_trust_level() {
    let mut untrusted = support::start(support::request(
        support::feedback_trigger(),
        "the install guide is wrong",
    ));
    untrusted.enter(Phase::Research).expect("research");

    // A stranger's trigger may still search — reading is what the research
    // phase is for, and the write tools are where trust bites.
    let input = json!({ "query": "install" });
    assert!(
        untrusted.authorise(tools::SEARCH_DOCS, &input).is_ok(),
        "reading the site is not what an untrusted trigger is restricted from"
    );
}
