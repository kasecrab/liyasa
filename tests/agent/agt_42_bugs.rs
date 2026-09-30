//! AGT-42 — Given a code sample that fails against staging in the mock environment;
//! when the agent researches; then a bug record with evidence is filed and, per
//! policy, an issue is opened in the connected tracker (recorded API).

use liyasa_agent::bug::{BugRecord, Evidence, IssuePolicy, Kind, NotFiled, Tracker};
use liyasa_agent::record::Phase;
use liyasa_agent::trust::{Restrictions, Trigger, TriggerKind, TrustLevel};
use liyasa_core::ids::Route;

use crate::agent_support as support;

/// The mock staging environment: a recorded request and the response it gave.
///
/// A real one goes out through `liyasa_core::net::HttpClient` under an allow list,
/// and the run that would do it is AGT-02's. What AGT-42 is about is what happens
/// to the finding, so the environment is the recording.
struct MockStaging {
    request: &'static str,
    status: u16,
    body: &'static str,
}

const FAILING_SAMPLE: MockStaging = MockStaging {
    request: "POST https://staging.acme.example/v1/seats",
    status: 422,
    body: "{\"error\":\"`plan` is required\"}",
};

const WORKING_SAMPLE: MockStaging = MockStaging {
    request: "GET https://staging.acme.example/v1/plans",
    status: 200,
    body: "{\"plans\":[\"free\",\"pro\"]}",
};

/// What the agent does when a sample fails: file a record, with the exchange as
/// evidence.
fn file_for(sample: &MockStaging, page: &str) -> Option<BugRecord> {
    if sample.status < 400 {
        return None;
    }
    BugRecord::file(
        Kind::FailingSample,
        "the seats sample is missing `plan`",
        Some(Route::new(page)),
        [Evidence::new(
            sample.request,
            format!("{} — {}", sample.status, sample.body),
            "the sample on this page sends no `plan` and expects a 201",
        )],
    )
    .ok()
}

fn member() -> Restrictions {
    Restrictions::for_run(
        TrustLevel::Member,
        &Trigger::new(TriggerKind::Prompt, TrustLevel::Member),
    )
}

fn stranger() -> Restrictions {
    Restrictions::for_run(
        TrustLevel::Anonymous,
        &Trigger::new(TriggerKind::Feedback, TrustLevel::Anonymous),
    )
}

#[test]
fn a_failing_sample_produces_a_bug_record_with_the_exchange_as_evidence() {
    let record = file_for(&FAILING_SAMPLE, "/guides/seats").expect("a record was filed");
    assert_eq!(record.kind, Kind::FailingSample);
    assert_eq!(record.page, Some(Route::new("/guides/seats")));
    assert_eq!(record.evidence().len(), 1);
    let evidence = &record.evidence()[0];
    assert!(
        evidence.source.contains("staging.acme.example"),
        "{evidence:?}"
    );
    assert!(evidence.observed.contains("422"), "{evidence:?}");
    assert!(evidence.expected.contains("201"), "{evidence:?}");
}

#[test]
fn a_working_sample_produces_no_record() {
    // A detector that fires on everything is not a detector.
    assert!(file_for(&WORKING_SAMPLE, "/guides/plans").is_none());
}

#[test]
fn a_record_cannot_be_filed_without_evidence() {
    assert_eq!(
        BugRecord::file(Kind::FailingSample, "something is wrong", None, []),
        Err(NotFiled::NoEvidence)
    );
}

#[test]
fn the_issue_body_carries_the_evidence_and_the_page_it_came_from() {
    let body = file_for(&FAILING_SAMPLE, "/guides/seats")
        .expect("filed")
        .issue_body();
    assert!(body.contains("staging.acme.example"), "{body}");
    assert!(body.contains("`plan` is required"), "{body}");
    assert!(body.contains("/guides/seats"), "{body}");
    assert!(
        body.contains("has not changed any page"),
        "a maintainer has to know the docs were not edited on this basis:\n{body}"
    );
}

#[test]
fn an_issue_is_opened_only_when_the_policy_says_so() {
    let record = file_for(&FAILING_SAMPLE, "/guides/seats").expect("filed");
    assert!(!record.may_open_issue(IssuePolicy::RecordOnly, &member()));
    assert!(record.may_open_issue(IssuePolicy::Open, &member()));
}

#[test]
fn a_stranger_triggered_run_files_the_record_and_never_opens_the_issue() {
    // An issue is an outbound action in the project's name; the record is internal.
    let record = file_for(&FAILING_SAMPLE, "/guides/seats").expect("filed");
    for policy in [IssuePolicy::RecordOnly, IssuePolicy::Open] {
        assert!(
            !record.may_open_issue(policy, &stranger()),
            "an anonymous run reached the tracker under {policy:?}"
        );
    }
}

#[test]
fn every_kind_of_product_problem_agt_42_names_can_be_filed() {
    for (kind, title) in [
        (Kind::FailingSample, "the seats sample fails"),
        (
            Kind::SpecContradictsCode,
            "the spec says `plan` is optional and the code requires it",
        ),
        (Kind::DeadIntegration, "the Slack integration returns 410"),
    ] {
        let record = BugRecord::file(
            kind,
            title,
            None,
            [Evidence::new("a source", "what happened", "what should")],
        )
        .unwrap_or_else(|e| panic!("{kind:?} could not be filed: {e}"));
        assert_eq!(record.kind, kind);
    }
}

#[test]
fn a_secret_in_a_recorded_response_never_reaches_the_tracker() {
    // A failing authenticated request is exactly where one turns up, and this body
    // goes to a third party.
    let record = BugRecord::file(
        Kind::FailingSample,
        "the sample fails",
        None,
        [Evidence::new(
            "GET https://staging.acme.example/v1/seats",
            "401 {\"echo\":\"AKIAQWERTYUIOPASDFGH\"}",
            "a 200",
        )],
    )
    .expect("filed");
    let body = record.issue_body();
    assert!(!body.contains("AKIAQWERTYUIOPASDFGH"), "{body}");
    assert!(body.contains("[redacted]"), "{body}");
}

#[test]
fn every_tracker_agt_42_names_exists() {
    for tracker in [Tracker::GitHub, Tracker::Linear, Tracker::Jira] {
        let name = serde_json::to_string(&tracker).expect("serializes");
        assert!(name.len() > 2, "{name}");
    }
}

#[test]
fn a_bug_found_in_research_does_not_stop_the_run_from_writing() {
    // AGT-42 is a side channel, not a failure: a page that is wrong for a reason
    // the agent cannot fix still gets whatever it can fix.
    let mut run = support::start(support::request(
        support::drift_trigger(),
        "the seats sample fails",
    ));
    run.enter(Phase::Research).expect("research");
    let record = file_for(&FAILING_SAMPLE, "/guides/seats").expect("filed");
    run.record_mut()
        .note(&format!("filed a bug record: {}", record.title));
    run.enter(Phase::Plan).expect("plan");
    run.enter(Phase::Write).expect("write");
    run.write(
        &Route::new("/pricing"),
        "pricing.md",
        support::page("Ten seats, or twenty on Pro."),
        Some(support::page("Ten seats.")),
    )
    .expect("the run carries on");
    assert_eq!(run.diff().files_changed(), 1);
    let text = serde_json::to_string(run.record()).expect("serializes");
    assert!(text.contains("filed a bug record"), "{text}");
}
