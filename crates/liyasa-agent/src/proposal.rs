//! The proposal a run ends in (AGT-21, AGT-22, AGT-40, AGT-41).
//!
//! The one thing to get right here is the order. AGT-06 says the gates run
//! "before a proposal is created", and [`Proposal::open`] is what makes that true
//! rather than intended: it takes a [`gates::Report`], which has no constructor
//! outside [`gates::check`], and refuses when that report carries a rejection. A
//! caller cannot open a proposal without having gated the diff, and cannot open one
//! from a diff the gate refused.
//!
//! The rest is shape.
//!
//! **AGT-40** wants six things in the summary and per-file accept and reject.
//! [`Summary`] has a field per thing and [`Proposal::header`] renders it, with the
//! gate's flags at the top because that is where AGT-06 puts them. `accepted` is a
//! decision per file, and a file with no decision is undecided rather than
//! accepted — a reviewer who accepted three of eight files has not accepted five
//! by omission.
//!
//! **AGT-22** wants one proposal with a section per task, and a split per page on
//! request. [`Proposal::split_per_page`] is that, and it re-gates nothing because
//! splitting a gated diff cannot introduce a finding the whole did not have.
//!
//! **AGT-21** wants the connected user as co-author for a person-triggered run and
//! the automation named for an automation's. [`Attribution`] has one constructor
//! per case and no third, so a run is attributed to a person or to an automation
//! and never to neither.
//!
//! **AGT-41** wants reviewer comments sent back as a follow-up commit on the same
//! proposal. [`Proposal::address`] appends a [`FollowUp`]; the branch does not
//! change, which is the whole of "the same proposal".

use std::collections::BTreeMap;

use liyasa_core::diagnostics::Diagnostic;
use liyasa_core::ids::Route;

use crate::diff::Diff;
use crate::gates;
use crate::record::RunId;
use crate::scope::{Layout, Target};

/// Who a proposal is attributed to (AGT-21).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribution {
    /// Triggered by a person. The app opens the pull request and names them as
    /// co-author.
    Person { name: String, email: String },
    /// An automation's run, attributed to the automation.
    Automation { name: String },
}

impl Attribution {
    pub fn person(name: impl Into<String>, email: impl Into<String>) -> Self {
        Self::Person {
            name: name.into(),
            email: email.into(),
        }
    }

    pub fn automation(name: impl Into<String>) -> Self {
        Self::Automation { name: name.into() }
    }

    /// The commit trailers the proposal's commits carry.
    pub fn trailers(&self) -> Vec<String> {
        match self {
            Attribution::Person { name, email } => {
                vec![format!("Co-authored-by: {name} <{email}>")]
            }
            Attribution::Automation { name } => vec![format!("Automation: {name}")],
        }
    }

    /// How the proposal header names it.
    pub fn describe(&self) -> String {
        match self {
            Attribution::Person { name, .. } => format!("requested by {name}"),
            Attribution::Automation { name } => format!("run by the `{name}` automation"),
        }
    }
}

/// One file's change, as the summary describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    /// Why, in the agent's words.
    pub why: String,
}

/// One verification result, as the summary lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckLine {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// One task's section of the summary (AGT-22).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    /// What the task was.
    pub task: String,
    /// What changed, and why (AGT-40).
    pub changed: Vec<Change>,
    /// What was already covered, so the reviewer knows why it was left alone.
    pub already_covered: Vec<String>,
    /// What the agent could not verify.
    pub unverified: Vec<String>,
    /// Open questions, including every `ask_reviewer` call.
    pub questions: Vec<String>,
}

/// AGT-40's summary report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub sections: Vec<Section>,
    pub verification: Vec<CheckLine>,
    pub preview: Option<String>,
}

/// Whether a reviewer has accepted a file (AGT-40).
///
/// Three states, not two. A file nobody has looked at is `Undecided`, and a
/// reviewer who accepted three of eight files has not accepted five by omission.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FileDecision {
    #[default]
    Undecided,
    Accepted,
    Rejected,
}

/// A round of reviewer comments answered (AGT-41).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUp {
    pub comments: Vec<String>,
    pub diff: Diff,
}

/// Why a proposal could not be opened.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NotOpened {
    #[error("the output gate rejected this run's diff")]
    Gated(Box<Diagnostic>),
}

impl NotOpened {
    pub fn diagnostic(&self) -> &Diagnostic {
        match self {
            NotOpened::Gated(diagnostic) => diagnostic,
        }
    }
}

