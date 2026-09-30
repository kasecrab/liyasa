//! What happens to a run's output (AGT-20).
//!
//! Three policies, and the third sentence of AGT-20 is the one this module exists
//! for: `automerge-if-verified` and `direct` are "**unavailable, not merely off**,
//! for untrusted-trigger runs … the policy engine enforces this regardless of
//! configuration".
//!
//! So [`decide`] does not consult a key that could say otherwise. It narrows the
//! configured policy to [`Policy::available_to`] first, and that function reads
//! nothing but [`Restrictions::may_automerge`], which is derived from the run's
//! trust level and cannot be set. A test walks every combination of every signal
//! against every policy and asserts that an untrusted-trigger run reaches
//! [`Outcome::Proposal`] in all of them — 384 cases, because "enforced regardless
//! of configuration" is a claim about all of them and not about the one a hand
//! test would pick.
//!
//! The signals are the caller's to gather. This module decides, and it says why:
//! a [`Decision`] that was downgraded carries the policy it was downgraded from
//! and the sentence a reviewer reads.

use serde::{Deserialize, Serialize};

use crate::trust::Restrictions;

/// `ai.agent.policy`, or an automation's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Policy {
    /// Always open a pull request or a workspace draft for review. The default.
    Proposal,
    /// Merge when validation, verification, CI and the output gates pass and the
    /// change is within limits.
    AutomergeIfVerified,
    /// Push to the deploy branch. Only with an admin's consent and branch
    /// protection's.
    Direct,
}

impl Default for Policy {
    fn default() -> Self {
        Self::Proposal
    }
}

impl Policy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Policy::Proposal => "proposal",
            Policy::AutomergeIfVerified => "automerge-if-verified",
            Policy::Direct => "direct",
        }
    }

    /// The policies a run at these restrictions may reach at all.
    ///
    /// Not "the policies that are switched on": a policy absent from here cannot
    /// be configured back, and there is no argument to this function that would
    /// let it.
    pub fn available_to(restrictions: &Restrictions) -> &'static [Policy] {
        if restrictions.may_automerge() {
            &[
                Policy::Proposal,
                Policy::AutomergeIfVerified,
                Policy::Direct,
            ]
        } else {
            &[Policy::Proposal]
        }
    }

    /// Whether a run at these restrictions may end in this policy.
    pub fn is_available_to(self, restrictions: &Restrictions) -> bool {
        Self::available_to(restrictions).contains(&self)
    }
}

/// What the caller has established about the run's output.
///
/// Every field defaults to the unsatisfied value, so a caller that forgets to set
/// one gets a proposal rather than a merge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Signals {
    pub validate_passed: bool,
    pub verify_passed: bool,
    pub ci_passed: bool,
    /// The output gate produced no rejection (AGT-06).
    pub gate_passed: bool,
    /// The change is inside `ai.agent.limits`.
    pub within_limits: bool,
    /// An admin turned `direct` on for this project.
    pub admin_enabled_direct: bool,
    /// Branch protection on the deploy branch permits this push.
    pub branch_protection_permits: bool,
}

impl Signals {
    /// Everything `automerge-if-verified` asks for.
    pub const fn verified(self) -> bool {
        self.validate_passed
            && self.verify_passed
            && self.ci_passed
            && self.gate_passed
            && self.within_limits
    }

    /// Reads the gate's own verdict, so the caller cannot report a gate it did
    /// not run.
    #[must_use]
    pub fn from_gate(mut self, report: &crate::gates::Report) -> Self {
        self.gate_passed = !report.rejected();
        self
    }
}

/// What a run's output does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// A branch and a pull request, or a workspace draft.
    Proposal,
    /// Merged without a person.
    Automerge,
    /// Pushed to the deploy branch.
    Direct,
}

/// The decision, with the reason it is not what was configured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub outcome: Outcome,
    pub configured: Policy,
    /// `None` when the configured policy was honoured.
    pub downgraded: Option<String>,
}

impl Decision {
    pub const fn ends_in_review(&self) -> bool {
        matches!(self.outcome, Outcome::Proposal)
    }
}

