//! What an untrusted build may not do (GIT-31).
//!
//! A pull request from a fork is code a stranger wrote, built by a machine
//! that holds the project's secrets. Three things keep that from being a
//! credential leak, and all three are decided here rather than inside the
//! build: the process environment the worker starts with, which branch the
//! trust-plane files are read from, and which truth sources may run.
//!
//! The rule that matters most is the dullest. `{{ env("GITHUB_TOKEN") }}` in a
//! page renders whatever the worker's environment holds. So an untrusted
//! worker's environment is built by an allow list, not filtered by a deny
//! list: a variable nobody thought of is absent rather than present.

use std::collections::BTreeMap;

use liyasa_git::event::{Event, PullRequestEvent, Push};
use serde::{Deserialize, Serialize};

/// The only variables an untrusted worker inherits, plus the `LIYASA_` prefix.
/// `PATH` so the toolchain resolves, `HOME` so a cache directory exists.
pub const KEPT: &[&str] = &["PATH", "HOME"];
pub const KEPT_PREFIX: &str = "LIYASA_";

/// What a preview built this way is labelled in the widget and the comment.
pub const UNTRUSTED_LABEL: &str = "untrusted preview";
pub const TRUSTED_LABEL: &str = "preview";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// The head is a fork of the repository (GIT-31).
    Fork,
    /// The branch is outside `verify.sources.trustedBranches`.
    BranchNotTrusted {
        branch: String,
    },
    Trusted,
}

impl Reason {
    pub fn untrusted(&self) -> bool {
        !matches!(self, Self::Trusted)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fork => "the pull request comes from a fork",
            Self::BranchNotTrusted { .. } => "the branch is not in the trusted set",
            Self::Trusted => "the branch is trusted",
        }
    }
}

/// A `fnmatch`-style branch pattern: `*` matches any run of characters,
/// including a slash, so `release/*` and `*-stable` both work.
pub fn matches_pattern(pattern: &str, branch: &str) -> bool {
    let mut parts = pattern.split('*');
    let Some(first) = parts.next() else {
        return false;
    };
    if !branch.starts_with(first) {
        return false;
    }
    let mut rest = &branch[first.len()..];
    let tail: Vec<&str> = parts.collect();
    if tail.is_empty() {
        return rest.is_empty();
    }
    for (index, part) in tail.iter().enumerate() {
        if index + 1 == tail.len() {
            // The final literal must be at the end, so `release/*-rc` does not
            // match `release/1-rc-2`.
            return rest.len() >= part.len() && rest.ends_with(part);
        }
        if part.is_empty() {
            continue;
        }
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}

/// TODO(rfc-1604): GIT-31 names `verify.sources.trustedBranches` and
/// `schemas/liyasa.schema.json` does not define it, so the set is supplied by
/// the caller and defaults to the deploy branch alone.
pub fn trusted_branch(branch: &str, deploy_branch: &str, trusted: &[String]) -> bool {
    branch == deploy_branch || trusted.iter().any(|p| matches_pattern(p, branch))
}

/// Whether a build of this event is trusted.
pub fn classify(event: &Event, deploy_branch: &str, trusted: &[String]) -> Reason {
    match event {
        Event::PullRequest(PullRequestEvent {
            from_fork: true, ..
        }) => Reason::Fork,
        Event::PullRequest(pull) => branch_reason(&pull.head_branch, deploy_branch, trusted),
        Event::Push(Push { branch, .. }) => branch_reason(branch, deploy_branch, trusted),
        Event::Ignored { .. } => Reason::Trusted,
    }
}

fn branch_reason(branch: &str, deploy_branch: &str, trusted: &[String]) -> Reason {
    match trusted_branch(branch, deploy_branch, trusted) {
        true => Reason::Trusted,
        false => Reason::BranchNotTrusted {
            branch: branch.to_owned(),
        },
    }
}

/// What a worker is allowed to do for one build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sandbox {
    /// The whole process environment the worker starts with.
    pub env: BTreeMap<String, String>,
    /// Whether `build.env` is honoured and `env()` returns values.
    pub allow_build_env: bool,
    /// The ref the trust-plane files are read from (CFG-95). For an untrusted
    /// build this is the deploy branch, never the pull request.
    pub trust_plane_ref: String,
    /// Whether `url`, `command` and `screenshot` sources are refreshed.
    pub refresh_sources: bool,
    /// Whether a `command` source may execute at all (VER-25).
    pub run_command_sources: bool,
    /// Whether a code runner that needs credentials may start.
    pub allow_credentialed_runners: bool,
    pub label: String,
}

