//! Bitbucket Cloud (GIT-02).
//!
//! One endpoint here is not JSON: `src`, which commits files, takes a form
//! where each field *name* is a file path and its value is that file's
//! contents. It is the only way Bitbucket accepts a multi-file commit, so the
//! form encoder below exists for that one call.

use liyasa_core::net::{BoxFut, HostSet, HttpClient, HttpPolicy, HttpRequest, Method, Url};
use serde_json::{Value, json};

use crate::provider::{
    Author, CheckRun, CheckRunRef, Endpoint, FileChange, GitError, GitProvider, NewPullRequest,
    PullRequestRef, TokenSource, json_request, policy, send,
};
use crate::repo::RepoRef;
use crate::webhook::Provider;

const NAME: &str = "bitbucket";

/// Percent-encodes one form field, `application/x-www-form-urlencoded` style:
/// a space is `+` and everything outside the unreserved set is `%XX`. Raw
/// bytes survive, so a PNG in a proposal is not corrupted.
pub fn encode_form_component(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for byte in bytes {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

pub struct Bitbucket {
    endpoint: Endpoint,
    http: std::sync::Arc<dyn HttpClient>,
    tokens: std::sync::Arc<dyn TokenSource>,
    policy: HttpPolicy,
}

impl std::fmt::Debug for Bitbucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bitbucket")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl Bitbucket {
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
            "repositories/{}/{}/{suffix}",
            repo.owner, repo.name
        ))
    }

    async fn call(&self, method: Method, url: Url, body: Option<Value>) -> Result<Value, GitError> {
        let token = self.tokens.token().await?;
        let request = json_request(method, url, &token, &[], body.as_ref());
        send(self.http.as_ref(), NAME, request, &self.policy).await
    }

    async fn form(&self, url: Url, fields: Vec<(String, Vec<u8>)>) -> Result<Value, GitError> {
        let token = self.tokens.token().await?;
        let body = fields
            .iter()
            .map(|(name, value)| {
                format!(
                    "{}={}",
                    encode_form_component(name.as_bytes()),
                    encode_form_component(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let request = HttpRequest {
            method: Method::POST,
            url,
            headers: vec![
                ("authorization".to_owned(), format!("Bearer {}", *token)),
                ("accept".to_owned(), "application/json".to_owned()),
                (
                    "content-type".to_owned(),
                    "application/x-www-form-urlencoded".to_owned(),
                ),
                (
                    "user-agent".to_owned(),
                    concat!("liyasa/", env!("CARGO_PKG_VERSION")).to_owned(),
                ),
            ],
            body: Some(body.into_bytes().into()),
        };
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

impl GitProvider for Bitbucket {
    fn kind(&self) -> Provider {
        Provider::Bitbucket
    }

    fn branch_head<'a>(
        &'a self,
        repo: &'a RepoRef,
        branch: &'a str,
    ) -> BoxFut<'a, Result<String, GitError>> {
        Box::pin(async move {
            let url = self.url(repo, &format!("refs/branches/{branch}"))?;
            let value = self.call(Method::GET, url, None).await?;
            Self::string_at(&value, "/target/hash", "target.hash")
        })
    }

    fn create_branch<'a>(
        &'a self,
        repo: &'a RepoRef,
        branch: &'a str,
        from_sha: &'a str,
    ) -> BoxFut<'a, Result<(), GitError>> {
        Box::pin(async move {
            let url = self.url(repo, "refs/branches")?;
            self.call(
                Method::POST,
                url,
                Some(json!({ "name": branch, "target": { "hash": from_sha } })),
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
            let parent = self.branch_head(repo, branch).await?;
            let mut fields: Vec<(String, Vec<u8>)> = vec![
                ("message".to_owned(), message.as_bytes().to_vec()),
                ("branch".to_owned(), branch.as_bytes().to_vec()),
                ("parents".to_owned(), parent.as_bytes().to_vec()),
                (
                    "author".to_owned(),
                    format!("{} <{}>", author.name, author.email).into_bytes(),
                ),
            ];
            for change in changes {
                match &change.content {
                    // `files` names a path to remove, and repeats for each.
                    None => fields.push(("files".to_owned(), change.path.as_bytes().to_vec())),
                    Some(bytes) => fields.push((change.path.clone(), bytes.clone())),
                }
            }
            let url = self.url(repo, "src")?;
            self.form(url, fields).await?;
            // The `src` endpoint answers 201 with no body, so the new head is
            // read back rather than parsed out of the response.
            self.branch_head(repo, branch).await
        })
    }

    fn open_pull_request<'a>(
        &'a self,
        repo: &'a RepoRef,
        request: &'a NewPullRequest,
    ) -> BoxFut<'a, Result<PullRequestRef, GitError>> {
        Box::pin(async move {
            let url = self.url(repo, "pullrequests")?;
            let value = self
                .call(
                    Method::POST,
                    url,
                    Some(json!({
                        "title": request.title,
                        "description": request.body,
                        "source": { "branch": { "name": request.head } },
                        "destination": { "branch": { "name": request.base } },
                    })),
                )
                .await?;
            Ok(PullRequestRef {
                number: value
                    .get("id")
                    .and_then(Value::as_u64)
                    .ok_or(GitError::Malformed {
                        provider: NAME,
                        field: "id",
                    })?,
                url: Self::string_at(&value, "/links/html/href", "links.html.href")?,
                head_sha: Self::string_at(&value, "/source/commit/hash", "source.commit.hash")?,
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
            let url = self.url(repo, &format!("pullrequests/{number}/comments"))?;
            self.call(
                Method::POST,
                url,
                Some(json!({ "content": { "raw": body } })),
            )
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
            // Bitbucket has build statuses, with no annotation surface: three
            // states and a description.
            let state = match (run.status, run.conclusion) {
                (crate::provider::CheckStatus::Completed, Some(conclusion)) => {
                    match conclusion.passes() {
                        true => "SUCCESSFUL",
                        false => "FAILED",
                    }
                }
                _ => "INPROGRESS",
            };
            let url = self.url(repo, &format!("commit/{}/statuses/build", run.head_sha))?;
            let value = self
                .call(
                    Method::POST,
                    url,
                    Some(json!({
                        "key": run.name,
                        "state": state,
                        "name": run.name,
                        "url": run.details_url.clone().unwrap_or_default(),
                        "description": match run.title.is_empty() {
                            true => run.summary.clone(),
                            false => run.title.clone(),
                        },
                    })),
                )
                .await?;
            Ok(CheckRunRef {
                id: value
                    .get("key")
                    .and_then(Value::as_str)
                    .unwrap_or(run.name.as_str())
                    .to_owned(),
                url: value
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::provider::{CheckConclusion, StaticToken};
    use crate::recorder::{Exchange, Recorded};

    fn client(exchanges: Vec<Exchange>) -> (Bitbucket, Arc<Recorded>) {
        let recorded = Arc::new(Recorded::new(exchanges));
        let bitbucket = Bitbucket::new(
            Endpoint::bitbucket_org(),
            recorded.clone(),
            Arc::new(StaticToken::new("bb_test")),
        );
        (bitbucket, recorded)
    }

    fn repo() -> RepoRef {
        RepoRef::new("team", "docs")
    }

    #[test]
    fn a_form_component_keeps_every_byte_recoverable() {
        assert_eq!(encode_form_component(b"docs/a b.md"), "docs%2Fa+b.md");
        assert_eq!(
            encode_form_component(&[0x89, 0x50, 0x4e, 0x47]),
            "%89PNG",
            "a PNG's magic bytes survive"
        );
        assert_eq!(
            encode_form_component(b"plain-name_1.0~x"),
            "plain-name_1.0~x"
        );
    }

    #[tokio::test]
    async fn a_commit_posts_each_path_as_its_own_form_field() {
        let (bitbucket, recorded) = client(vec![
            Exchange::get(
                "/2.0/repositories/team/docs/refs/branches/proposal-1",
                200,
                r#"{"target":{"hash":"parent-hash"}}"#,
            ),
            Exchange::post("/2.0/repositories/team/docs/src", 201, ""),
            Exchange::get(
                "/2.0/repositories/team/docs/refs/branches/proposal-1",
                200,
                r#"{"target":{"hash":"new-hash"}}"#,
            ),
        ]);
        let changes = [
            FileChange {
                path: "docs/a.md".to_owned(),
                content: Some(b"# A".to_vec()),
            },
            FileChange {
                path: "docs/gone.md".to_owned(),
                content: None,
            },
        ];
        let sha = bitbucket
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
        assert_eq!(sha, "new-hash");

        let form = recorded
            .made()
            .into_iter()
            .find(|call| call.url.ends_with("/src"))
            .expect("a src call");
        assert_eq!(
            form.header("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        let body = form.body.expect("a form body");
        assert!(body.contains("branch=proposal-1"), "{body}");
        assert!(body.contains("parents=parent-hash"), "{body}");
        assert!(body.contains("docs%2Fa.md=%23+A"), "{body}");
        assert!(
            body.contains("files=docs%2Fgone.md"),
            "a deletion is a `files` field: {body}"
        );
        assert!(
            body.contains("author=A+Writer+%3Cwriter%40example.com%3E"),
            "{body}"
        );
        assert_eq!(recorded.unused(), 0);
    }

    #[tokio::test]
    async fn a_pull_request_reports_its_number_url_and_head() {
        let (bitbucket, _) = client(vec![Exchange::post(
            "/2.0/repositories/team/docs/pullrequests",
            201,
            r#"{"id":9,"links":{"html":{"href":"https://bitbucket.org/team/docs/pull-requests/9"}},"source":{"commit":{"hash":"src-hash"}}}"#,
        )]);
        let reference = bitbucket
            .open_pull_request(
                &repo(),
                &NewPullRequest {
                    title: "docs: fix".to_owned(),
                    body: String::new(),
                    head: "proposal-1".to_owned(),
                    base: "main".to_owned(),
                    draft: false,
                },
            )
            .await
            .expect("the pull request opens");
        assert_eq!(reference.number, 9);
        assert_eq!(reference.head_sha, "src-hash");
    }

    #[tokio::test]
    async fn a_check_becomes_a_build_status_in_bitbuckets_vocabulary() {
        for (run, expected) in [
            (
                CheckRun::completed("liyasa", "abc", CheckConclusion::Success),
                "SUCCESSFUL",
            ),
            (
                CheckRun::completed("liyasa", "abc", CheckConclusion::Failure),
                "FAILED",
            ),
            (CheckRun::queued("liyasa", "abc"), "INPROGRESS"),
        ] {
            let (bitbucket, recorded) = client(vec![Exchange::post(
                "/2.0/repositories/team/docs/commit/abc/statuses/build",
                201,
                r#"{"key":"liyasa","url":"https://liyasa.example/builds/1"}"#,
            )]);
            bitbucket
                .report_check(&repo(), &run)
                .await
                .expect("the status posts");
            assert_eq!(recorded.made()[0].json()["state"], expected);
        }
    }

    #[tokio::test]
    async fn a_comment_is_raw_markdown_under_content() {
        let (bitbucket, recorded) = client(vec![Exchange::post(
            "/2.0/repositories/team/docs/pullrequests/9/comments",
            201,
            r#"{"id":1}"#,
        )]);
        bitbucket
            .comment(&repo(), 9, "Preview: https://liyasa.example/pr-9")
            .await
            .expect("the comment posts");
        assert_eq!(
            recorded.made()[0].json()["content"]["raw"],
            "Preview: https://liyasa.example/pr-9"
        );
    }
}