/// Applies AGT-20.
pub fn decide(configured: Policy, restrictions: &Restrictions, signals: &Signals) -> Decision {
    let downgrade = |reason: String| Decision {
        outcome: Outcome::Proposal,
        configured,
        downgraded: Some(reason),
    };

    if !configured.is_available_to(restrictions) {
        return downgrade(format!(
            "`{}` is unavailable to a run triggered by {} at `{:?}` trust: \
             an untrusted-trigger run always ends in a human review (AGT-20)",
            configured.as_str(),
            restrictions.trigger().as_str(),
            restrictions.trust()
        ));
    }

    match configured {
        Policy::Proposal => Decision {
            outcome: Outcome::Proposal,
            configured,
            downgraded: None,
        },
        Policy::AutomergeIfVerified if signals.verified() => Decision {
            outcome: Outcome::Automerge,
            configured,
            downgraded: None,
        },
        Policy::AutomergeIfVerified => downgrade(format!(
            "`automerge-if-verified` needs every check to pass: {}",
            unmet(signals).join(", ")
        )),
        Policy::Direct if !signals.admin_enabled_direct => {
            downgrade("`direct` needs an admin to enable it for this project".to_owned())
        }
        Policy::Direct if !signals.branch_protection_permits => {
            downgrade("branch protection on the deploy branch does not permit this push".to_owned())
        }
        Policy::Direct if !signals.verified() => downgrade(format!(
            "`direct` still needs every check to pass: {}",
            unmet(signals).join(", ")
        )),
        Policy::Direct => Decision {
            outcome: Outcome::Direct,
            configured,
            downgraded: None,
        },
    }
}

