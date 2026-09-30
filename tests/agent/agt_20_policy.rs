//! AGT-20 — Given policies `proposal`, `automerge-if-verified`, and `direct`; when
//! applied to a `member` run and an `anonymous` run; then the anonymous run always
//! ends in review regardless of policy, and `direct` respects branch protection.

use liyasa_agent::policy::{Outcome, Policy, Signals, decide};
use liyasa_agent::trust::{Restrictions, Trigger, TriggerKind, TrustLevel};

const POLICIES: [Policy; 3] = [
    Policy::Proposal,
    Policy::AutomergeIfVerified,
    Policy::Direct,
];

fn member() -> Restrictions {
    Restrictions::for_run(
        TrustLevel::Member,
        &Trigger::new(TriggerKind::Prompt, TrustLevel::Member),
    )
}

fn anonymous() -> Restrictions {
    Restrictions::for_run(
        TrustLevel::Anonymous,
        &Trigger::new(TriggerKind::Feedback, TrustLevel::Anonymous),
    )
}

fn everything_passes() -> Signals {
    Signals {
        validate_passed: true,
        verify_passed: true,
        ci_passed: true,
        gate_passed: true,
        within_limits: true,
        admin_enabled_direct: true,
        branch_protection_permits: true,
    }
}

/// Every combination of the seven signals.
fn all_signals() -> impl Iterator<Item = Signals> {
    (0u8..128).map(|bits| Signals {
        validate_passed: bits & 1 != 0,
        verify_passed: bits & 2 != 0,
        ci_passed: bits & 4 != 0,
        gate_passed: bits & 8 != 0,
        within_limits: bits & 16 != 0,
        admin_enabled_direct: bits & 32 != 0,
        branch_protection_permits: bits & 64 != 0,
    })
}

#[test]
fn a_member_run_reaches_what_each_policy_says() {
    assert_eq!(
        decide(Policy::Proposal, &member(), &everything_passes()).outcome,
        Outcome::Proposal
    );
    assert_eq!(
        decide(Policy::AutomergeIfVerified, &member(), &everything_passes()).outcome,
        Outcome::Automerge
    );
    assert_eq!(
        decide(Policy::Direct, &member(), &everything_passes()).outcome,
        Outcome::Direct
    );
}

#[test]
fn an_anonymous_run_ends_in_review_under_every_policy_and_every_signal() {
    // "Unavailable, not merely off … regardless of configuration" is a claim about
    // all 384 of these.
    let mut cases = 0usize;
    for policy in POLICIES {
        for signals in all_signals() {
            let decision = decide(policy, &anonymous(), &signals);
            assert!(
                decision.ends_in_review(),
                "{policy:?} with {signals:?} reached {:?}",
                decision.outcome
            );
            cases += 1;
        }
    }
    assert_eq!(cases, 3 * 128);
}

#[test]
fn the_reason_an_anonymous_run_was_downgraded_is_its_trust_and_not_a_check() {
    let decision = decide(Policy::Direct, &anonymous(), &everything_passes());
    let reason = decision.downgraded.expect("a reason");
    assert!(reason.contains("untrusted-trigger"), "{reason}");
    assert!(!reason.contains("validation"), "{reason}");
    assert!(!reason.contains("branch protection"), "{reason}");
}

#[test]
fn direct_respects_branch_protection() {
    let mut signals = everything_passes();
    signals.branch_protection_permits = false;
    let decision = decide(Policy::Direct, &member(), &signals);
    assert_eq!(decision.outcome, Outcome::Proposal);
    assert!(
        decision
            .downgraded
            .as_deref()
            .expect("a reason")
            .contains("branch protection"),
        "{decision:?}"
    );
}

#[test]
fn direct_needs_an_admin_to_have_enabled_it() {
    let mut signals = everything_passes();
    signals.admin_enabled_direct = false;
    let decision = decide(Policy::Direct, &member(), &signals);
    assert_eq!(decision.outcome, Outcome::Proposal);
    assert!(
        decision
            .downgraded
            .as_deref()
            .expect("a reason")
            .contains("admin"),
        "{decision:?}"
    );
}

#[test]
fn automerge_needs_every_one_of_the_four_checks_and_the_limits() {
    for (label, mutate) in [
        ("validation", 0usize),
        ("verification", 1),
        ("CI", 2),
        ("the output gates", 3),
        ("the size limits", 4),
    ] {
        let mut signals = everything_passes();
        match mutate {
            0 => signals.validate_passed = false,
            1 => signals.verify_passed = false,
            2 => signals.ci_passed = false,
            3 => signals.gate_passed = false,
            _ => signals.within_limits = false,
        }
        let decision = decide(Policy::AutomergeIfVerified, &member(), &signals);
        assert_eq!(decision.outcome, Outcome::Proposal, "{label}");
        assert!(
            decision
                .downgraded
                .as_deref()
                .expect("a reason")
                .contains(label),
            "{label} was not named: {decision:?}"
        );
    }
}

#[test]
fn only_proposal_is_even_available_to_an_anonymous_run() {
    assert_eq!(Policy::available_to(&anonymous()), &[Policy::Proposal]);
    assert_eq!(Policy::available_to(&member()), &POLICIES);
}

#[test]
fn every_trigger_the_prd_calls_untrusted_always_ends_in_review() {
    // "feedback-, ticket-, chat-, and pull-request-body-triggered runs always end in
    // a human review."
    for kind in [
        TriggerKind::Feedback,
        TriggerKind::SupportTicket,
        TriggerKind::PullRequest,
        TriggerKind::AssistantGap,
    ] {
        for trust in [TrustLevel::Anonymous, TrustLevel::External] {
            let restrictions = Restrictions::for_run(trust, &Trigger::new(kind, trust));
            for policy in POLICIES {
                assert!(
                    decide(policy, &restrictions, &everything_passes()).ends_in_review(),
                    "{kind:?} at {trust:?} under {policy:?} did not end in review"
                );
            }
        }
    }
}

#[test]
fn proposal_is_the_default_policy() {
    assert_eq!(Policy::default(), Policy::Proposal);
}