/// A branch and a pull request, or a workspace draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    run: RunId,
    branch: String,
    diff: Diff,
    report: gates::Report,
    summary: Summary,
    attribution: Attribution,
    decisions: BTreeMap<String, FileDecision>,
    follow_ups: Vec<FollowUp>,
}

impl Proposal {
    /// Opens a proposal, or refuses because the gate did.
    ///
    /// `report` can only have come from [`gates::check`], so there is no path here
    /// that skipped the gate.
    pub fn open(
        run: RunId,
        branch: impl Into<String>,
        diff: Diff,
        report: gates::Report,
        summary: Summary,
        attribution: Attribution,
    ) -> Result<Self, NotOpened> {
        if let Some(diagnostic) = report.diagnostic() {
            return Err(NotOpened::Gated(Box::new(diagnostic)));
        }
        let decisions = diff
            .files
            .iter()
            .map(|file| (file.path.clone(), FileDecision::Undecided))
            .collect();
        Ok(Self {
            run,
            branch: branch.into(),
            diff,
            report,
            summary,
            attribution,
            decisions,
            follow_ups: Vec::new(),
        })
    }

    pub fn run(&self) -> &RunId {
        &self.run
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    pub fn diff(&self) -> &Diff {
        &self.diff
    }

    pub fn summary(&self) -> &Summary {
        &self.summary
    }

    pub fn attribution(&self) -> &Attribution {
        &self.attribution
    }

    pub fn follow_ups(&self) -> &[FollowUp] {
        &self.follow_ups
    }

    /// The gate's flags, which AGT-06 puts in the header.
    pub fn flags(&self) -> Vec<String> {
        self.report.flags().map(ToString::to_string).collect()
    }

    /// The proposal header and body, as Markdown for a pull request.
    pub fn header(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Proposed by the writing agent, {}.\n",
            self.attribution.describe()
        ));
        let flags = self.flags();
        if !flags.is_empty() {
            out.push_str("\n## Flagged for review\n\n");
            for flag in &flags {
                out.push_str(&format!("- {flag}\n"));
            }
        }
        for section in &self.summary.sections {
            out.push_str(&format!("\n## {}\n", section.task));
            out.push_str("\n### What changed and why\n\n");
            if section.changed.is_empty() {
                out.push_str("- nothing\n");
            }
            for change in &section.changed {
                out.push_str(&format!("- `{}` — {}\n", change.path, change.why));
            }
            out.push_str("\n### Already covered\n\n");
            if section.already_covered.is_empty() {
                out.push_str("- nothing was already covered\n");
            }
            for item in &section.already_covered {
                out.push_str(&format!("- {item}\n"));
            }
            out.push_str("\n### Could not verify\n\n");
            if section.unverified.is_empty() {
                out.push_str("- everything in this section was verified\n");
            }
            for item in &section.unverified {
                out.push_str(&format!("- {item}\n"));
            }
            out.push_str("\n### Open questions\n\n");
            if section.questions.is_empty() {
                out.push_str("- none\n");
            }
            for item in &section.questions {
                out.push_str(&format!("- {item}\n"));
            }
        }
        out.push_str("\n## Verification\n\n");
        if self.summary.verification.is_empty() {
            out.push_str("- nothing was run\n");
        }
        for check in &self.summary.verification {
            let mark = if check.passed { "passed" } else { "FAILED" };
            out.push_str(&format!("- {} — {mark}: {}\n", check.name, check.detail));
        }
        if let Some(preview) = &self.summary.preview {
            out.push_str(&format!("\n## Preview\n\n{preview}\n"));
        }
        if !self.follow_ups.is_empty() {
            out.push_str(&format!(
                "\n## Follow-ups\n\n{} round(s) of reviewer comments addressed.\n",
                self.follow_ups.len()
            ));
        }
        out
    }

    /// The paths a reviewer may decide on.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.decisions.keys().map(String::as_str)
    }

    pub fn decision(&self, path: &str) -> FileDecision {
        self.decisions
            .get(path)
            .copied()
            .unwrap_or(FileDecision::Undecided)
    }

    /// Accepts one file. `false` when the proposal has no such file.
    pub fn accept(&mut self, path: &str) -> bool {
        self.set(path, FileDecision::Accepted)
    }

    /// Rejects one file.
    pub fn reject(&mut self, path: &str) -> bool {
        self.set(path, FileDecision::Rejected)
    }

    fn set(&mut self, path: &str, decision: FileDecision) -> bool {
        match self.decisions.get_mut(path) {
            Some(slot) => {
                *slot = decision;
                true
            }
            None => false,
        }
    }

    /// Accepts the whole run (AGT-40's per-run accept).
    pub fn accept_all(&mut self) {
        for decision in self.decisions.values_mut() {
            *decision = FileDecision::Accepted;
        }
    }

    /// Rejects the whole run.
    pub fn reject_all(&mut self) {
        for decision in self.decisions.values_mut() {
            *decision = FileDecision::Rejected;
        }
    }

    /// The files a reviewer accepted. Not the undecided ones.
    pub fn accepted(&self) -> Vec<&str> {
        self.decisions
            .iter()
            .filter(|(_, d)| **d == FileDecision::Accepted)
            .map(|(path, _)| path.as_str())
            .collect()
    }

    pub fn undecided(&self) -> Vec<&str> {
        self.decisions
            .iter()
            .filter(|(_, d)| **d == FileDecision::Undecided)
            .map(|(path, _)| path.as_str())
            .collect()
    }

    /// Whether every file has been decided.
    pub fn is_fully_reviewed(&self) -> bool {
        self.undecided().is_empty()
    }

    /// Addresses a round of reviewer comments (AGT-41).
    ///
    /// The branch does not change: a follow-up is a commit on the same proposal,
    /// which is what "in the same proposal" means. The follow-up's diff is gated
    /// by the caller before it gets here, the same as the first one.
    pub fn address(
        &mut self,
        comments: impl IntoIterator<Item = impl Into<String>>,
        diff: Diff,
        report: &gates::Report,
    ) -> Result<&FollowUp, NotOpened> {
        if let Some(diagnostic) = report.diagnostic() {
            return Err(NotOpened::Gated(Box::new(diagnostic)));
        }
        for file in &diff.files {
            self.decisions
                .entry(file.path.clone())
                .insert_entry(FileDecision::Undecided);
        }
        self.follow_ups.push(FollowUp {
            comments: comments.into_iter().map(Into::into).collect(),
            diff,
        });
        Ok(self.follow_ups.last().expect("just pushed"))
    }

    /// One proposal per page it touches (AGT-22's "split per page on request").
    ///
    /// Nothing is re-gated. A subset of a diff the gate passed cannot carry a
    /// finding the whole did not — every check here is either per file or a cap on
    /// a total, and a subset's total is no larger.
    ///
    /// Files that are not pages travel with the first proposal, because a config
    /// or asset change belongs with something rather than in a proposal of its own
    /// that nobody asked for.
    pub fn split_per_page(&self, layout: &Layout) -> Vec<Proposal> {
        let mut groups: BTreeMap<Option<Route>, Vec<crate::diff::FileChange>> = BTreeMap::new();
        for file in &self.diff.files {
            let key = match layout.classify(&file.path) {
                Target::Page(route) => Some(route),
                _ => None,
            };
            groups.entry(key).or_default().push(file.clone());
        }
        if groups.len() <= 1 {
            return vec![self.clone()];
        }
        let mut out: Vec<Proposal> = Vec::new();
        let mut leftovers = groups.remove(&None).unwrap_or_default();
        for (route, files) in groups {
            let mut files = files;
            if out.is_empty() {
                files.append(&mut leftovers);
            }
            let suffix = route
                .as_ref()
                .map_or_else(|| "other".to_owned(), |r| r.as_str().replace('/', "-"));
            let mut part = self.clone();
            part.branch = format!("{}{}", self.branch, suffix);
            part.diff = Diff::new(files);
            part.decisions = part
                .diff
                .files
                .iter()
                .map(|f| (f.path.clone(), self.decision(&f.path)))
                .collect();
            part.follow_ups = Vec::new();
            out.push(part);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Limits;
    use crate::diff::FileChange;
    use crate::hosts::KnownHosts;
    use crate::injection::Detector;
    use crate::trust::{Restrictions, Trigger, TriggerKind, TrustLevel};

    fn page(body: &str) -> String {
        format!("---\ntitle: t\n---\n\n{body}\n")
    }

    fn diff() -> Diff {
        Diff::new([
            FileChange::modified("guides/install.md", page("Old."), page("New.")),
            FileChange::modified("guides/upgrade.md", page("Old."), page("Newer.")),
        ])
    }

    fn gated(diff: &Diff) -> gates::Report {
        let restrictions = Restrictions::for_run(
            TrustLevel::Member,
            &Trigger::new(TriggerKind::Prompt, TrustLevel::Member),
        );
        let limits = Limits::default();
        let layout = Layout::default();
        let known = KnownHosts::new(["docs.example.com"]);
        let detector = Detector::default();
        gates::check(gates::Inputs {
            diff,
            restrictions: &restrictions,
            limits: &limits,
            layout: &layout,
            known_hosts: &known,
            injection: &detector,
            allow_bulk_delete: false,
        })
    }

    fn gated_with_a_flag(diff: &Diff) -> gates::Report {
        // A member run adding a link to a new host: a flag, not a rejection.
        let restrictions = Restrictions::for_run(
            TrustLevel::Member,
            &Trigger::new(TriggerKind::Prompt, TrustLevel::Member),
        );
        let limits = Limits::default();
        let layout = Layout::default();
        let known = KnownHosts::default();
        let detector = Detector::default();
        gates::check(gates::Inputs {
            diff,
            restrictions: &restrictions,
            limits: &limits,
            layout: &layout,
            known_hosts: &known,
            injection: &detector,
            allow_bulk_delete: false,
        })
    }

    fn summary() -> Summary {
        Summary {
            sections: vec![Section {
                task: "Drift 41: the seat limit changed".to_owned(),
                changed: vec![Change {
                    path: "guides/install.md".to_owned(),
                    why: "the seat limit is now 10".to_owned(),
                }],
                already_covered: vec!["`/pricing` already says 10".to_owned()],
                unverified: vec!["the screenshot on `/tour` was not re-taken".to_owned()],
                questions: vec!["is the old limit worth a note?".to_owned()],
            }],
            verification: vec![CheckLine {
                name: "liyasa verify --changed".to_owned(),
                passed: true,
                detail: "4 checks, 0 failures".to_owned(),
            }],
            preview: Some("https://preview.example.com/run-1/".to_owned()),
        }
    }

    fn open(diff: Diff, report: gates::Report) -> Proposal {
        Proposal::open(
            RunId::new("run-1"),
            "agent/run-1",
            diff,
            report,
            summary(),
            Attribution::person("Ada", "ada@example.com"),
        )
        .expect("the gate passed")
    }

    #[test]
    fn a_gated_diff_opens_a_proposal() {
        let diff = diff();
        let report = gated(&diff);
        let proposal = open(diff, report);
        assert_eq!(proposal.branch(), "agent/run-1");
        assert_eq!(proposal.diff().files_changed(), 2);
    }

    #[test]
    fn a_rejected_diff_does_not_open_one() {
        // AGT-06's "before a proposal is created", as a type rather than a habit.
        let diff = Diff::new([FileChange::added(
            "guides/p.md",
            page("Use AKIAQWERTYUIOPASDFGH."),
        )]);
        let report = gated(&diff);
        assert!(report.rejected());
        let error = Proposal::open(
            RunId::new("run-1"),
            "agent/run-1",
            diff,
            report,
            summary(),
            Attribution::automation("nightly-drift"),
        )
        .expect_err("the gate rejected it");
        assert_eq!(
            error.diagnostic().code,
            liyasa_core::diagnostics::code::E0904
        );
    }

    #[test]
    fn the_header_carries_all_six_things_agt_40_names() {
        let diff = diff();
        let report = gated(&diff);
        let header = open(diff, report).header();
        for expected in [
            "What changed and why",
            "the seat limit is now 10",
            "Already covered",
            "Could not verify",
            "Open questions",
            "Verification",
            "Preview",
            "https://preview.example.com/run-1/",
        ] {
            assert!(
                header.contains(expected),
                "`{expected}` is missing:\n{header}"
            );
        }
    }

    #[test]
    fn the_gates_flags_are_in_the_header() {
        // AGT-06: "Flags are shown to the reviewer in the proposal header."
        let diff = Diff::new([FileChange::modified(
            "guides/install.md",
            page("Old."),
            page("See [more](https://elsewhere.example/x)."),
        )]);
        let report = gated_with_a_flag(&diff);
        assert!(!report.rejected(), "{:?}", report.findings());
        let proposal = open(diff, report);
        assert!(!proposal.flags().is_empty());
        let header = proposal.header();
        assert!(header.contains("Flagged for review"), "{header}");
        assert!(header.contains("elsewhere.example"), "{header}");
    }

    #[test]
    fn a_header_with_nothing_flagged_has_no_flag_section() {
        let diff = diff();
        let report = gated(&diff);
        let header = open(diff, report).header();
        assert!(!header.contains("Flagged for review"), "{header}");
    }

    #[test]
    fn an_empty_summary_section_says_so_rather_than_going_missing() {
        // A reviewer who sees no "Could not verify" heading cannot tell whether
        // everything was verified or the section was dropped.
        let diff = diff();
        let report = gated(&diff);
        let mut proposal = open(diff, report);
        proposal.summary = Summary {
            sections: vec![Section {
                task: "a task".to_owned(),
                ..Section::default()
            }],
            ..Summary::default()
        };
        let header = proposal.header();
        assert!(
            header.contains("everything in this section was verified"),
            "{header}"
        );
        assert!(header.contains("nothing was already covered"), "{header}");
        assert!(header.contains("nothing was run"), "{header}");
    }

    #[test]
    fn a_file_starts_undecided_and_is_not_accepted_by_omission() {
        let diff = diff();
        let report = gated(&diff);
        let mut proposal = open(diff, report);
        assert_eq!(
            proposal.decision("guides/install.md"),
            FileDecision::Undecided
        );
        assert!(proposal.accepted().is_empty());
        assert!(!proposal.is_fully_reviewed());

        assert!(proposal.accept("guides/install.md"));
        assert_eq!(proposal.accepted(), ["guides/install.md"]);
        assert_eq!(proposal.undecided(), ["guides/upgrade.md"]);
        assert!(!proposal.is_fully_reviewed());
    }

    #[test]
    fn per_file_accept_and_reject_both_work() {
        let diff = diff();
        let report = gated(&diff);
        let mut proposal = open(diff, report);
        proposal.accept("guides/install.md");
        proposal.reject("guides/upgrade.md");
        assert_eq!(proposal.accepted(), ["guides/install.md"]);
        assert_eq!(
            proposal.decision("guides/upgrade.md"),
            FileDecision::Rejected
        );
        assert!(proposal.is_fully_reviewed());
    }

    #[test]
    fn a_decision_on_a_file_the_proposal_does_not_have_is_refused() {
        let diff = diff();
        let report = gated(&diff);
        let mut proposal = open(diff, report);
        assert!(!proposal.accept("guides/nowhere.md"));
        assert_eq!(
            proposal.decision("guides/nowhere.md"),
            FileDecision::Undecided
        );
    }

    #[test]
    fn per_run_accept_and_reject_decide_every_file() {
        let diff = diff();
        let report = gated(&diff);
        let mut proposal = open(diff, report);
        proposal.accept_all();
        assert_eq!(proposal.accepted().len(), 2);
        proposal.reject_all();
        assert!(proposal.accepted().is_empty());
        assert!(proposal.is_fully_reviewed());
    }

    #[test]
    fn a_person_triggered_run_names_the_person_as_co_author() {
        let attribution = Attribution::person("Ada", "ada@example.com");
        assert_eq!(
            attribution.trailers(),
            ["Co-authored-by: Ada <ada@example.com>"]
        );
        assert!(attribution.describe().contains("Ada"));
    }

    #[test]
    fn an_automation_run_is_attributed_to_the_automation() {
        let attribution = Attribution::automation("nightly-drift");
        assert_eq!(attribution.trailers(), ["Automation: nightly-drift"]);
        assert!(attribution.describe().contains("nightly-drift"));
        assert!(
            !attribution.trailers()[0].starts_with("Co-authored-by"),
            "an automation must not be someone's co-author"
        );
    }

    #[test]
    fn addressing_comments_adds_a_follow_up_on_the_same_branch() {
        // AGT-41. "The same proposal" is the branch not changing.
        let diff = diff();
        let report = gated(&diff);
        let mut proposal = open(diff, report);
        let branch = proposal.branch().to_owned();
        let follow = Diff::new([FileChange::modified(
            "guides/install.md",
            page("New."),
            page("Newest."),
        )]);
        let follow_report = gated(&follow);
        let added = proposal
            .address(["shorten the second paragraph"], follow, &follow_report)
            .expect("the follow-up gated clean");
        assert_eq!(added.comments, ["shorten the second paragraph"]);
        assert_eq!(proposal.branch(), branch);
        assert_eq!(proposal.follow_ups().len(), 1);
        assert!(
            proposal.header().contains("Follow-ups"),
            "{}",
            proposal.header()
        );
    }

    #[test]
    fn a_follow_up_the_gate_rejects_is_refused_and_changes_nothing() {
        let diff = diff();
        let report = gated(&diff);
        let mut proposal = open(diff, report);
        let hostile = Diff::new([FileChange::modified(
            "guides/install.md",
            page("New."),
            page("Use AKIAQWERTYUIOPASDFGH."),
        )]);
        let hostile_report = gated(&hostile);
        assert!(
            proposal
                .address(["do it"], hostile, &hostile_report)
                .is_err()
        );
        assert!(proposal.follow_ups().is_empty());
    }

    #[test]
    fn a_follow_up_that_touches_a_new_file_makes_it_undecided() {
        let diff = diff();
        let report = gated(&diff);
        let mut proposal = open(diff, report);
        proposal.accept_all();
        let follow = Diff::new([FileChange::added("guides/new.md", page("New page."))]);
        let follow_report = gated(&follow);
        proposal
            .address(["add a page for this"], follow, &follow_report)
            .expect("clean");
        assert_eq!(proposal.undecided(), ["guides/new.md"]);
        assert!(!proposal.is_fully_reviewed());
    }

    #[test]
    fn a_follow_up_does_not_undo_a_decision_on_a_file_it_did_not_touch() {
        let diff = diff();
        let report = gated(&diff);
        let mut proposal = open(diff, report);
        proposal.accept("guides/upgrade.md");
        let follow = Diff::new([FileChange::modified(
            "guides/install.md",
            page("New."),
            page("Newest."),
        )]);
        let follow_report = gated(&follow);
        proposal
            .address(["x"], follow, &follow_report)
            .expect("clean");
        assert_eq!(
            proposal.decision("guides/upgrade.md"),
            FileDecision::Accepted
        );
    }

    #[test]
    fn one_run_with_several_tasks_is_one_proposal_with_a_section_each() {
        // AGT-22.
        let diff = diff();
        let report = gated(&diff);
        let mut proposal = open(diff, report);
        proposal.summary.sections = vec![
            Section {
                task: "Drift 41".to_owned(),
                ..Section::default()
            },
            Section {
                task: "Ticket 88".to_owned(),
                ..Section::default()
            },
        ];
        let header = proposal.header();
        assert!(header.contains("## Drift 41"), "{header}");
        assert!(header.contains("## Ticket 88"), "{header}");
    }

    #[test]
    fn a_run_can_be_split_per_page() {
        let diff = diff();
        let report = gated(&diff);
        let proposal = open(diff, report);
        let parts = proposal.split_per_page(&Layout::default());
        assert_eq!(
            parts.len(),
            2,
            "{:?}",
            parts.iter().map(Proposal::branch).collect::<Vec<_>>()
        );
        let branches: Vec<&str> = parts.iter().map(Proposal::branch).collect();
        assert!(branches.iter().all(|b| b.starts_with("agent/run-1")));
        assert_eq!(
            branches.len(),
            branches
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        );
        for part in &parts {
            assert_eq!(part.diff().files_changed(), 1);
        }
    }

    #[test]
    fn splitting_a_one_page_run_gives_back_the_one_proposal() {
        let diff = Diff::new([FileChange::modified(
            "guides/install.md",
            page("Old."),
            page("New."),
        )]);
        let report = gated(&diff);
        let proposal = open(diff, report);
        let parts = proposal.split_per_page(&Layout::default());
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].branch(), "agent/run-1");
    }

    #[test]
    fn a_split_keeps_every_file_and_loses_none() {
        let diff = Diff::new([
            FileChange::modified("guides/install.md", page("Old."), page("New.")),
            FileChange::modified("guides/upgrade.md", page("Old."), page("New.")),
            FileChange::added("assets/diagram.svg", "<svg/>"),
        ]);
        let report = gated(&diff);
        let before: std::collections::BTreeSet<String> =
            diff.files.iter().map(|f| f.path.clone()).collect();
        let proposal = open(diff, report);
        let parts = proposal.split_per_page(&Layout::default());
        let after: std::collections::BTreeSet<String> = parts
            .iter()
            .flat_map(|p| p.diff().files.iter().map(|f| f.path.clone()))
            .collect();
        assert_eq!(after, before, "a split dropped or duplicated a file");
    }

    #[test]
    fn a_split_carries_the_decisions_already_made() {
        let diff = diff();
        let report = gated(&diff);
        let mut proposal = open(diff, report);
        proposal.accept("guides/install.md");
        let parts = proposal.split_per_page(&Layout::default());
        let install = parts
            .iter()
            .find(|p| p.files().any(|f| f == "guides/install.md"))
            .expect("the install part");
        assert_eq!(
            install.decision("guides/install.md"),
            FileDecision::Accepted
        );
    }
}