/// The checks that did not pass, named as a reviewer would name them.
fn unmet(signals: &Signals) -> Vec<&'static str> {
    let mut out = Vec::new();
    if !signals.validate_passed {
        out.push("validation");
    }
    if !signals.verify_passed {
        out.push("verification");
    }
    if !signals.ci_passed {
        out.push("CI");
    }
    if !signals.gate_passed {
        out.push("the output gates");
    }
    if !signals.within_limits {
        out.push("the size limits");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust::{Trigger, TriggerKind, TrustLevel};

    fn member() -> Restrictions {
        Restrictions::for_run(
            TrustLevel::Member,
            &Trigger::new(TriggerKind::Prompt, TrustLevel::Member),
        )
    }

    fn stranger(kind: TriggerKind, trust: TrustLevel) -> Restrictions {
        Restrictions::for_run(trust, &Trigger::new(kind, trust))
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
    fn proposal_is_the_default() {
        assert_eq!(Policy::default(), Policy::Proposal);
    }

    #[test]
    fn a_member_run_with_everything_passing_automerges() {
        let decision = decide(Policy::AutomergeIfVerified, &member(), &everything_passes());
        assert_eq!(decision.outcome, Outcome::Automerge);
        assert_eq!(decision.downgraded, None);
    }

    #[test]
    fn a_member_run_with_one_check_failing_ends_in_review_and_names_it() {
        for (name, mut signals) in [
            ("validation", everything_passes()),
            ("verification", everything_passes()),
            ("CI", everything_passes()),
            ("the output gates", everything_passes()),
            ("the size limits", everything_passes()),
        ] {
            match name {
                "validation" => signals.validate_passed = false,
                "verification" => signals.verify_passed = false,
                "CI" => signals.ci_passed = false,
                "the output gates" => signals.gate_passed = false,
                _ => signals.within_limits = false,
            }
            let decision = decide(Policy::AutomergeIfVerified, &member(), &signals);
            assert_eq!(decision.outcome, Outcome::Proposal, "{name}");
            let reason = decision.downgraded.expect("a reason");
            assert!(reason.contains(name), "{reason}");
        }
    }

    #[test]
    fn direct_needs_an_admin_and_then_branch_protection() {
        let mut signals = everything_passes();
        signals.admin_enabled_direct = false;
        let decision = decide(Policy::Direct, &member(), &signals);
        assert_eq!(decision.outcome, Outcome::Proposal);
        assert!(
            decision
                .downgraded
                .as_deref()
                .unwrap_or_default()
                .contains("admin"),
            "{decision:?}"
        );

        signals.admin_enabled_direct = true;
        signals.branch_protection_permits = false;
        let decision = decide(Policy::Direct, &member(), &signals);
        assert_eq!(decision.outcome, Outcome::Proposal);
        assert!(
            decision
                .downgraded
                .as_deref()
                .unwrap_or_default()
                .contains("branch protection"),
            "{decision:?}"
        );
    }

    #[test]
    fn direct_with_an_admin_and_branch_protection_and_every_check_pushes() {
        let decision = decide(Policy::Direct, &member(), &everything_passes());
        assert_eq!(decision.outcome, Outcome::Direct);
        assert_eq!(decision.downgraded, None);
    }

    #[test]
    fn an_untrusted_trigger_run_ends_in_review_for_every_policy_and_every_signal() {
        // "unavailable, not merely off … regardless of configuration" is a claim
        // about all 384 of these, not about the one a hand test would pick.
        let mut cases = 0usize;
        for kind in [
            TriggerKind::Feedback,
            TriggerKind::SupportTicket,
            TriggerKind::PullRequest,
            TriggerKind::AssistantGap,
        ] {
            for trust in [TrustLevel::Anonymous, TrustLevel::External] {
                let restrictions = stranger(kind, trust);
                for policy in [
                    Policy::Proposal,
                    Policy::AutomergeIfVerified,
                    Policy::Direct,
                ] {
                    for signals in all_signals() {
                        let decision = decide(policy, &restrictions, &signals);
                        assert!(
                            decision.ends_in_review(),
                            "{kind:?} at {trust:?} under {policy:?} reached {:?}",
                            decision.outcome
                        );
                        cases += 1;
                    }
                }
            }
        }
        assert_eq!(cases, 4 * 2 * 3 * 128);
    }

    #[test]
    fn the_downgrade_reason_says_it_is_the_trust_level_and_not_a_failed_check() {
        // A reviewer who reads "verification did not pass" goes and fixes
        // verification. This one cannot be fixed, and the reason has to say so.
        let decision = decide(
            Policy::AutomergeIfVerified,
            &stranger(TriggerKind::Feedback, TrustLevel::Anonymous),
            &everything_passes(),
        );
        let reason = decision.downgraded.expect("a reason");
        assert!(reason.contains("untrusted-trigger"), "{reason}");
        assert!(reason.contains("feedback item"), "{reason}");
        assert!(!reason.contains("validation"), "{reason}");
    }

    #[test]
    fn only_proposal_is_available_to_a_stranger() {
        let restrictions = stranger(TriggerKind::Feedback, TrustLevel::External);
        assert_eq!(Policy::available_to(&restrictions), &[Policy::Proposal]);
        assert!(!Policy::AutomergeIfVerified.is_available_to(&restrictions));
        assert!(!Policy::Direct.is_available_to(&restrictions));
    }

    #[test]
    fn the_gate_signal_is_read_off_the_gates_own_report() {
        // So a caller cannot report a gate that passed without running one.
        use crate::diff::{Diff, FileChange};
        use crate::gates::{Inputs, check};
        let diff = Diff::new([FileChange::added(
            "guides/p.md",
            "---\ntitle: t\n---\n\nUse AKIAQWERTYUIOPASDFGH.\n",
        )]);
        let restrictions = member();
        let limits = crate::config::Limits::default();
        let layout = crate::scope::Layout::default();
        let known = crate::hosts::KnownHosts::default();
        let detector = crate::injection::Detector::default();
        let report = check(Inputs {
            diff: &diff,
            restrictions: &restrictions,
            limits: &limits,
            layout: &layout,
            known_hosts: &known,
            injection: &detector,
            allow_bulk_delete: false,
        });
        let signals = everything_passes().from_gate(&report);
        assert!(!signals.gate_passed);
        assert_eq!(
            decide(Policy::AutomergeIfVerified, &restrictions, &signals).outcome,
            Outcome::Proposal
        );
    }

    #[test]
    fn a_default_signals_gets_a_proposal_even_under_direct() {
        // Every field defaults to unsatisfied, so forgetting to gather them is
        // safe rather than a merge.
        let decision = decide(Policy::Direct, &member(), &Signals::default());
        assert_eq!(decision.outcome, Outcome::Proposal);
    }

    #[test]
    fn the_policy_names_are_the_ones_the_prd_uses() {
        assert_eq!(Policy::Proposal.as_str(), "proposal");
        assert_eq!(
            Policy::AutomergeIfVerified.as_str(),
            "automerge-if-verified"
        );
        assert_eq!(Policy::Direct.as_str(), "direct");
        assert_eq!(
            serde_json::to_string(&Policy::AutomergeIfVerified).expect("serializes"),
            "\"automerge-if-verified\""
        );
    }
}
