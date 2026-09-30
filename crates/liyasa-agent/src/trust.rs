//! Trust levels, and what a run may do at each (AGT-04, §30.2.2).
//!
//! Two rules, and the second is the one that does the work.
//!
//! The first: every input to a run carries a level, and the run's policy is the
//! **minimum over its inputs**. [`TrustLevel`] is ordered most-trusted first, so
//! the minimum trust is the maximum value, which is why [`effective_trust`]
//! calls `max`. An operator's prompt that quotes a support ticket is not an
//! operator run.
//!
//! The second: a run whose effective level is `anonymous` or `external` is an
//! **untrusted-trigger run**, and its restrictions are not configuration. They
//! are computed here from the level and the trigger, and
//! [`Restrictions::permits`] is the only way to ask. There is no key that turns
//! them off, because the whole class of attack AGT-04 is about is a stranger
//! getting the agent to widen its own reach.
//!
//! The write scope of an untrusted-trigger run is the set of pages the trigger
//! named. An **empty** set therefore means *nothing is writable*, not
//! *everything is*. That reading is not an accident of the type: a feedback item
//! that names no page must not become a licence to rewrite the site, and
//! "empty means unrestricted" is the shape that would make it one.

use std::collections::BTreeSet;

use liyasa_core::ids::Route;

pub use liyasa_core::ai::TrustLevel;

use crate::scope::{ConfigArea, Target};

/// What an input to a run is (AGT-01 lists them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InputKind {
    /// The prompt or typed signal that started the run.
    Task,
    ContentTree,
    TruthGraph,
    ContextRepo,
    VerificationResults,
    StyleGuide,
    AgentsMd,
}

impl InputKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            InputKind::Task => "task",
            InputKind::ContentTree => "content tree",
            InputKind::TruthGraph => "truth graph",
            InputKind::ContextRepo => "context repository",
            InputKind::VerificationResults => "verification results",
            InputKind::StyleGuide => "style guide",
            InputKind::AgentsMd => "AGENTS.md",
        }
    }
}

/// One input, with how far its content may travel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub label: String,
    pub kind: InputKind,
    pub trust: TrustLevel,
}

impl Input {
    pub fn new(kind: InputKind, label: impl Into<String>, trust: TrustLevel) -> Self {
        Self {
            label: label.into(),
            kind,
            trust,
        }
    }
}

/// What started a run (AGT-01), and the pages it named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TriggerKind {
    /// A person typed a prompt.
    Prompt,
    Drift,
    PullRequest,
    SupportTicket,
    Feedback,
    AssistantGap,
}

impl TriggerKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            TriggerKind::Prompt => "prompt",
            TriggerKind::Drift => "drift record",
            TriggerKind::PullRequest => "pull request",
            TriggerKind::SupportTicket => "support ticket",
            TriggerKind::Feedback => "feedback item",
            TriggerKind::AssistantGap => "assistant gap",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    pub kind: TriggerKind,
    pub trust: TrustLevel,
    /// The pages the trigger is about: the page the feedback names, the pages
    /// the drift record lists. An untrusted-trigger run may write these and
    /// nothing else.
    pub pages: BTreeSet<Route>,
}

impl Trigger {
    pub fn new(kind: TriggerKind, trust: TrustLevel) -> Self {
        Self {
            kind,
            trust,
            pages: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn about(mut self, routes: impl IntoIterator<Item = Route>) -> Self {
        self.pages.extend(routes);
        self
    }
}

/// `anonymous` and `external` are the two untrusted levels (§30.2.2).
pub const fn is_untrusted(trust: TrustLevel) -> bool {
    matches!(trust, TrustLevel::Anonymous | TrustLevel::External)
}

/// The run's level: the minimum over its inputs, which for this ordering is the
/// maximum value.
///
/// A run with no inputs is `operator`. It cannot happen — a run has a task — and
/// if it did, the caller has supplied nothing untrusted.
pub fn effective_trust(inputs: &[Input]) -> TrustLevel {
    inputs
        .iter()
        .map(|i| i.trust)
        .max()
        .unwrap_or(TrustLevel::Operator)
}

/// Where a run may write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteScope {
    /// Any page in the project.
    Anywhere,
    /// These routes only. Empty means none.
    Pages(BTreeSet<Route>),
}

impl WriteScope {
    pub fn allows(&self, route: &Route) -> bool {
        match self {
            WriteScope::Anywhere => true,
            WriteScope::Pages(pages) => pages.contains(route),
        }
    }
}

/// Why a write was refused, worded as AGT-04 words it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Denial {
    #[error("`{route}` is not one of the pages this run's trigger named")]
    OutOfScope { route: Route },
    #[error("an untrusted-trigger run cannot change {what}")]
    Forbidden { what: &'static str },
    #[error("a link to `{host}` would be the first on this site")]
    NewHost { host: String },
    #[error("`{path}` is not a path inside this project")]
    NotInProject { path: String },
}

/// What a run may do, computed from its level and its trigger.
///
/// Every field is derived. There is no constructor that takes them, because a
/// configuration key that set `may_automerge` on an untrusted-trigger run is
/// exactly what AGT-20 says must be impossible rather than merely off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restrictions {
    trust: TrustLevel,
    trigger: TriggerKind,
    scope: WriteScope,
}

