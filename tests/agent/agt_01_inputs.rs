//! AGT-01 — Given a run triggered by a drift record; when it starts; then the run
//! record lists the task, trust level, content tree snapshot, graph access, context
//! repos, and policy.

use liyasa_agent::policy::Policy;
use liyasa_agent::record::GraphAccess;
use liyasa_agent::trust::{TriggerKind, TrustLevel, WriteScope};
use liyasa_core::ids::Route;

use crate::agent_support as support;

#[test]
fn a_drift_triggered_run_records_every_input_agt_01_names() {
    let run = support::start(support::request(
        support::drift_trigger(),
        "the seat limit changed from 5 to 10",
    ));
    let header = run.record().header();

    // The task.
    assert_eq!(header.task.trigger, TriggerKind::Drift);
    assert_eq!(header.task.text, "the seat limit changed from 5 to 10");
    assert_eq!(header.task.pages, [Route::new("/pricing")].into());

    // The trust level.
    assert_eq!(header.trust, TrustLevel::Member);

    // The content tree snapshot.
    assert_eq!(header.content_tree.as_deref(), Some("blake3:9f2c4e"));

    // Graph access.
    assert_eq!(header.graph, GraphAccess::ReadAndPropose);

    // The context repositories.
    assert_eq!(header.context_repos, ["acme/api", "acme/cli"]);

    // The policy.
    assert_eq!(header.policy, Policy::Proposal);
}

#[test]
fn the_record_lists_each_input_with_its_own_trust_level() {
    // "Every input to a run carries a trust level" (AGT-04) is only inspectable if
    // the record keeps them apart rather than keeping the minimum.
    let run = support::start(support::request(support::drift_trigger(), "a task"));
    let header = run.record().header();
    assert!(header.inputs.len() >= 6, "{:?}", header.inputs);
    assert!(
        header
            .inputs
            .iter()
            .any(|i| i.trust == TrustLevel::Operator),
        "AGENTS.md is operator text and the record should say so: {:?}",
        header.inputs
    );
    assert!(
        header.inputs.iter().any(|i| i.trust == TrustLevel::Member),
        "{:?}",
        header.inputs
    );
}

#[test]
fn the_record_lists_the_write_scope_the_trust_level_produced() {
    // Not in AGT-01's list, and the record is useless for review without it: the
    // trust level is the reason and the scope is the consequence.
    let drift = support::start(support::request(support::drift_trigger(), "a task"));
    assert_eq!(drift.record().header().scope, WriteScope::Anywhere);

    let feedback = support::start(support::request(support::feedback_trigger(), "a task"));
    assert_eq!(
        feedback.record().header().scope,
        WriteScope::Pages([Route::new("/guides/install")].into())
    );
}

#[test]
fn an_untrusted_trigger_lowers_the_whole_runs_level() {
    // The same six trusted inputs, one untrusted trigger.
    let run = support::start(support::request(
        support::feedback_trigger(),
        "this page is wrong",
    ));
    assert_eq!(run.record().header().trust, TrustLevel::Anonymous);
    assert!(run.restrictions().is_untrusted_trigger());
    assert!(!run.restrictions().may_automerge());
}

#[test]
fn the_record_is_readable_back_out_of_json() {
    // It is stored for review and debugging (AGT-05), so it has to survive that.
    let run = support::start(support::request(support::drift_trigger(), "a task"));
    let text = serde_json::to_string(run.record()).expect("serializes");
    let back: liyasa_agent::record::RunRecord = serde_json::from_str(&text).expect("reads back");
    assert_eq!(back.header(), run.record().header());
}
