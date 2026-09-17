//! GitLab, on gitlab.com and self-managed (GIT-02).
//!
//! Two shapes differ from GitHub and both matter. A project is addressed by
//! its URL-encoded path, so `team/platform/docs` becomes one path segment.
//! And a multi-file change is one `commits` call with a list of actions rather
//! than a tree built from blobs, which makes the proposal a single request.

use base64::Engine as _;
use liyasa_core::net::{BoxFut, HostSet, HttpClient, HttpPolicy, Method, Url};
use serde_json::{Value, json};

use crate::provider::{
    Author, CheckRun, CheckRunRef, Endpoint, FileChange, GitError, GitProvider, NewPullRequest,
    PullRequestRef, TokenSource, json_request, policy, send,
};
use crate::repo::RepoRef;
use crate::webhook::Provider;

const NAME: &str = "gitlab";

/// Percent-encodes a project path so `team/platform/docs` is one path segment.
///
/// Only the characters GitLab's own documentation calls out need encoding, and
/// the set is small enough to spell out rather than take a dependency for.
pub fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 8);
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

pub struct GitLab {
    endpoint: Endpoint,
    http: std::sync::Arc<dyn HttpClient>,
    tokens: std::sync::Arc<dyn TokenSource>,
    policy: HttpPolicy,
}

impl std::fmt::Debug for GitLab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitLab")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl GitLab {
    pub fn new(
        endpoint: Endpoint,
        http: std::sync::Arc<dyn HttpClient>,
        tokens: std::sync::Arc<dyn TokenSource>,
    ) -> Self {
        Self {
            endpoint,
            http,
            tokens,
            policy: policy(HostSet::default()),
        }
    }

    pub fn with_allowed_hosts(mut self, hosts: HostSet) -> Self {
        self.policy = policy(hosts);
        self
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    fn url(&self, repo: &RepoRef, suffix: &str) -> Result<Url, GitError> {
        self.endpoint.join(&format!(
            "projects/{}/{suffix}",
            encode_path(&repo.full_name())
        ))
    }

    async fn call(&self, method: Method, url: Url, body: Option<Value>) -> Result<Value, GitError> {
        let token = self.tokens.token().await?;
        let request = json_request(method, url, &token, &[], body.as_ref());
        send(self.http.as_ref(), NAME, request, &self.policy).await
    }

    fn string_at(value: &Value, pointer: &str, field: &'static str) -> Result<String, GitError> {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(GitError::Malformed {
                provider: NAME,
                field,
            })
    }
}

impl GitProvider for GitLab {
    fn kind(&self) -> Provider {
        Provider::GitLab
    }

