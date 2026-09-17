//! The GitHub App, and GitHub Enterprise Server on the same code (GIT-01).
//!
//! A proposal is one commit, not one commit per file: blobs, then a tree on
//! top of the branch's tree, then a commit, then a ref update. The contents
//! API would have been three lines shorter and would have put five commits on
//! a five-file proposal, which is what a reviewer would then have to read.

use base64::Engine as _;
use liyasa_core::net::{BoxFut, HostSet, HttpClient, HttpPolicy, Method, Url};
use serde_json::{Value, json};

use crate::provider::{
    Author, CheckRun, CheckRunRef, Endpoint, FileChange, GitError, GitProvider, NewPullRequest,
    PullRequestRef, TokenSource, json_request, policy, send,
};
use crate::repo::RepoRef;
use crate::webhook::Provider;

const NAME: &str = "github";

/// The version header GitHub requires on every REST call. Pinned rather than
/// left to the default, so an API change is a decision here and not a surprise
/// on a Tuesday.
pub const API_VERSION: &str = "2022-11-28";

pub struct GitHub {
    endpoint: Endpoint,
    http: std::sync::Arc<dyn HttpClient>,
    tokens: std::sync::Arc<dyn TokenSource>,
    policy: HttpPolicy,
}

impl std::fmt::Debug for GitHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHub")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl GitHub {
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

    /// Restricts provider traffic to the hosts an operator named. Left empty,
    /// the address checks of §30.2.3 still apply but any public host resolves.
    pub fn with_allowed_hosts(mut self, hosts: HostSet) -> Self {
        self.policy = policy(hosts);
        self
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    fn url(&self, repo: &RepoRef, suffix: &str) -> Result<Url, GitError> {
        self.endpoint
            .join(&format!("repos/{}/{}/{suffix}", repo.owner, repo.name))
    }

    async fn call(&self, method: Method, url: Url, body: Option<Value>) -> Result<Value, GitError> {
        let token = self.tokens.token().await?;
        let request = json_request(
            method,
            url,
            &token,
            &[
                ("accept", "application/vnd.github+json"),
                ("x-github-api-version", API_VERSION),
            ],
            body.as_ref(),
        );
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

    /// Uploads one file's bytes and returns the blob SHA. Base64 rather than
    /// `content` as text: an image or a PDF in a proposal is not UTF-8, and a
    /// silent lossy conversion would corrupt it.
    async fn blob(&self, repo: &RepoRef, bytes: &[u8]) -> Result<String, GitError> {
        let body = json!({
            "content": base64::engine::general_purpose::STANDARD.encode(bytes),
            "encoding": "base64",
        });
        let value = self
            .call(Method::POST, self.url(repo, "git/blobs")?, Some(body))
            .await?;
        Self::string_at(&value, "/sha", "sha")
    }
}

impl GitProvider for GitHub {
    fn kind(&self) -> Provider {
        Provider::GitHub
    }

    fn branch_head<'a>(
        &'a self,
        repo: &'a RepoRef,
        branch: &'a str,
    ) -> BoxFut<'a, Result<String, GitError>> {
        Box::pin(async move {
            let url = self.url(repo, &format!("git/ref/heads/{branch}"))?;
            let value = self.call(Method::GET, url, None).await?;
            Self::string_at(&value, "/object/sha", "object.sha")
        })
    }

    fn create_branch<'a>(
        &'a self,
        repo: &'a RepoRef,
        branch: &'a str,
        from_sha: &'a str,
    ) -> BoxFut<'a, Result<(), GitError>> {
        Box::pin(async move {
            let body = json!({ "ref": format!("refs/heads/{branch}"), "sha": from_sha });
            self.call(Method::POST, self.url(repo, "git/refs")?, Some(body))
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
            let parent = self.branch_head(repo, branch).await?;
            let commit = self
                .call(
                    Method::GET,
                    self.url(repo, &format!("git/commits/{parent}"))?,
                    None,
                )
                .await?;
            let base_tree = Self::string_at(&commit, "/tree/sha", "tree.sha")?;

            let mut entries = Vec::with_capacity(changes.len());
            for change in changes {
                let mut entry = json!({
                    "path": change.path,
                    "mode": "100644",
                    "type": "blob",
                });
                match &change.content {
                    // A null SHA is how the tree API spells a deletion.
                    None => entry["sha"] = Value::Null,
                    Some(bytes) => entry["sha"] = Value::String(self.blob(repo, bytes).await?),
                }
                entries.push(entry);
            }

            let tree = self
                .call(
                    Method::POST,
                    self.url(repo, "git/trees")?,
                    Some(json!({ "base_tree": base_tree, "tree": entries })),
                )
                .await?;
            let tree_sha = Self::string_at(&tree, "/sha", "sha")?;

            let created = self
                .call(
                    Method::POST,
                    self.url(repo, "git/commits")?,
                    Some(json!({
                        "message": message,
                        "tree": tree_sha,
                        "parents": [parent],
                        // GIT-04: the person who made the change, not the app.
                        "author": { "name": author.name, "email": author.email },
                    })),
                )
                .await?;
            let sha = Self::string_at(&created, "/sha", "sha")?;

            self.call(
                Method::PATCH,
                self.url(repo, &format!("git/refs/heads/{branch}"))?,
                Some(json!({ "sha": sha, "force": false })),
            )
            .await?;
            Ok(sha)
        })
    }

    fn open_pull_request<'a>(
        &'a self,
        repo: &'a RepoRef,
        request: &'a NewPullRequest,
    ) -> BoxFut<'a, Result<PullRequestRef, GitError>> {
        Box::pin(async move {
            let body = json!({
                "title": request.title,
                "body": request.body,
                "head": request.head,
                "base": request.base,
                "draft": request.draft,
            });
            let value = self
                .call(Method::POST, self.url(repo, "pulls")?, Some(body))
                .await?;
            Ok(PullRequestRef {
                number: value
                    .get("number")
                    .and_then(Value::as_u64)
                    .ok_or(GitError::Malformed {
                        provider: NAME,
                        field: "number",
                    })?,
                url: Self::string_at(&value, "/html_url", "html_url")?,
                head_sha: Self::string_at(&value, "/head/sha", "head.sha")?,
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
            // A pull request is an issue for commenting; the pulls endpoint is
            // for review comments, which are anchored to a diff line.
            let url = self.url(repo, &format!("issues/{number}/comments"))?;
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
            let (first, _rest) = run.split_annotations();
            let annotations: Vec<Value> = first
                .iter()
                .map(|annotation| {
                    json!({
                        "path": annotation.path,
                        "start_line": annotation.start_line,
                        "end_line": annotation.end_line,
                        "annotation_level": annotation.level,
                        "message": annotation.message,
                        "title": annotation.title,
                    })
                })
                .collect();
            let mut body = json!({
                "name": run.name,
                "head_sha": run.head_sha,
                "status": run.status.as_str(),
                "output": {
                    "title": run.title,
                    "summary": run.summary,
                    "annotations": annotations,
                },
            });
            if let Some(conclusion) = run.conclusion {
                body["conclusion"] = Value::String(conclusion.as_str().to_owned());
            }
            if let Some(url) = &run.details_url {
                body["details_url"] = Value::String(url.clone());
            }
            let value = self
                .call(Method::POST, self.url(repo, "check-runs")?, Some(body))
                .await?;
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
                url: Self::string_at(&value, "/html_url", "html_url")?,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::provider::{Annotation, CheckConclusion, StaticToken};
    use crate::recorder::{Exchange, Recorded};

    fn client(exchanges: Vec<Exchange>) -> (GitHub, Arc<Recorded>) {
        let recorded = Arc::new(Recorded::new(exchanges));
        let github = GitHub::new(
            Endpoint::github_com(),
            recorded.clone(),
            Arc::new(StaticToken::new("ghs_test")),
        );
        (github, recorded)
    }

    fn repo() -> RepoRef {
        RepoRef::new("kasecrab", "liyasa")
    }

    #[tokio::test]
    async fn a_check_run_carries_its_conclusion_annotations_and_details_url() {
        let (github, recorded) = client(vec![Exchange::post(
            "/repos/kasecrab/liyasa/check-runs",
            201,
            r#"{"id":99,"html_url":"https://github.com/kasecrab/liyasa/runs/99"}"#,
        )]);
        let run = CheckRun::completed("liyasa", "abc123", CheckConclusion::Failure)
            .with_output("2 errors", "E0401 broken link")
            .with_details_url("https://liyasa.example/builds/1")
            .with_annotations(vec![Annotation {
                path: "docs/index.md".to_owned(),
                start_line: 4,
                end_line: 4,
                level: "failure".to_owned(),
                message: "link target does not exist".to_owned(),
                title: Some("E0401".to_owned()),
            }]);
        let reference = github
            .report_check(&repo(), &run)
            .await
            .expect("the check run posts");
        assert_eq!(reference.id, "99");

        let made = recorded.made();
        assert_eq!(made.len(), 1);
        assert_eq!(
            made[0].url,
            "https://api.github.com/repos/kasecrab/liyasa/check-runs"
        );
        assert_eq!(made[0].header("authorization"), Some("Bearer ghs_test"));
        assert_eq!(made[0].header("x-github-api-version"), Some(API_VERSION));
        let body = made[0].json();
        assert_eq!(body["head_sha"], "abc123");
        assert_eq!(body["status"], "completed");
        assert_eq!(body["conclusion"], "failure");
        assert_eq!(body["details_url"], "https://liyasa.example/builds/1");
        assert_eq!(
            body["output"]["annotations"][0]["annotation_level"],
            "failure"
        );
        assert_eq!(body["output"]["annotations"][0]["start_line"], 4);
        assert_eq!(recorded.unused(), 0);
    }

    #[tokio::test]
    async fn a_queued_check_run_sends_no_conclusion_at_all() {
        let (github, recorded) = client(vec![Exchange::post(
            "/repos/kasecrab/liyasa/check-runs",
            201,
            r#"{"id":1,"html_url":"https://github.com/x"}"#,
        )]);
        github
            .report_check(&repo(), &CheckRun::queued("liyasa", "abc"))
            .await
            .expect("the check run posts");
        let body = recorded.made()[0].json();
        assert_eq!(body["status"], "queued");
        assert!(
            body.get("conclusion").is_none(),
            "a queued run with a conclusion is rejected by the API"
        );
    }

    #[tokio::test]
    async fn a_proposal_of_three_files_is_one_commit() {
        let (github, recorded) = client(vec![
            Exchange::get(
                "/repos/kasecrab/liyasa/git/ref/heads/proposal-1",
                200,
                r#"{"object":{"sha":"parent-sha"}}"#,
            ),
            Exchange::get(
                "/repos/kasecrab/liyasa/git/commits/parent-sha",
                200,
                r#"{"tree":{"sha":"base-tree"}}"#,
            ),
            Exchange::post(
                "/repos/kasecrab/liyasa/git/blobs",
                201,
                r#"{"sha":"blob-a"}"#,
            ),
            Exchange::post(
                "/repos/kasecrab/liyasa/git/blobs",
                201,
                r#"{"sha":"blob-b"}"#,
            ),
            Exchange::post(
                "/repos/kasecrab/liyasa/git/trees",
                201,
                r#"{"sha":"new-tree"}"#,
            ),
            Exchange::post(
                "/repos/kasecrab/liyasa/git/commits",
                201,
                r#"{"sha":"new-commit"}"#,
            ),
            Exchange::patch(
                "/repos/kasecrab/liyasa/git/refs/heads/proposal-1",
                200,
                r#"{"object":{"sha":"new-commit"}}"#,
            ),
        ]);
        let author = Author {
            name: "A Writer".to_owned(),
            email: "writer@example.com".to_owned(),
        };
        let changes = [
            FileChange {
                path: "docs/a.md".to_owned(),
                content: Some(b"# A".to_vec()),
            },
            FileChange {
                path: "docs/b.md".to_owned(),
                content: Some(b"# B".to_vec()),
            },
            FileChange {
                path: "docs/gone.md".to_owned(),
                content: None,
            },
        ];
        let sha = github
            .commit(&repo(), "proposal-1", "docs: rewrite", &author, &changes)
            .await
            .expect("the commit lands");
        assert_eq!(sha, "new-commit");
        assert_eq!(recorded.unused(), 0);

        let made = recorded.made();
        let tree = made
            .iter()
            .find(|call| call.url.ends_with("/git/trees"))
            .expect("a tree call")
            .json();
        assert_eq!(tree["base_tree"], "base-tree");
        assert_eq!(tree["tree"][0]["sha"], "blob-a");
        assert_eq!(tree["tree"][1]["sha"], "blob-b");
        assert_eq!(
            tree["tree"][2]["sha"],
            Value::Null,
            "a deletion is a null sha, not a missing entry"
        );

        let commit = made
            .iter()
            .find(|call| call.url.ends_with("/git/commits"))
            .expect("a commit call")
            .json();
        assert_eq!(commit["parents"][0], "parent-sha");
        assert_eq!(commit["author"]["email"], "writer@example.com");
    }

    #[tokio::test]
    async fn a_binary_file_survives_as_base64() {
        let (github, recorded) = client(vec![
            Exchange::get(
                "/repos/kasecrab/liyasa/git/ref/heads/main",
                200,
                r#"{"object":{"sha":"p"}}"#,
            ),
            Exchange::get(
                "/repos/kasecrab/liyasa/git/commits/p",
                200,
                r#"{"tree":{"sha":"t"}}"#,
            ),
            Exchange::post("/repos/kasecrab/liyasa/git/blobs", 201, r#"{"sha":"b"}"#),
            Exchange::post("/repos/kasecrab/liyasa/git/trees", 201, r#"{"sha":"nt"}"#),
            Exchange::post("/repos/kasecrab/liyasa/git/commits", 201, r#"{"sha":"nc"}"#),
            Exchange::patch("/repos/kasecrab/liyasa/git/refs/heads/main", 200, r#"{}"#),
        ]);
        let bytes = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        github
            .commit(
                &repo(),
                "main",
                "docs: add a logo",
                &Author {
                    name: "A".to_owned(),
                    email: "a@example.com".to_owned(),
                },
                &[FileChange {
                    path: "docs/logo.png".to_owned(),
                    content: Some(bytes.clone()),
                }],
            )
            .await
            .expect("the commit lands");
        let blob = recorded
            .made()
            .into_iter()
            .find(|call| call.url.ends_with("/git/blobs"))
            .expect("a blob call")
            .json();
        assert_eq!(blob["encoding"], "base64");
        let sent = base64::engine::general_purpose::STANDARD
            .decode(blob["content"].as_str().expect("the content"))
            .expect("valid base64");
        assert_eq!(sent, bytes, "the bytes round-trip unchanged");
    }

    #[tokio::test]
    async fn a_pull_request_reports_its_number_url_and_head() {
        let (github, recorded) = client(vec![Exchange::post(
            "/repos/kasecrab/liyasa/pulls",
            201,
            r#"{"number":42,"html_url":"https://github.com/kasecrab/liyasa/pull/42","head":{"sha":"head-sha"}}"#,
        )]);
        let reference = github
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
            .expect("the pull request opens");
        assert_eq!(reference.number, 42);
        assert_eq!(reference.head_sha, "head-sha");
        assert_eq!(recorded.made()[0].json()["draft"], true);
    }

    #[tokio::test]
    async fn a_comment_goes_to_the_issues_endpoint() {
        let (github, recorded) = client(vec![Exchange::post(
            "/repos/kasecrab/liyasa/issues/42/comments",
            201,
            r#"{"id":1}"#,
        )]);
        github
            .comment(&repo(), 42, "Preview: https://liyasa.example/pr-42")
            .await
            .expect("the comment posts");
        assert_eq!(
            recorded.made()[0].json()["body"],
            "Preview: https://liyasa.example/pr-42"
        );
    }

    #[tokio::test]
    async fn a_permission_error_names_what_github_said() {
        let (github, _) = client(vec![Exchange::post(
            "/repos/kasecrab/liyasa/check-runs",
            403,
            r#"{"message":"Resource not accessible by integration"}"#,
        )]);
        let error = github
            .report_check(&repo(), &CheckRun::queued("liyasa", "abc"))
            .await
            .expect_err("a refusal");
        assert!(
            error
                .to_string()
                .contains("Resource not accessible by integration"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn enterprise_server_calls_the_configured_host() {
        let recorded = Arc::new(Recorded::new(vec![Exchange::get(
            "/api/v3/repos/kasecrab/liyasa/git/ref/heads/main",
            200,
            r#"{"object":{"sha":"deadbeef"}}"#,
        )]));
        let github = GitHub::new(
            Endpoint::github_enterprise("https://git.example.com").expect("a base"),
            recorded.clone(),
            Arc::new(StaticToken::new("ghs_test")),
        );
        let head = github
            .branch_head(&repo(), "main")
            .await
            .expect("the ref resolves");
        assert_eq!(head, "deadbeef");
        assert!(
            recorded.made()[0]
                .url
                .starts_with("https://git.example.com/api/v3/"),
            "{}",
            recorded.made()[0].url
        );
    }

    #[tokio::test]
    async fn a_response_missing_the_field_we_need_is_an_error_rather_than_an_empty_string() {
        let (github, _) = client(vec![Exchange::get(
            "/repos/kasecrab/liyasa/git/ref/heads/main",
            200,
            r#"{"object":{}}"#,
        )]);
        let error = github
            .branch_head(&repo(), "main")
            .await
            .expect_err("a refusal");
        assert_eq!(
            error,
            GitError::Malformed {
                provider: "github",
                field: "object.sha"
            }
        );
    }
}
