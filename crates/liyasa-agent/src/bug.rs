//! Bug records the agent files while researching (AGT-42).
//!
//! The agent reads code to ground what it writes, so it is in a position to find
//! that the code and the documentation disagree — and when it does, writing a page
//! that matches the code is the wrong answer on its own. AGT-42 wants the finding
//! recorded with evidence, and an issue opened in the connected tracker per policy.
//!
//! Two rules make this worth having rather than a source of noise.
//!
//! **Evidence is not optional.** [`BugRecord::file`] takes at least one
//! [`Evidence`] and refuses an empty list. A bug record that says "the sample seems
//! wrong" costs a maintainer's afternoon and proves nothing; one that carries the
//! request, the response and what was expected is a report.
//!
//! **Evidence is redacted.** It quotes what a server sent back, and a failing
//! authenticated request is exactly the shape that carries a token. It goes through
//! [`crate::secrets::redact`] on the way in, like everything in the run record.
//!
//! An untrusted-trigger run may file a record and may not open an issue, whatever
//! the policy says. Opening an issue is an outbound action in the project's name,
//! and a stranger who can make the agent do that has a channel out of the review
//! AGT-20 guarantees.

use liyasa_core::ids::Route;
use serde::{Deserialize, Serialize};

use crate::trust::Restrictions;

/// What kind of product problem this is (AGT-42's three).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A code sample that does not work against staging.
    FailingSample,
    /// A spec that contradicts the code.
    SpecContradictsCode,
    /// An integration that no longer answers.
    DeadIntegration,
}

impl Kind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::FailingSample => "a code sample that fails",
            Kind::SpecContradictsCode => "a spec that contradicts the code",
            Kind::DeadIntegration => "a dead integration",
        }
    }
}

/// One piece of evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// Where it came from: a URL, a repository path, an operation id.
    pub source: String,
    /// What was observed.
    pub observed: String,
    /// What the documentation says should happen.
    pub expected: String,
}

impl Evidence {
    pub fn new(
        source: impl Into<String>,
        observed: impl Into<String>,
        expected: impl Into<String>,
    ) -> Self {
        Self {
            source: crate::secrets::redact(&source.into()),
            observed: crate::secrets::redact(&observed.into()),
            expected: crate::secrets::redact(&expected.into()),
        }
    }
}

/// Which tracker an issue would be opened in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tracker {
    GitHub,
    Linear,
    Jira,
}

/// When the agent may open an issue rather than only filing a record.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IssuePolicy {
    /// File the record; never open an issue. The default.
    #[default]
    RecordOnly,
    /// Open one, for a run that is allowed to act on its own.
    Open,
}

/// Why a record could not be filed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NotFiled {
    #[error("a bug record needs at least one piece of evidence")]
    NoEvidence,
    #[error("a bug record needs a title")]
    NoTitle,
}

/// A product problem the agent found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BugRecord {
    pub kind: Kind,
    pub title: String,
    /// The page the problem was found from, where there was one.
    pub page: Option<Route>,
    evidence: Vec<Evidence>,
}

impl BugRecord {
    /// Files a record. Refuses one with no evidence or no title.
    pub fn file(
        kind: Kind,
        title: impl Into<String>,
        page: Option<Route>,
        evidence: impl IntoIterator<Item = Evidence>,
    ) -> Result<Self, NotFiled> {
        let title = crate::secrets::redact(title.into().trim());
        if title.is_empty() {
            return Err(NotFiled::NoTitle);
        }
        let evidence: Vec<Evidence> = evidence.into_iter().collect();
        if evidence.is_empty() {
            return Err(NotFiled::NoEvidence);
        }
        Ok(Self {
            kind,
            title,
            page,
            evidence,
        })
    }

    /// Never empty: [`Self::file`] refuses a record without it.
    pub fn evidence(&self) -> &[Evidence] {
        &self.evidence
    }

    /// Whether an issue may be opened for this record.
    ///
    /// An untrusted-trigger run never may, whatever the policy says: an issue is an
    /// outbound action in the project's name, and a stranger who can cause one has
    /// a channel around the review AGT-20 guarantees.
    pub fn may_open_issue(&self, policy: IssuePolicy, restrictions: &Restrictions) -> bool {
        policy == IssuePolicy::Open && !restrictions.is_untrusted_trigger()
    }