    fn branch_head<'a>(
        &'a self,
        repo: &'a RepoRef,
        branch: &'a str,
    ) -> BoxFut<'a, Result<String, GitError>> {
        Box::pin(async move {
            let url = self.url(
                repo,
                &format!("repository/branches/{}", encode_path(branch)),
            )?;
            let value = self.call(Method::GET, url, None).await?;
            Self::string_at(&value, "/commit/id", "commit.id")
        })
    }

    fn create_branch<'a>(
        &'a self,
        repo: &'a RepoRef,
        branch: &'a str,
        from_sha: &'a str,
    ) -> BoxFut<'a, Result<(), GitError>> {
        Box::pin(async move {
            let url = self.url(repo, "repository/branches")?;
            self.call(
                Method::POST,
                url,
                Some(json!({ "branch": branch, "ref": from_sha })),
            )
            .await?;
            Ok(())
        })
    }

    fn commit<'a>(
        &'a self,
        repo: &'a RepoRef,
        branch: &'a str,
        message: &'a str,
        author: &'a Author,
        changes: &'a [FileChange],
    ) -> BoxFut<'a, Result<String, GitError>> {
        Box::pin(async move {
            // GitLab distinguishes creating a file from updating one, and
            // answers 400 for the wrong verb. Asking what exists first would
            // be one request per file; `update` after a failed `create` would
            // be two. `action: "update"` with `overwrite` semantics does not
            // exist, so the file list is checked in one tree listing.
            let existing = self.existing_paths(repo, branch).await?;
            let actions: Vec<Value> = changes
                .iter()
                .map(|change| match &change.content {
                    None => json!({ "action": "delete", "file_path": change.path }),
                    Some(bytes) => json!({
                        "action": match existing.contains(&change.path) {
                            true => "update",
                            false => "create",
                        },
                        "file_path": change.path,
                        "content": base64::engine::general_purpose::STANDARD.encode(bytes),
                        "encoding": "base64",
                    }),
                })
                .collect();
            let url = self.url(repo, "repository/commits")?;
            let value = self
                .call(
                    Method::POST,
                    url,
                    Some(json!({
                        "branch": branch,
                        "commit_message": message,
                        "author_name": author.name,
                        "author_email": author.email,
                        "actions": actions,
                    })),
                )
                .await?;
            Self::string_at(&value, "/id", "id")
        })
    }

    fn open_pull_request<'a>(
        &'a self,
        repo: &'a RepoRef,
        request: &'a NewPullRequest,
    ) -> BoxFut<'a, Result<PullRequestRef, GitError>> {
        Box::pin(async move {
            let url = self.url(repo, "merge_requests")?;
            let value = self
                .call(
                    Method::POST,
                    url,
                    Some(json!({
                        // A draft merge request is spelled in the title.
                        "title": match request.draft {
                            true => format!("Draft: {}", request.title),
                            false => request.title.clone(),
                        },
                        "description": request.body,
                        "source_branch": request.head,
                        "target_branch": request.base,
                    })),
                )
                .await?;
            Ok(PullRequestRef {
                number: value
                    .get("iid")
                    .and_then(Value::as_u64)
                    .ok_or(GitError::Malformed {
                        provider: NAME,
                        field: "iid",
                    })?,
                url: Self::string_at(&value, "/web_url", "web_url")?,
                head_sha: Self::string_at(&value, "/sha", "sha")?,
            })
        })
    }

    fn comment<'a>(
        &'a self,
        repo: &'a RepoRef,
        number: u64,
        body: &'a str,
    ) -> BoxFut<'a, Result<(), GitError>> {
        Box::pin(async move {
            let url = self.url(repo, &format!("merge_requests/{number}/notes"))?;
            self.call(Method::POST, url, Some(json!({ "body": body })))
                .await?;
            Ok(())
        })
    }

    fn report_check<'a>(
        &'a self,
        repo: &'a RepoRef,
        run: &'a CheckRun,
    ) -> BoxFut<'a, Result<CheckRunRef, GitError>> {
        Box::pin(async move {
            // GitLab has commit statuses rather than check runs. There is no
            // annotation surface, so the diagnostics are summarised into the
            // description, which is what a reviewer sees on the pipeline.
            let state = match (run.status, run.conclusion) {
                (crate::provider::CheckStatus::Queued, _) => "pending",
                (crate::provider::CheckStatus::InProgress, _) => "running",
                (_, Some(conclusion)) if conclusion.passes() => "success",
                (_, Some(crate::provider::CheckConclusion::Cancelled)) => "canceled",
                (_, _) => "failed",
            };
            let mut body = json!({
                "state": state,
                "name": run.name,
                "description": match run.title.is_empty() {
                    true => run.summary.clone(),
                    false => run.title.clone(),
                },
            });
            if let Some(url) = &run.details_url {
                body["target_url"] = Value::String(url.clone());
            }
            let url = self.url(repo, &format!("statuses/{}", run.head_sha))?;
            let value = self.call(Method::POST, url, Some(body)).await?;
            Ok(CheckRunRef {
                id: value
                    .get("id")
                    .map(|id| match id {
                        Value::Number(n) => n.to_string(),
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .ok_or(GitError::Malformed {
                        provider: NAME,
                        field: "id",
                    })?,
                url: value
                    .get("target_url")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            })
        })
    }
}

