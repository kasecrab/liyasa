//! What a verified delivery turns out to be (GIT-04, GIT-30).
//!
//! Four providers describe a push four ways. Everything downstream — the
//! deploy queue, the preview lifecycle, the untrusted-build rules — works on
//! [`Event`], so adding a provider is a parser and nothing else.
//!
//! An event is parsed only after [`crate::webhook::Verifier`] has accepted the
//! delivery. Parsing an unverified body is the mistake GIT-01 exists to
//! prevent, so nothing here takes raw bytes: the input is a [`Verified`] and
//! the payload together.

use serde_json::Value;

use crate::repo::{RepoRef, branch_of_ref};
use crate::webhook::{Provider, Verified};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Push(Push),
    PullRequest(PullRequestEvent),
    /// Verified, understood to be none of our business. Kept as a value rather
    /// than an error so a caller can log what it ignored.
    Ignored {
        kind: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Push {
    pub repo: RepoRef,
    pub branch: String,
    /// The commit the branch now points at. Empty for a deletion.
    pub head: String,
    pub before: Option<String>,
    pub deleted: bool,
    /// Every path the push touched, sorted and de-duplicated. Empty means the
    /// provider did not say, not that nothing changed — a caller that needs
    /// certainty diffs the two commits itself.
    pub changed: Vec<String>,
    /// The GitHub App installation the delivery came from.
    pub installation: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrAction {
    Opened,
    Reopened,
    /// A new commit on the head branch: the preview rebuilds (GIT-30).
    Synchronized,
    Closed,
    Merged,
    Other,
}

impl PrAction {
    /// Whether this action should produce or refresh a preview.
    pub fn builds(self) -> bool {
        matches!(self, Self::Opened | Self::Reopened | Self::Synchronized)
    }

    /// Whether this action starts the preview's deletion clock (GIT-30).
    pub fn retires(self) -> bool {
        matches!(self, Self::Closed | Self::Merged)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestEvent {
    pub repo: RepoRef,
    pub number: u64,
    pub action: PrAction,
    pub head_branch: String,
    pub head_sha: String,
    pub base_branch: String,
    /// A pull request from a fork is never trusted (GIT-31).
    pub from_fork: bool,
    pub draft: bool,
    pub installation: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("the payload is not JSON: {0}")]
    NotJson(String),
    #[error("`{0}` is missing or not the shape this event needs")]
    Missing(&'static str),
    #[error("`{0}` is not a repository name of the form `owner/name`")]
    BadRepo(String),
}

/// Parses a verified delivery.
pub fn parse(verified: &Verified, body: &[u8]) -> Result<Event, ParseError> {
    let payload: Value =
        serde_json::from_slice(body).map_err(|error| ParseError::NotJson(error.to_string()))?;
    match verified.provider {
        Provider::GitHub | Provider::Generic => github(&verified.event, &payload),
        Provider::GitLab => gitlab(&verified.event, &payload),
        Provider::Bitbucket => bitbucket(&verified.event, &payload),
    }
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

fn repo_at(value: &Value, pointer: &str, key: &'static str) -> Result<RepoRef, ParseError> {
    let full = value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or(ParseError::Missing(key))?;
    RepoRef::parse(full).ok_or_else(|| ParseError::BadRepo(full.to_owned()))
}

/// `added`, `modified` and `removed` across every commit in the push.
fn github_changed(payload: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(commits) = payload.get("commits").and_then(Value::as_array) {
        for commit in commits {
            for field in ["added", "modified", "removed"] {
                if let Some(paths) = commit.get(field).and_then(Value::as_array) {
                    out.extend(paths.iter().filter_map(Value::as_str).map(str::to_owned));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn installation_of(payload: &Value) -> Option<u64> {
    payload.pointer("/installation/id")?.as_u64()
}

/// `0000000000000000000000000000000000000000` is how git spells "no commit".
fn is_zero_sha(sha: &str) -> bool {
    !sha.is_empty() && sha.bytes().all(|b| b == b'0')
}

fn github(event: &str, payload: &Value) -> Result<Event, ParseError> {
    match event {
        "push" => {
            let git_ref = text(payload, "ref").ok_or(ParseError::Missing("ref"))?;
            let Some(branch) = branch_of_ref(git_ref) else {
                return Ok(Event::Ignored {
                    kind: format!("push to {git_ref}"),
                });
            };
            let after = text(payload, "after").unwrap_or_default();
            let before = text(payload, "before").filter(|sha| !is_zero_sha(sha));
            Ok(Event::Push(Push {
                repo: repo_at(payload, "/repository/full_name", "repository.full_name")?,
                branch: branch.to_owned(),
                deleted: payload
                    .get("deleted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    || is_zero_sha(after),
                head: if is_zero_sha(after) {
                    String::new()
                } else {
                    after.to_owned()
                },
                before: before.map(str::to_owned),
                changed: github_changed(payload),
                installation: installation_of(payload),
            }))
        }
        "pull_request" => {
            let action = match text(payload, "action").unwrap_or_default() {
                "opened" => PrAction::Opened,
                "reopened" => PrAction::Reopened,
                "synchronize" => PrAction::Synchronized,
                "closed" => match payload.pointer("/pull_request/merged") {
                    Some(Value::Bool(true)) => PrAction::Merged,
                    _ => PrAction::Closed,
                },
                _ => PrAction::Other,
            };
            let pull = payload
                .get("pull_request")
                .ok_or(ParseError::Missing("pull_request"))?;
            let number = pull
                .get("number")
                .and_then(Value::as_u64)
                .or_else(|| payload.get("number").and_then(Value::as_u64))
                .ok_or(ParseError::Missing("pull_request.number"))?;
            Ok(Event::PullRequest(PullRequestEvent {
                repo: repo_at(payload, "/repository/full_name", "repository.full_name")?,
                number,
                action,
                head_branch: pull
                    .pointer("/head/ref")
                    .and_then(Value::as_str)
                    .ok_or(ParseError::Missing("pull_request.head.ref"))?
                    .to_owned(),
                head_sha: pull
                    .pointer("/head/sha")
                    .and_then(Value::as_str)
                    .ok_or(ParseError::Missing("pull_request.head.sha"))?
                    .to_owned(),
                base_branch: pull
                    .pointer("/base/ref")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                // A head repository that differs from the base repository is a
                // fork, and GitHub says so outright with `head.repo.fork`.
                from_fork: pull
                    .pointer("/head/repo/fork")
                    .and_then(Value::as_bool)
                    .unwrap_or_else(|| {
                        let head = pull.pointer("/head/repo/full_name").and_then(Value::as_str);
                        let base = payload
                            .pointer("/repository/full_name")
                            .and_then(Value::as_str);
                        // Absent head repository means a deleted fork, which is
                        // the untrusted case, so this defaults to true.
                        head.is_none() || head != base
                    }),
                draft: pull.get("draft").and_then(Value::as_bool).unwrap_or(false),
                installation: installation_of(payload),
            }))
        }
        other => Ok(Event::Ignored {
            kind: other.to_owned(),
        }),
    }
}

fn gitlab(event: &str, payload: &Value) -> Result<Event, ParseError> {
    match event {
        "Push Hook" | "Tag Push Hook" => {
            let git_ref = text(payload, "ref").ok_or(ParseError::Missing("ref"))?;
            let Some(branch) = branch_of_ref(git_ref) else {
                return Ok(Event::Ignored {
                    kind: format!("push to {git_ref}"),
                });
            };
            let after = text(payload, "after").unwrap_or_default();
            Ok(Event::Push(Push {
                repo: repo_at(
                    payload,
                    "/project/path_with_namespace",
                    "project.path_with_namespace",
                )?,
                branch: branch.to_owned(),
                deleted: is_zero_sha(after),
                head: if is_zero_sha(after) {
                    String::new()
                } else {
                    after.to_owned()
                },
                before: text(payload, "before")
                    .filter(|sha| !is_zero_sha(sha))
                    .map(str::to_owned),
                changed: github_changed(payload),
                installation: None,
            }))
        }
        "Merge Request Hook" => {
            let attributes = payload
                .get("object_attributes")
                .ok_or(ParseError::Missing("object_attributes"))?;
            let action = match attributes.get("action").and_then(Value::as_str) {
                Some("open") => PrAction::Opened,
                Some("reopen") => PrAction::Reopened,
                Some("update") => PrAction::Synchronized,
                Some("merge") => PrAction::Merged,
                Some("close") => PrAction::Closed,
                _ => PrAction::Other,
            };
            let source = attributes.pointer("/source/path_with_namespace");
            let target = payload.pointer("/project/path_with_namespace");
            Ok(Event::PullRequest(PullRequestEvent {
                repo: repo_at(
                    payload,
                    "/project/path_with_namespace",
                    "project.path_with_namespace",
                )?,
                number: attributes
                    .get("iid")
                    .and_then(Value::as_u64)
                    .ok_or(ParseError::Missing("object_attributes.iid"))?,
                action,
                head_branch: attributes
                    .get("source_branch")
                    .and_then(Value::as_str)
                    .ok_or(ParseError::Missing("object_attributes.source_branch"))?
                    .to_owned(),
                head_sha: attributes
                    .pointer("/last_commit/id")
                    .and_then(Value::as_str)
                    .ok_or(ParseError::Missing("object_attributes.last_commit.id"))?
                    .to_owned(),
                base_branch: attributes
                    .get("target_branch")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                from_fork: source.is_none() || source != target,
                draft: attributes
                    .get("work_in_progress")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                installation: None,
            }))
        }
        other => Ok(Event::Ignored {
            kind: other.to_owned(),
        }),
    }
}

fn bitbucket(event: &str, payload: &Value) -> Result<Event, ParseError> {
    match event {
        "repo:push" => {
            let changes = payload
                .pointer("/push/changes")
                .and_then(Value::as_array)
                .ok_or(ParseError::Missing("push.changes"))?;
            // Bitbucket batches several ref updates into one delivery. Only a
            // branch matters, and the first branch in the batch is the one the
            // deploy acts on; the rest are reported by their own deliveries.
            let Some(change) = changes.iter().find(|change| {
                change.pointer("/new/type").and_then(Value::as_str) == Some("branch")
                    || change.pointer("/old/type").and_then(Value::as_str) == Some("branch")
            }) else {
                return Ok(Event::Ignored {
                    kind: "repo:push with no branch".to_owned(),
                });
            };
            let deleted = change.get("new").is_none_or(Value::is_null);
            let side = if deleted { "/old" } else { "/new" };
            let branch = change
                .pointer(&format!("{side}/name"))
                .and_then(Value::as_str)
                .ok_or(ParseError::Missing("push.changes[].new.name"))?;
            Ok(Event::Push(Push {
                repo: repo_at(payload, "/repository/full_name", "repository.full_name")?,
                branch: branch.to_owned(),
                head: change
                    .pointer("/new/target/hash")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                before: change
                    .pointer("/old/target/hash")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                deleted,
                // Bitbucket does not list paths in the delivery; the caller
                // diffs `before..head` when it needs them.
                changed: Vec::new(),
                installation: None,
            }))
        }
        other if other.starts_with("pullrequest:") => {
            let action = match other {
                "pullrequest:created" => PrAction::Opened,
                "pullrequest:updated" => PrAction::Synchronized,
                "pullrequest:fulfilled" => PrAction::Merged,
                "pullrequest:rejected" => PrAction::Closed,
                _ => PrAction::Other,
            };
            let pull = payload
                .get("pullrequest")
                .ok_or(ParseError::Missing("pullrequest"))?;
            let source = pull.pointer("/source/repository/full_name");
            let target = payload.pointer("/repository/full_name");
            Ok(Event::PullRequest(PullRequestEvent {
                repo: repo_at(payload, "/repository/full_name", "repository.full_name")?,
                number: pull
                    .get("id")
                    .and_then(Value::as_u64)
                    .ok_or(ParseError::Missing("pullrequest.id"))?,
                action,
                head_branch: pull
                    .pointer("/source/branch/name")
                    .and_then(Value::as_str)
                    .ok_or(ParseError::Missing("pullrequest.source.branch.name"))?
                    .to_owned(),
                head_sha: pull
                    .pointer("/source/commit/hash")
                    .and_then(Value::as_str)
                    .ok_or(ParseError::Missing("pullrequest.source.commit.hash"))?
                    .to_owned(),
                base_branch: pull
                    .pointer("/destination/branch/name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                from_fork: source.is_none() || source != target,
                draft: false,
                installation: None,
            }))
        }
        other => Ok(Event::Ignored {
            kind: other.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn verified(provider: Provider, event: &str) -> Verified {
        Verified {
            provider,
            event: event.to_owned(),
            delivery: "d-1".to_owned(),
        }
    }

    fn parse_json(provider: Provider, event: &str, payload: Value) -> Event {
        parse(&verified(provider, event), payload.to_string().as_bytes())
            .expect("a well-formed payload")
    }

    #[test]
    fn a_github_push_names_its_branch_commit_and_changed_paths() {
        let event = parse_json(
            Provider::GitHub,
            "push",
            json!({
                "ref": "refs/heads/main",
                "before": "1111111111111111111111111111111111111111",
                "after": "2222222222222222222222222222222222222222",
                "repository": { "full_name": "kasecrab/liyasa" },
                "installation": { "id": 4242 },
                "commits": [
                    { "added": ["docs/new.md"], "modified": ["docs/index.md"], "removed": [] },
                    { "added": [], "modified": ["docs/index.md"], "removed": ["docs/old.md"] }
                ]
            }),
        );
        let Event::Push(push) = event else {
            panic!("a push");
        };
        assert_eq!(push.repo.full_name(), "kasecrab/liyasa");
        assert_eq!(push.branch, "main");
        assert_eq!(push.head, "2".repeat(40));
        assert_eq!(push.before.as_deref(), Some("1".repeat(40).as_str()));
        assert_eq!(push.installation, Some(4242));
        assert_eq!(
            push.changed,
            ["docs/index.md", "docs/new.md", "docs/old.md"],
            "sorted and de-duplicated across commits"
        );
        assert!(!push.deleted);
    }

    #[test]
    fn a_tag_push_is_ignored_rather_than_deployed() {
        let event = parse_json(
            Provider::GitHub,
            "push",
            json!({
                "ref": "refs/tags/v1.0.0",
                "after": "3333333333333333333333333333333333333333",
                "repository": { "full_name": "kasecrab/liyasa" }
            }),
        );
        assert!(matches!(event, Event::Ignored { .. }), "{event:?}");
    }

    #[test]
    fn a_branch_deletion_is_a_push_with_no_head() {
        let event = parse_json(
            Provider::GitHub,
            "push",
            json!({
                "ref": "refs/heads/gone",
                "before": "4444444444444444444444444444444444444444",
                "after": "0000000000000000000000000000000000000000",
                "deleted": true,
                "repository": { "full_name": "kasecrab/liyasa" }
            }),
        );
        let Event::Push(push) = event else {
            panic!("a push");
        };
        assert!(push.deleted);
        assert!(push.head.is_empty());
        assert_eq!(push.branch, "gone");
    }

    #[test]
    fn a_first_push_to_a_new_branch_has_no_before() {
        let event = parse_json(
            Provider::GitHub,
            "push",
            json!({
                "ref": "refs/heads/new",
                "before": "0000000000000000000000000000000000000000",
                "after": "5555555555555555555555555555555555555555",
                "repository": { "full_name": "kasecrab/liyasa" }
            }),
        );
        let Event::Push(push) = event else {
            panic!("a push");
        };
        assert_eq!(push.before, None);
        assert!(!push.deleted);
    }

    #[test]
    fn a_fork_pull_request_is_marked_untrusted() {
        let event = parse_json(
            Provider::GitHub,
            "pull_request",
            json!({
                "action": "opened",
                "repository": { "full_name": "kasecrab/liyasa" },
                "pull_request": {
                    "number": 7,
                    "draft": false,
                    "head": {
                        "ref": "patch-1",
                        "sha": "6666666666666666666666666666666666666666",
                        "repo": { "fork": true, "full_name": "stranger/liyasa" }
                    },
                    "base": { "ref": "main" }
                }
            }),
        );
        let Event::PullRequest(pull) = event else {
            panic!("a pull request");
        };
        assert_eq!(pull.number, 7);
        assert_eq!(pull.action, PrAction::Opened);
        assert!(pull.from_fork);
        assert!(pull.action.builds());
        assert!(!pull.action.retires());
    }

    #[test]
    fn a_pull_request_whose_head_repository_is_gone_is_treated_as_a_fork() {
        let event = parse_json(
            Provider::GitHub,
            "pull_request",
            json!({
                "action": "synchronize",
                "repository": { "full_name": "kasecrab/liyasa" },
                "pull_request": {
                    "number": 8,
                    "head": { "ref": "patch-2", "sha": "77", "repo": null },
                    "base": { "ref": "main" }
                }
            }),
        );
        let Event::PullRequest(pull) = event else {
            panic!("a pull request");
        };
        assert!(
            pull.from_fork,
            "an unknown provenance defaults to untrusted, not to trusted"
        );
        assert_eq!(pull.action, PrAction::Synchronized);
    }

    #[test]
    fn a_branch_pull_request_in_the_same_repository_is_trusted() {
        let event = parse_json(
            Provider::GitHub,
            "pull_request",
            json!({
                "action": "opened",
                "repository": { "full_name": "kasecrab/liyasa" },
                "pull_request": {
                    "number": 9,
                    "head": {
                        "ref": "feat/x",
                        "sha": "88",
                        "repo": { "fork": false, "full_name": "kasecrab/liyasa" }
                    },
                    "base": { "ref": "main" }
                }
            }),
        );
        let Event::PullRequest(pull) = event else {
            panic!("a pull request");
        };
        assert!(!pull.from_fork);
    }

    #[test]
    fn a_merged_pull_request_is_not_the_same_action_as_a_closed_one() {
        for (merged, expected) in [(true, PrAction::Merged), (false, PrAction::Closed)] {
            let event = parse_json(
                Provider::GitHub,
                "pull_request",
                json!({
                    "action": "closed",
                    "repository": { "full_name": "kasecrab/liyasa" },
                    "pull_request": {
                        "number": 10,
                        "merged": merged,
                        "head": { "ref": "f", "sha": "99", "repo": { "fork": false, "full_name": "kasecrab/liyasa" } },
                        "base": { "ref": "main" }
                    }
                }),
            );
            let Event::PullRequest(pull) = event else {
                panic!("a pull request");
            };
            assert_eq!(pull.action, expected);
            assert!(pull.action.retires());
            assert!(!pull.action.builds());
        }
    }

    #[test]
    fn a_gitlab_push_uses_the_namespaced_project_path() {
        let event = parse_json(
            Provider::GitLab,
            "Push Hook",
            json!({
                "object_kind": "push",
                "ref": "refs/heads/main",
                "before": "aaaa",
                "after": "bbbb",
                "project": { "path_with_namespace": "team/platform/docs" },
                "commits": [{ "added": ["a.md"], "modified": [], "removed": [] }]
            }),
        );
        let Event::Push(push) = event else {
            panic!("a push");
        };
        assert_eq!(push.repo.owner, "team/platform");
        assert_eq!(push.repo.name, "docs");
        assert_eq!(push.changed, ["a.md"]);
    }

    #[test]
    fn a_gitlab_merge_request_maps_its_actions() {
        for (action, expected) in [
            ("open", PrAction::Opened),
            ("update", PrAction::Synchronized),
            ("merge", PrAction::Merged),
            ("close", PrAction::Closed),
            ("approved", PrAction::Other),
        ] {
            let event = parse_json(
                Provider::GitLab,
                "Merge Request Hook",
                json!({
                    "project": { "path_with_namespace": "team/docs" },
                    "object_attributes": {
                        "iid": 11,
                        "action": action,
                        "source_branch": "topic",
                        "target_branch": "main",
                        "work_in_progress": true,
                        "source": { "path_with_namespace": "team/docs" },
                        "last_commit": { "id": "cccc" }
                    }
                }),
            );
            let Event::PullRequest(pull) = event else {
                panic!("a merge request");
            };
            assert_eq!(pull.action, expected, "{action}");
            assert_eq!(pull.number, 11);
            assert!(pull.draft);
            assert!(!pull.from_fork);
        }
    }

    #[test]
    fn a_bitbucket_push_reads_the_first_branch_change() {
        let event = parse_json(
            Provider::Bitbucket,
            "repo:push",
            json!({
                "repository": { "full_name": "team/docs" },
                "push": { "changes": [
                    { "new": { "type": "tag", "name": "v1", "target": { "hash": "dddd" } } },
                    { "new": { "type": "branch", "name": "main", "target": { "hash": "eeee" } },
                      "old": { "type": "branch", "name": "main", "target": { "hash": "ffff" } } }
                ]}
            }),
        );
        let Event::Push(push) = event else {
            panic!("a push");
        };
        assert_eq!(push.branch, "main");
        assert_eq!(push.head, "eeee");
        assert_eq!(push.before.as_deref(), Some("ffff"));
        assert!(push.changed.is_empty(), "bitbucket sends no path list");
    }

    #[test]
    fn a_bitbucket_branch_deletion_reads_the_old_side() {
        let event = parse_json(
            Provider::Bitbucket,
            "repo:push",
            json!({
                "repository": { "full_name": "team/docs" },
                "push": { "changes": [
                    { "new": null, "old": { "type": "branch", "name": "gone", "target": { "hash": "1234" } } }
                ]}
            }),
        );
        let Event::Push(push) = event else {
            panic!("a push");
        };
        assert!(push.deleted);
        assert_eq!(push.branch, "gone");
        assert!(push.head.is_empty());
    }

    #[test]
    fn a_bitbucket_pull_request_from_another_repository_is_a_fork() {
        let event = parse_json(
            Provider::Bitbucket,
            "pullrequest:created",
            json!({
                "repository": { "full_name": "team/docs" },
                "pullrequest": {
                    "id": 12,
                    "source": {
                        "branch": { "name": "patch" },
                        "commit": { "hash": "5678" },
                        "repository": { "full_name": "stranger/docs" }
                    },
                    "destination": { "branch": { "name": "main" } }
                }
            }),
        );
        let Event::PullRequest(pull) = event else {
            panic!("a pull request");
        };
        assert_eq!(pull.number, 12);
        assert!(pull.from_fork);
    }

    #[test]
    fn a_generic_host_is_read_with_the_github_shape() {
        let event = parse_json(
            Provider::Generic,
            "push",
            json!({
                "ref": "refs/heads/main",
                "after": "abcd",
                "repository": { "full_name": "self/hosted" }
            }),
        );
        let Event::Push(push) = event else {
            panic!("a push");
        };
        assert_eq!(push.repo.full_name(), "self/hosted");
    }

    #[test]
    fn a_payload_that_is_not_json_is_an_error_rather_than_a_panic() {
        let error = parse(&verified(Provider::GitHub, "push"), b"not json").expect_err("a refusal");
        assert!(matches!(error, ParseError::NotJson(_)), "{error}");
    }

    #[test]
    fn a_push_with_no_repository_is_an_error() {
        let error = parse(
            &verified(Provider::GitHub, "push"),
            json!({ "ref": "refs/heads/main", "after": "aa" })
                .to_string()
                .as_bytes(),
        )
        .expect_err("a refusal");
        assert_eq!(error, ParseError::Missing("repository.full_name"));
    }

    #[test]
    fn a_repository_name_with_no_owner_is_named_in_the_error() {
        let error = parse(
            &verified(Provider::GitHub, "push"),
            json!({ "ref": "refs/heads/main", "after": "aa", "repository": { "full_name": "liyasa" } })
                .to_string()
                .as_bytes(),
        )
        .expect_err("a refusal");
        assert_eq!(error, ParseError::BadRepo("liyasa".to_owned()));
    }

    #[test]
    fn an_event_we_do_not_act_on_is_ignored_by_name() {
        let event = parse_json(Provider::GitHub, "star", json!({}));
        assert_eq!(
            event,
            Event::Ignored {
                kind: "star".to_owned()
            }
        );
    }
}
