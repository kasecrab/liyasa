//! AGT-03 — Given each tool's JSON Schema; when called with invalid input or outside
//! its `min_trust`; then the call is rejected with `E0903` and audited.

use liyasa_agent::dispatch::Rejection;
use liyasa_agent::record::{CallOutcome, Phase};
use liyasa_agent::tools;
use liyasa_agent::trust::TrustLevel;
use liyasa_core::diagnostics::code;
use serde_json::{Value, json};

use crate::agent_support as support;

/// A valid input for each of the sixteen tools.
fn valid(name: &str) -> Value {
    match name {
        tools::SEARCH_DOCS => json!({ "query": "seats" }),
        tools::READ_PAGE => json!({ "route": "/pricing" }),
        tools::WRITE_PAGE => json!({ "route": "/pricing", "markdown": "# Pricing\n" }),
        tools::MOVE_PAGE => json!({ "from": "/pricing", "to": "/plans" }),
        tools::EDIT_NAVIGATION => {
            json!({ "operation": "rename", "route": "/pricing", "title": "Plans" })
        }
        tools::READ_REPO_FILE => json!({ "repo": "acme/api", "path": "src/seats.rs" }),
        tools::SEARCH_REPO => json!({ "repo": "acme/api", "query": "seat_limit" }),
        tools::GET_OPENAPI => json!({ "operation": "GET /seats" }),
        tools::GET_FACT => json!({ "fact": "plan.seats" }),
        tools::PROPOSE_FACT_UPDATE => {
            json!({ "fact": "plan.seats", "value": 10, "evidence": "the API returns 10" })
        }
        tools::RUN_VALIDATE => json!({}),
        tools::RUN_VERIFY => json!({ "changed_only": true }),
        tools::RENDER_PREVIEW => json!({ "route": "/pricing" }),
        tools::LIST_DRIFT => json!({ "open": true }),
        tools::WEB_FETCH => json!({ "url": "https://docs.acme.example/changelog" }),
        tools::ASK_REVIEWER => json!({ "question": "should the old limit stay?" }),
        other => panic!("`{other}` has no fixture in this test"),
    }
}

/// Inputs every tool's schema must refuse: an undeclared key, and nothing at all
/// where something is required.
fn invalid(name: &str) -> Vec<Value> {
    let mut out = vec![{
        let mut base = valid(name);
        base.as_object_mut()
            .expect("an object")
            .insert("nope".to_owned(), json!(1));
        base
    }];
    let spec = tools::spec(name).expect("a tool");
    if spec.input_schema["required"]
        .as_array()
        .is_some_and(|r| !r.is_empty())
    {
        out.push(json!({}));
    }
    out
}

#[test]
fn every_tool_rejects_an_undeclared_key_with_e0903_and_audits_the_call() {
    let mut run = support::start(support::request(support::drift_trigger(), "a task"));
    // An operator-level run, so `min_trust` is not what refuses anything here.
    let run_operator = &mut run;
    let mut audited = 0usize;
    let mut checked = 0usize;
    for name in tools::NAMES {
        for bad in invalid(name) {
            let before = run_operator.record().call_count();
            let rejection = run_operator
                .authorise(name, &bad)
                .expect_err(&format!("`{name}` accepted {bad}"));
            assert!(
                matches!(rejection, Rejection::InvalidInput { .. }),
                "`{name}` with {bad} was refused for the wrong reason: {rejection:?}"
            );
            assert_eq!(rejection.diagnostic().code, code::E0903);
            assert_eq!(
                run_operator.record().call_count(),
                before + 1,
                "`{name}` was refused without being audited"
            );
            audited += 1;
            checked += 1;
        }
    }
    assert!(checked >= tools::NAMES.len(), "{checked} inputs checked");
    assert_eq!(audited, checked);
}

#[test]
fn a_call_below_min_trust_is_rejected_with_e0903_and_audited() {
    let mut run = support::start(support::request(
        support::feedback_trigger(),
        "this page is wrong",
    ));
    assert_eq!(run.restrictions().trust(), TrustLevel::Anonymous);
    let restricted: Vec<&str> = tools::specs()
        .iter()
        .filter(|s| s.min_trust == TrustLevel::Member)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(restricted.len(), 6, "{restricted:?}");
    for name in restricted {
        let before = run.record().call_count();
        let rejection = run
            .authorise(name, &valid(name))
            .expect_err(&format!("`{name}` was offered to an anonymous run"));
        assert!(
            matches!(rejection, Rejection::BelowTrust { .. }),
            "`{name}`: {rejection:?}"
        );
        assert_eq!(rejection.diagnostic().code, code::E0903);
        assert_eq!(run.record().call_count(), before + 1);
    }
}

#[test]
fn the_audit_entry_says_which_tool_and_why() {
    let mut run = support::start(support::request(support::feedback_trigger(), "wrong"));
    run.authorise(tools::EDIT_NAVIGATION, &valid(tools::EDIT_NAVIGATION))
        .expect_err("below min_trust");
    let (name, _, outcome) = run.record().calls().last().expect("an audit entry");
    assert_eq!(name, tools::EDIT_NAVIGATION);
    match outcome {
        CallOutcome::Rejected { reason } => {
            assert!(reason.contains("edit_navigation"), "{reason}");
            assert!(reason.contains("trust"), "{reason}");
        }
        other => panic!("the call was audited as {other:?}"),
    }
}

#[test]
fn a_valid_call_at_sufficient_trust_is_authorised_and_audited() {
    let mut run = support::start(support::request(support::drift_trigger(), "a task"));
    support::to_write(&mut run);
    for name in tools::NAMES {
        let before = run.record().call_count();
        let spec = run
            .authorise(name, &valid(name))
            .unwrap_or_else(|e| panic!("`{name}` refused a valid call: {e}"));
        assert_eq!(spec.name, name);
        assert_eq!(run.record().call_count(), before + 1);
    }
}

#[test]
fn an_unknown_tool_name_is_rejected_with_e0903() {
    let mut run = support::start(support::request(support::drift_trigger(), "a task"));
    let rejection = run
        .authorise("run_shell", &json!({ "cmd": "ls" }))
        .expect_err("there is no such tool");
    assert!(
        matches!(rejection, Rejection::Unknown { .. }),
        "{rejection:?}"
    );
    assert_eq!(rejection.diagnostic().code, code::E0903);
}

#[test]
fn the_phase_a_call_was_made_in_is_in_the_record() {
    // "For review and debugging" (AGT-05): a call is only interpretable next to
    // the phase it was made in.
    let mut run = support::start(support::request(support::drift_trigger(), "a task"));
    run.enter(Phase::Research).expect("research");
    run.authorise(tools::SEARCH_DOCS, &valid(tools::SEARCH_DOCS))
        .expect("a search in research");
    let entries = run.record().entries();
    let phase_at = entries
        .iter()
        .position(|e| matches!(e, liyasa_agent::record::Entry::Phase { .. }))
        .expect("a phase entry");
    let call_at = entries
        .iter()
        .position(|e| matches!(e, liyasa_agent::record::Entry::ToolCall { .. }))
        .expect("a call entry");
    assert!(phase_at < call_at, "the call is not under a phase");
}