impl GitLab {
    /// The file paths on a branch, so a commit knows which actions are
    /// `create` and which are `update`.
    async fn existing_paths(&self, repo: &RepoRef, branch: &str) -> Result<Vec<String>, GitError> {
        let url = self.url(
            repo,
            &format!(
                "repository/tree?ref={}&recursive=true&per_page=100",
                encode_path(branch)
            ),
        )?;
        let value = self.call(Method::GET, url, None).await?;
        Ok(value
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("blob"))
                    .filter_map(|entry| entry.get("path").and_then(Value::as_str))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::provider::{CheckConclusion, StaticToken};
    use crate::recorder::{Exchange, Recorded};

    fn client(exchanges: Vec<Exchange>) -> (GitLab, Arc<Recorded>) {
        let recorded = Arc::new(Recorded::new(exchanges));
        let gitlab = GitLab::new(
            Endpoint::gitlab_com(),
            recorded.clone(),
            Arc::new(StaticToken::new("glpat_test")),
        );
        (gitlab, recorded)
    }

    fn repo() -> RepoRef {
        RepoRef::new("team/platform", "docs")
    }

    #[test]
    fn a_nested_project_path_becomes_one_encoded_segment() {
        assert_eq!(encode_path("team/platform/docs"), "team%2Fplatform%2Fdocs");
        assert_eq!(encode_path("feat/new nav"), "feat%2Fnew%20nav");
        assert_eq!(encode_path("simple-name_1.0~x"), "simple-name_1.0~x");
    }

    #[tokio::test]
    async fn a_branch_head_is_read_from_the_encoded_project_path() {
        let (gitlab, recorded) = client(vec![Exchange::get(
            "/api/v4/projects/team%2Fplatform%2Fdocs/repository/branches/main",
            200,
            r#"{"commit":{"id":"deadbeef"}}"#,
        )]);
        assert_eq!(
            gitlab.branch_head(&repo(), "main").await.expect("the head"),
            "deadbeef"
        );
        assert!(
            recorded.made()[0].url.contains("team%2Fplatform%2Fdocs"),
            "{}",
            recorded.made()[0].url
        );
    }

    #[tokio::test]
    async fn a_proposal_is_one_commits_call_with_create_update_and_delete_actions() {
        let (gitlab, recorded) = client(vec![
            Exchange::get(
                "/api/v4/projects/team%2Fplatform%2Fdocs/repository/tree",
                200,
                r#"[{"type":"blob","path":"docs/existing.md"},{"type":"tree","path":"docs"}]"#,
            ),
            Exchange::post(
                "/api/v4/projects/team%2Fplatform%2Fdocs/repository/commits",
                201,
                r#"{"id":"new-commit"}"#,
            ),
        ]);
        let changes = [
            FileChange {
                path: "docs/existing.md".to_owned(),
                content: Some(b"updated".to_vec()),
            },
            FileChange {
                path: "docs/new.md".to_owned(),
                content: Some(b"new".to_vec()),
            },
            FileChange {
                path: "docs/gone.md".to_owned(),
                content: None,
            },
        ];
        let sha = gitlab
            .commit(
                &repo(),
                "proposal-1",
                "docs: rewrite",
                &Author {
                    name: "A Writer".to_owned(),
                    email: "writer@example.com".to_owned(),
                },
                &changes,
            )
            .await
            .expect("the commit lands");
        assert_eq!(sha, "new-commit");

        let body = recorded
            .made()
            .into_iter()
            .find(|call| call.url.ends_with("/repository/commits"))
            .expect("a commits call")
            .json();
        assert_eq!(body["actions"][0]["action"], "update");
        assert_eq!(body["actions"][1]["action"], "create");
        assert_eq!(body["actions"][2]["action"], "delete");
        assert_eq!(body["actions"][1]["encoding"], "base64");
        assert_eq!(body["author_email"], "writer@example.com");
        assert_eq!(recorded.unused(), 0);
    }

    #[tokio::test]
    async fn a_draft_merge_request_says_so_in_its_title() {
        let (gitlab, recorded) = client(vec![Exchange::post(
            "/api/v4/projects/team%2Fplatform%2Fdocs/merge_requests",
            201,
            r#"{"iid":12,"web_url":"https://gitlab.com/team/platform/docs/-/merge_requests/12","sha":"abc"}"#,
        )]);
        let reference = gitlab
            .open_pull_request(
                &repo(),
                &NewPullRequest {
                    title: "docs: fix the install guide".to_owned(),
                    body: "Proposed by the maintenance agent.".to_owned(),
                    head: "proposal-1".to_owned(),
                    base: "main".to_owned(),
                    draft: true,
                },
            )
            .await
            .expect("the merge request opens");
        assert_eq!(reference.number, 12);
        assert_eq!(
            recorded.made()[0].json()["title"],
            "Draft: docs: fix the install guide"
        );
    }

    #[tokio::test]
    async fn a_check_becomes_a_commit_status_with_the_matching_state() {
        for (conclusion, expected) in [
            (CheckConclusion::Success, "success"),
            (CheckConclusion::Neutral, "success"),
            (CheckConclusion::Failure, "failed"),
            (CheckConclusion::Cancelled, "canceled"),
        ] {
            let (gitlab, recorded) = client(vec![Exchange::post(
                "/api/v4/projects/team%2Fplatform%2Fdocs/statuses/abc",
                201,
                r#"{"id":5,"target_url":"https://liyasa.example/builds/1"}"#,
            )]);
            gitlab
                .report_check(
                    &repo(),
                    &CheckRun::completed("liyasa", "abc", conclusion)
                        .with_output("1 error", "E0401")
                        .with_details_url("https://liyasa.example/builds/1"),
                )
                .await
                .expect("the status posts");
            assert_eq!(
                recorded.made()[0].json()["state"],
                expected,
                "{}",
                conclusion.as_str()
            );
        }
    }

    #[tokio::test]
    async fn a_queued_check_is_a_pending_status() {
        let (gitlab, recorded) = client(vec![Exchange::post(
            "/api/v4/projects/team%2Fplatform%2Fdocs/statuses/abc",
            201,
            r#"{"id":5}"#,
        )]);
        gitlab
            .report_check(&repo(), &CheckRun::queued("liyasa", "abc"))
            .await
            .expect("the status posts");
        assert_eq!(recorded.made()[0].json()["state"], "pending");
    }

    #[tokio::test]
    async fn a_self_managed_instance_calls_its_own_host() {
        let recorded = Arc::new(Recorded::new(vec![Exchange::get(
            "/api/v4/projects/team%2Fplatform%2Fdocs/repository/branches/main",
            200,
            r#"{"commit":{"id":"aa"}}"#,
        )]));
        let gitlab = GitLab::new(
            Endpoint::parse(
                "https://gitlab.example.com/api/v4/",
                "https://gitlab.example.com/",
            )
            .expect("a base"),
            recorded.clone(),
            Arc::new(StaticToken::new("glpat_test")),
        );
        assert_eq!(
            gitlab.branch_head(&repo(), "main").await.expect("the head"),
            "aa"
        );
        assert!(
            recorded.made()[0]
                .url
                .starts_with("https://gitlab.example.com/api/v4/"),
            "{}",
            recorded.made()[0].url
        );
    }

    #[tokio::test]
    async fn a_note_goes_on_the_merge_request() {
        let (gitlab, recorded) = client(vec![Exchange::post(
            "/api/v4/projects/team%2Fplatform%2Fdocs/merge_requests/12/notes",
            201,
            r#"{"id":1}"#,
        )]);
        gitlab
            .comment(&repo(), 12, "Preview: https://liyasa.example/mr-12")
            .await
            .expect("the note posts");
        assert_eq!(
            recorded.made()[0].json()["body"],
            "Preview: https://liyasa.example/mr-12"
        );
    }
}