impl Restrictions {
    /// The restrictions for a run at `trust` started by `trigger`.
    pub fn for_run(trust: TrustLevel, trigger: &Trigger) -> Self {
        let scope = if is_untrusted(trust) {
            WriteScope::Pages(trigger.pages.clone())
        } else {
            WriteScope::Anywhere
        };
        Self {
            trust,
            trigger: trigger.kind,
            scope,
        }
    }

    /// The restrictions implied by a whole input list, whose least-trusted
    /// member sets the level.
    pub fn for_inputs(inputs: &[Input], trigger: &Trigger) -> Self {
        let trust = effective_trust(inputs).max(trigger.trust);
        Self::for_run(trust, trigger)
    }

    pub const fn trust(&self) -> TrustLevel {
        self.trust
    }

    pub const fn trigger(&self) -> TriggerKind {
        self.trigger
    }

    pub const fn scope(&self) -> &WriteScope {
        &self.scope
    }

    /// Whether this is an untrusted-trigger run in AGT-04's sense.
    pub const fn is_untrusted_trigger(&self) -> bool {
        is_untrusted(self.trust)
    }

    /// Whether a proposal from this run may ever be merged without a person.
    ///
    /// `false` for every untrusted-trigger run, whatever the project's policy
    /// says. [`crate::policy`] is where that is enforced; this is the fact it
    /// enforces.
    pub const fn may_automerge(&self) -> bool {
        !self.is_untrusted_trigger()
    }

    /// Whether a link to a host the site does not already link to may be added.
    pub const fn may_link_new_hosts(&self) -> bool {
        !self.is_untrusted_trigger()
    }