impl Sandbox {
    /// A build of a trusted branch: the worker's own environment, the pull
    /// request's own trust plane, every source live.
    pub fn trusted(env: BTreeMap<String, String>, build_ref: &str) -> Self {
        Self {
            env,
            allow_build_env: true,
            trust_plane_ref: build_ref.to_owned(),
            refresh_sources: true,
            run_command_sources: true,
            allow_credentialed_runners: true,
            label: TRUSTED_LABEL.to_owned(),
        }
    }

    /// A build of a fork or an untrusted branch (GIT-31).
    pub fn untrusted(deploy_branch: &str) -> Self {
        Self {
            env: BTreeMap::new(),
            allow_build_env: false,
            trust_plane_ref: deploy_branch.to_owned(),
            refresh_sources: false,
            run_command_sources: false,
            allow_credentialed_runners: false,
            label: UNTRUSTED_LABEL.to_owned(),
        }
    }

    /// Builds the sandbox for a classified event, taking the worker's own
    /// environment as the source for a trusted build.
    pub fn for_reason<I, K, V>(
        reason: &Reason,
        ambient: I,
        build_ref: &str,
        deploy_branch: &str,
    ) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        match reason.untrusted() {
            true => {
                let mut sandbox = Self::untrusted(deploy_branch);
                sandbox.env = keep_only(ambient);
                sandbox
            }
            false => Self::trusted(
                ambient
                    .into_iter()
                    .map(|(k, v)| (k.as_ref().to_owned(), v.as_ref().to_owned()))
                    .collect(),
                build_ref,
            ),
        }
    }

    /// Whether a named variable is visible to this build's `env()`.
    pub fn sees(&self, name: &str) -> bool {
        self.allow_build_env && self.env.contains_key(name)
    }
}