    /// The issue body, as a tracker would carry it.
    pub fn issue_body(&self) -> String {
        let mut out = format!(
            "Found while writing documentation: {}.\n",
            self.kind.as_str()
        );
        if let Some(page) = &self.page {
            out.push_str(&format!("\nFound from `{page}`.\n"));
        }
        out.push_str("\n## Evidence\n");
        for item in &self.evidence {
            out.push_str(&format!(
                "\n### {}\n\nObserved: {}\n\nThe documentation says: {}\n",
                item.source, item.observed, item.expected
            ));
        }
        out.push_str(
            "\nFiled by the writing agent. It has not changed any page on the strength of this.\n",
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust::{Trigger, TriggerKind, TrustLevel};

    const SECRET: &str = "AKIAQWERTYUIOPASDFGH";

    fn evidence() -> Evidence {
        Evidence::new(
            "POST https://staging.example.com/v1/seats",
            "422 Unprocessable Entity: `plan` is required",
            "the sample on `/guides/seats` sends no `plan`",
        )
    }

    fn record() -> BugRecord {
        BugRecord::file(
            Kind::FailingSample,
            "the seats sample is missing `plan`",
            Some(Route::new("/guides/seats")),
            [evidence()],
        )
        .expect("a record with evidence")
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
    fn a_record_with_evidence_is_filed() {
        let record = record();
        assert_eq!(record.kind, Kind::FailingSample);
        assert_eq!(record.evidence().len(), 1);
        assert_eq!(record.page, Some(Route::new("/guides/seats")));
    }

    #[test]
    fn a_record_with_no_evidence_is_refused() {
        // "The sample seems wrong" costs a maintainer an afternoon and proves
        // nothing.
        assert_eq!(
            BugRecord::file(Kind::FailingSample, "something is off", None, []),
            Err(NotFiled::NoEvidence)
        );
    }

    #[test]
    fn a_record_with_no_title_is_refused() {
        assert_eq!(
            BugRecord::file(Kind::DeadIntegration, "   ", None, [evidence()]),
            Err(NotFiled::NoTitle)
        );
    }

    #[test]
    fn evidence_is_redacted_on_the_way_in() {
        // A failing authenticated request is exactly the shape that carries a
        // token, and this record is going into a tracker.
        let record = BugRecord::file(
            Kind::FailingSample,
            "the sample fails",
            None,
            [Evidence::new(
                format!("GET https://staging.example.com/?key={SECRET}"),
                format!("401: the key {SECRET} is not valid"),
                "a 200",
            )],
        )
        .expect("filed");
        let body = record.issue_body();
        assert!(
            !body.contains(SECRET),
            "the issue body carries the key: {body}"
        );
        assert!(body.contains("[redacted]"), "{body}");
    }

    #[test]
    fn a_secret_in_the_title_is_redacted_too() {
        let record = BugRecord::file(
            Kind::FailingSample,
            format!("the key {SECRET} is rejected"),
            None,
            [evidence()],
        )
        .expect("filed");
        assert!(!record.title.contains(SECRET));
    }

    #[test]
    fn the_issue_body_carries_the_evidence_and_says_nothing_was_changed() {
        let body = record().issue_body();
        assert!(body.contains("422 Unprocessable Entity"), "{body}");
        assert!(
            body.contains("the sample on `/guides/seats` sends no `plan`"),
            "{body}"
        );
        assert!(body.contains("/guides/seats"), "{body}");
        assert!(
            body.contains("has not changed any page"),
            "a maintainer has to know the docs were not edited on this basis:\n{body}"
        );
    }

    #[test]
    fn record_only_is_the_default_policy() {
        assert_eq!(IssuePolicy::default(), IssuePolicy::RecordOnly);
        assert!(!record().may_open_issue(IssuePolicy::default(), &member()));
    }

    #[test]
    fn a_member_run_may_open_an_issue_when_the_policy_says_so() {
        assert!(record().may_open_issue(IssuePolicy::Open, &member()));
    }

    #[test]
    fn a_stranger_may_file_a_record_and_may_not_open_an_issue() {
        // The record is internal; the issue is an outbound action in the
        // project's name.
        let record = record();
        assert!(!record.may_open_issue(IssuePolicy::Open, &stranger()));
        assert!(!record.may_open_issue(IssuePolicy::RecordOnly, &stranger()));
    }

    #[test]
    fn every_kind_agt_42_names_has_a_description() {
        for kind in [
            Kind::FailingSample,
            Kind::SpecContradictsCode,
            Kind::DeadIntegration,
        ] {
            assert!(!kind.as_str().is_empty());
            assert!(record().issue_body().contains("Found while writing"));
            let _ = kind;
        }
    }

    #[test]
    fn a_record_round_trips_through_json() {
        let text = serde_json::to_string(&record()).expect("serializes");
        let back: BugRecord = serde_json::from_str(&text).expect("reads back");
        assert_eq!(back, record());
    }

    #[test]
    fn every_tracker_agt_42_names_is_a_variant() {
        for tracker in [Tracker::GitHub, Tracker::Linear, Tracker::Jira] {
            let text = serde_json::to_string(&tracker).expect("serializes");
            assert!(!text.is_empty());
        }
        assert_eq!(
            serde_json::to_string(&Tracker::GitHub).expect("serializes"),
            "\"github\""
        );
    }
}