    /// Decides one write.
    pub fn permits(&self, target: &Target) -> Result<(), Denial> {
        if !self.is_untrusted_trigger() {
            // A trusted run is still refused a path that is not in the project:
            // that is not a trust question.
            return match target {
                Target::Unknown => Err(Denial::NotInProject {
                    path: "<unnormalisable>".to_owned(),
                }),
                _ => Ok(()),
            };
        }
        match target {
            Target::Page(route) if self.scope.allows(route) => Ok(()),
            Target::Page(route) => Err(Denial::OutOfScope {
                route: route.clone(),
            }),
            Target::Config(area) => Err(Denial::Forbidden {
                what: match area {
                    ConfigArea::Navigation => "navigation",
                    ConfigArea::Redirects => "redirects",
                    ConfigArea::Automations => "automations",
                    ConfigArea::Unspecified => "config",
                },
            }),
            Target::Facts => Err(Denial::Forbidden { what: "facts" }),
            Target::AgentsMd => Err(Denial::Forbidden { what: "AGENTS.md" }),
            Target::Other => Err(Denial::Forbidden {
                what: "anything its trigger did not name",
            }),
            Target::Unknown => Err(Denial::NotInProject {
                path: "<unnormalisable>".to_owned(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(path: &str) -> Route {
        Route::new(path)
    }

    fn feedback_about(path: &str) -> Trigger {
        Trigger::new(TriggerKind::Feedback, TrustLevel::Anonymous).about([route(path)])
    }

    #[test]
    fn the_runs_level_is_the_least_trusted_of_its_inputs() {
        let inputs = [
            Input::new(InputKind::Task, "prompt", TrustLevel::Operator),
            Input::new(InputKind::ContentTree, "site", TrustLevel::Member),
            Input::new(InputKind::Task, "ticket body", TrustLevel::External),
        ];
        assert_eq!(effective_trust(&inputs), TrustLevel::External);
    }

    #[test]
    fn an_operator_prompt_that_quotes_a_ticket_is_not_an_operator_run() {
        let inputs = [
            Input::new(InputKind::Task, "operator prompt", TrustLevel::Operator),
            Input::new(InputKind::Task, "quoted ticket", TrustLevel::External),
        ];
        let r = Restrictions::for_inputs(
            &inputs,
            &Trigger::new(TriggerKind::Prompt, TrustLevel::Operator),
        );
        assert!(r.is_untrusted_trigger());
        assert!(!r.may_automerge());
    }

    #[test]
    fn a_member_run_may_write_anywhere_and_may_automerge() {
        let r = Restrictions::for_run(
            TrustLevel::Member,
            &Trigger::new(TriggerKind::Prompt, TrustLevel::Member),
        );
        assert_eq!(r.scope(), &WriteScope::Anywhere);
        assert!(r.may_automerge());
        assert!(r.may_link_new_hosts());
        assert_eq!(r.permits(&Target::Page(route("/anything"))), Ok(()));
        assert_eq!(r.permits(&Target::AgentsMd), Ok(()));
    }

    #[test]
    fn an_untrusted_run_may_write_only_the_pages_its_trigger_named() {
        let r = Restrictions::for_run(TrustLevel::Anonymous, &feedback_about("/guides/install"));
        assert_eq!(r.permits(&Target::Page(route("/guides/install"))), Ok(()));
        assert_eq!(
            r.permits(&Target::Page(route("/guides/upgrade"))),
            Err(Denial::OutOfScope {
                route: route("/guides/upgrade")
            })
        );
    }

    #[test]
    fn an_untrusted_run_is_refused_every_thing_agt_04_names() {
        let r = Restrictions::for_run(TrustLevel::External, &feedback_about("/a"));
        for (target, what) in [
            (Target::Config(ConfigArea::Navigation), "navigation"),
            (Target::Config(ConfigArea::Redirects), "redirects"),
            (Target::Config(ConfigArea::Automations), "automations"),
            (Target::Config(ConfigArea::Unspecified), "config"),
            (Target::Facts, "facts"),
            (Target::AgentsMd, "AGENTS.md"),
        ] {
            assert_eq!(
                r.permits(&target),
                Err(Denial::Forbidden { what }),
                "{target:?} was permitted"
            );
        }
        assert!(!r.may_link_new_hosts());
        assert!(!r.may_automerge());
    }

    #[test]
    fn a_trigger_that_names_no_page_makes_nothing_writable() {
        // The trap this asserts against: reading an empty scope as
        // "unrestricted". A feedback item with no page attached must not be a
        // licence to rewrite the site.
        let r = Restrictions::for_run(
            TrustLevel::Anonymous,
            &Trigger::new(TriggerKind::Feedback, TrustLevel::Anonymous),
        );
        assert_eq!(r.scope(), &WriteScope::Pages(BTreeSet::new()));
        assert_eq!(
            r.permits(&Target::Page(route("/index"))),
            Err(Denial::OutOfScope {
                route: route("/index")
            })
        );
    }

    #[test]
    fn an_unclassifiable_path_is_refused_at_every_level() {
        for trust in [
            TrustLevel::Operator,
            TrustLevel::Member,
            TrustLevel::Anonymous,
            TrustLevel::External,
        ] {
            let r = Restrictions::for_run(trust, &Trigger::new(TriggerKind::Prompt, trust));
            assert!(
                r.permits(&Target::Unknown).is_err(),
                "an unnormalisable path was permitted at {trust:?}"
            );
        }
    }

    #[test]
    fn a_trusted_input_list_under_an_untrusted_trigger_is_still_untrusted() {
        // The trigger's own level counts even when nothing else is untrusted:
        // a pull-request body is the task, and a caller that forgot to add it to
        // the input list must not get an unrestricted run.
        let inputs = [Input::new(
            InputKind::ContentTree,
            "site",
            TrustLevel::Member,
        )];
        let r = Restrictions::for_inputs(
            &inputs,
            &Trigger::new(TriggerKind::PullRequest, TrustLevel::External),
        );
        assert!(r.is_untrusted_trigger());
    }

    #[test]
    fn every_trigger_kind_the_prd_calls_untrusted_produces_a_review() {
        // AGT-20: feedback-, ticket-, chat-, and pull-request-body-triggered
        // runs always end in a human review.
        for kind in [
            TriggerKind::Feedback,
            TriggerKind::SupportTicket,
            TriggerKind::PullRequest,
            TriggerKind::AssistantGap,
        ] {
            let trigger = Trigger::new(kind, TrustLevel::External);
            assert!(
                !Restrictions::for_run(TrustLevel::External, &trigger).may_automerge(),
                "{kind:?} could automerge"
            );
        }
    }
}