/// The allow list of GIT-31(a): `PATH`, `HOME`, and `LIYASA_*`. Everything
/// else is dropped, whatever it is called.
pub fn keep_only<I, K, V>(vars: I) -> BTreeMap<String, String>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    vars.into_iter()
        .filter(|(name, _)| {
            let name = name.as_ref();
            KEPT.contains(&name) || name.starts_with(KEPT_PREFIX)
        })
        .map(|(name, value)| (name.as_ref().to_owned(), value.as_ref().to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use liyasa_git::event::PrAction;
    use liyasa_git::repo::RepoRef;

    use super::*;

    fn pull(from_fork: bool, head_branch: &str) -> Event {
        Event::PullRequest(PullRequestEvent {
            repo: RepoRef::new("kasecrab", "liyasa"),
            number: 1,
            action: PrAction::Opened,
            head_branch: head_branch.to_owned(),
            head_sha: "abc".to_owned(),
            base_branch: "main".to_owned(),
            from_fork,
            draft: false,
            installation: None,
        })
    }

    fn push(branch: &str) -> Event {
        Event::Push(Push {
            repo: RepoRef::new("kasecrab", "liyasa"),
            branch: branch.to_owned(),
            head: "abc".to_owned(),
            before: None,
            deleted: false,
            changed: Vec::new(),
            installation: None,
        })
    }

    #[test]
    fn a_fork_is_untrusted_whatever_its_branch_is_called() {
        assert_eq!(classify(&pull(true, "main"), "main", &[]), Reason::Fork);
        assert!(classify(&pull(true, "main"), "main", &[]).untrusted());
    }

    #[test]
    fn a_branch_in_the_same_repository_is_trusted_only_if_the_set_says_so() {
        let trusted = ["release/*".to_owned()];
        assert_eq!(
            classify(&push("main"), "main", &trusted),
            Reason::Trusted,
            "the deploy branch is trusted without being listed"
        );
        assert_eq!(
            classify(&push("release/2"), "main", &trusted),
            Reason::Trusted
        );
        assert_eq!(
            classify(&push("feat/x"), "main", &trusted),
            Reason::BranchNotTrusted {
                branch: "feat/x".to_owned()
            }
        );
    }

    #[test]
    fn a_pattern_matches_the_way_a_branch_name_is_actually_spelled() {
        assert!(matches_pattern("main", "main"));
        assert!(!matches_pattern("main", "maintenance"));
        assert!(matches_pattern("*", "anything/at/all"));
        assert!(matches_pattern("release/*", "release/2.0"));
        assert!(!matches_pattern("release/*", "releases/2.0"));
        assert!(matches_pattern("*-stable", "v2-stable"));
        assert!(!matches_pattern("*-stable", "v2-stable-rc"));
        assert!(matches_pattern("feat/*/docs", "feat/nav/docs"));
        assert!(!matches_pattern("feat/*/docs", "feat/nav/src"));
    }

    #[test]
    fn an_untrusted_worker_starts_with_an_allow_list_not_a_deny_list() {
        let ambient = [
            ("PATH", "/usr/bin"),
            ("HOME", "/home/build"),
            ("LIYASA_CACHE", "/var/cache"),
            ("GITHUB_TOKEN", "ghs_secret"),
            ("AWS_SECRET_ACCESS_KEY", "aws_secret"),
            ("SOMETHING_NOBODY_ANTICIPATED", "also secret"),
        ];
        let kept = keep_only(ambient);
        assert_eq!(kept.len(), 3);
        assert!(kept.contains_key("PATH"));
        assert!(kept.contains_key("LIYASA_CACHE"));
        for leaked in [
            "GITHUB_TOKEN",
            "AWS_SECRET_ACCESS_KEY",
            "SOMETHING_NOBODY_ANTICIPATED",
        ] {
            assert!(!kept.contains_key(leaked), "{leaked} reached the worker");
        }
    }

    #[test]
    fn an_untrusted_build_cannot_see_a_token_even_through_build_env() {
        let sandbox = Sandbox::for_reason(
            &Reason::Fork,
            [("PATH", "/usr/bin"), ("GITHUB_TOKEN", "ghs_secret")],
            "patch-1",
            "main",
        );
        assert!(!sandbox.allow_build_env);
        assert!(!sandbox.sees("GITHUB_TOKEN"));
        assert!(
            !sandbox.sees("PATH"),
            "`env()` returns undefined for everything, not just for secrets"
        );
        assert!(
            !sandbox.env.contains_key("GITHUB_TOKEN"),
            "and the variable is not in the process at all"
        );
    }

    #[test]
    fn an_untrusted_build_reads_its_trust_plane_from_the_deploy_branch() {
        let sandbox = Sandbox::for_reason(&Reason::Fork, [("PATH", "/usr/bin")], "patch-1", "main");
        assert_eq!(
            sandbox.trust_plane_ref, "main",
            "never from the pull request"
        );
        assert!(!sandbox.refresh_sources);
        assert!(!sandbox.run_command_sources);
        assert!(!sandbox.allow_credentialed_runners);
        assert_eq!(sandbox.label, UNTRUSTED_LABEL);
    }

    #[test]
    fn a_trusted_build_keeps_its_environment_and_its_own_trust_plane() {
        let sandbox = Sandbox::for_reason(
            &Reason::Trusted,
            [("PATH", "/usr/bin"), ("GITHUB_TOKEN", "ghs_secret")],
            "release/2",
            "main",
        );
        assert!(sandbox.allow_build_env);
        assert!(sandbox.sees("GITHUB_TOKEN"));
        assert_eq!(sandbox.trust_plane_ref, "release/2");
        assert!(sandbox.run_command_sources);
        assert_eq!(sandbox.label, TRUSTED_LABEL);
    }

    #[test]
    fn a_sandbox_round_trips_as_json_so_a_worker_reads_the_same_decision() {
        let sandbox = Sandbox::for_reason(&Reason::Fork, [("PATH", "/usr/bin")], "p", "main");
        let text = serde_json::to_string(&sandbox).expect("a sandbox serializes");
        let back: Sandbox = serde_json::from_str(&text).expect("it deserializes");
        assert_eq!(back, sandbox);
    }
}
