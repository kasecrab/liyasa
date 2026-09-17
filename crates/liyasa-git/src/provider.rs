//! What Liyasa asks a git host to do, and how it is allowed to ask (GIT-01..04).
//!
//! Every call is an [`HttpRequest`] through the one HTTP client, with
//! `Purpose::GitProvider`, so an operator's host allow list and §30.2.3's
//! connect-time address checks apply to provider traffic exactly as they apply
//! to a fact source (`plan/rfcs/1600-provider-http-over-the-net-seam.md`).

use std::time::Duration;

use liyasa_core::net::{
    BoxFut, HostSet, HttpClient, HttpPolicy, HttpRequest, HttpResponse, Method, NetError, Purpose,
    Url,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::repo::RepoRef;
use crate::webhook::Provider;

/// A provider response larger than this is a bug or an attack, not an API.
pub const MAX_RESPONSE: u64 = 4 * 1024 * 1024;
pub const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GitError {
    #[error("{0}")]
    Net(#[from] NetError),
    #[error("{provider} answered {status}: {message}")]
    Status {
        provider: &'static str,
        status: u16,
        message: String,
    },
    #[error("{provider} answered with no `{field}`")]
    Malformed {
        provider: &'static str,
        field: &'static str,
    },
    #[error("{operation} is not something {provider} offers")]
    Unsupported {
        operation: &'static str,
        provider: &'static str,
    },
    #[error("no credential is configured for {0}")]
    NoCredential(&'static str),
    #[error("`{0}` is not a URL")]
    BadUrl(String),
}

impl GitError {
    /// Whether retrying the same call could succeed. A 5xx or a timeout could;
    /// a 404 or a bad signature could not.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Net(NetError::Timeout | NetError::Io(_) | NetError::Dns(_)) => true,
            Self::Net(NetError::Status(status)) => *status >= 500 || *status == 429,
            Self::Status { status, .. } => *status >= 500 || *status == 429,
            _ => false,
        }
    }
}

/// The bearer token for one installation.
///
/// TODO(rfc-1602): the app flow signs a JWT with RS256 and exchanges it here.
/// No RSA implementation is in the tree, so the shipped source is a token the
/// operator supplies.
pub trait TokenSource: Send + Sync + std::fmt::Debug {
    fn token<'a>(&'a self) -> BoxFut<'a, Result<Zeroizing<String>, GitError>>;
}

#[derive(Clone)]
pub struct StaticToken(Zeroizing<String>);

impl StaticToken {
    pub fn new(token: impl Into<String>) -> Self {
        Self(Zeroizing::new(token.into()))
    }
}

/// Never prints the token, on any path, including a panic message.
impl std::fmt::Debug for StaticToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StaticToken").finish_non_exhaustive()
    }
}

impl TokenSource for StaticToken {
    fn token<'a>(&'a self) -> BoxFut<'a, Result<Zeroizing<String>, GitError>> {
        let token = self.0.clone();
        Box::pin(async move { Ok(token) })
    }
}

/// TODO(rfc-1602): the RS256 signature the GitHub App flow needs. Nothing
/// implements it yet; `StaticToken` is the shipped path.
pub trait JwtSigner: Send + Sync {
    fn sign_rs256(&self, claims: &str) -> Result<String, GitError>;
}

/// Where a provider lives. `api_base` is configurable so GitHub Enterprise
/// Server, a self-managed GitLab and a Gitea all work (GIT-01, GIT-02, GIT-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub api_base: Url,
    /// Where a human goes: the pull request page, the commit. Not always the
    /// API host — github.com's API is api.github.com and its pages are not.
    pub web_base: Url,
}

impl Endpoint {
    pub fn parse(api_base: &str, web_base: &str) -> Result<Self, GitError> {
        Ok(Self {
            api_base: api_base
                .parse()
                .map_err(|_| GitError::BadUrl(api_base.to_owned()))?,
            web_base: web_base
                .parse()
                .map_err(|_| GitError::BadUrl(web_base.to_owned()))?,
        })
    }

    pub fn github_com() -> Self {
        Self::parse("https://api.github.com/", "https://github.com/")
            .expect("the github.com endpoints are valid URLs")
    }

    /// GitHub Enterprise Server puts the API under `/api/v3/` on the same host
    /// as the pages, which is the single most common misconfiguration in a
    /// self-hosted setup.
    pub fn github_enterprise(host_base: &str) -> Result<Self, GitError> {
        let base = host_base.trim_end_matches('/');
        Self::parse(&format!("{base}/api/v3/"), &format!("{base}/"))
    }

    pub fn gitlab_com() -> Self {
        Self::parse("https://gitlab.com/api/v4/", "https://gitlab.com/")
            .expect("the gitlab.com endpoints are valid URLs")
    }

    pub fn bitbucket_org() -> Self {
        Self::parse("https://api.bitbucket.org/2.0/", "https://bitbucket.org/")
            .expect("the bitbucket.org endpoints are valid URLs")
    }

    /// `api_base` joined with a relative path. A path is never allowed to
    /// escape the configured base, so a repository name out of a webhook
    /// cannot redirect a call to another host.
    pub fn join(&self, path: &str) -> Result<Url, GitError> {
        // `//host/x` and `scheme://host/x` are both absolute to `Url::join`,
        // so neither is a path under this base however it is spelled.
        if path.starts_with("//") || path.contains("://") {
            return Err(GitError::BadUrl(path.to_owned()));
        }
        let joined = self
            .api_base
            .join(path.trim_start_matches('/'))
            .map_err(|_| GitError::BadUrl(path.to_owned()))?;
        if joined.origin() != self.api_base.origin() {
            return Err(GitError::BadUrl(path.to_owned()));
        }
        Ok(joined)
    }
}

/// The policy every provider call is made under.
pub fn policy(allow_hosts: HostSet) -> HttpPolicy {
    HttpPolicy {
        allow_hosts,
        deny_hosts: HostSet::default(),
        allow_private: false,
        // A provider that redirects an API call is a provider whose API moved;
        // following it would leave the allow list silently.
        max_redirects: 0,
        max_bytes: MAX_RESPONSE,
        timeout: TIMEOUT,
        purpose: Purpose::GitProvider,
    }
}

/// Who a commit is attributed to (GIT-04). An editor change is committed as
/// the person who made it, never as the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Queued,
    InProgress,
    Completed,
}

impl CheckStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckConclusion {
    Success,
    Failure,
    Neutral,
    Cancelled,
    TimedOut,
    ActionRequired,
}

impl CheckConclusion {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Neutral => "neutral",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::ActionRequired => "action_required",
        }
    }

    /// Whether a required check with this conclusion lets a merge through
    /// (GIT-51).
    pub fn passes(self) -> bool {
        matches!(self, Self::Success | Self::Neutral)
    }
}

/// One diagnostic, placed on a line of a file (GIT-50).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Annotation {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    /// `notice`, `warning` or `failure`, which is the vocabulary both GitHub
    /// checks and GitLab code-quality reports use.
    pub level: String,
    pub message: String,
    pub title: Option<String>,
}

/// A provider caps a single check-run update at fifty annotations; more are
/// sent in follow-up updates.
pub const ANNOTATIONS_PER_UPDATE: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRun {
    pub name: String,
    pub head_sha: String,
    pub status: CheckStatus,
    pub conclusion: Option<CheckConclusion>,
    pub title: String,
    pub summary: String,
    pub details_url: Option<String>,
    pub annotations: Vec<Annotation>,
}

impl CheckRun {
    pub fn queued(name: impl Into<String>, head_sha: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            head_sha: head_sha.into(),
            status: CheckStatus::Queued,
            conclusion: None,
            title: String::new(),
            summary: String::new(),
            details_url: None,
            annotations: Vec::new(),
        }
    }

    pub fn completed(
        name: impl Into<String>,
        head_sha: impl Into<String>,
        conclusion: CheckConclusion,
    ) -> Self {
        Self {
            status: CheckStatus::Completed,
            conclusion: Some(conclusion),
            ..Self::queued(name, head_sha)
        }
    }

    pub fn with_output(mut self, title: impl Into<String>, summary: impl Into<String>) -> Self {
        self.title = title.into();
        self.summary = summary.into();
        self
    }

    pub fn with_details_url(mut self, url: impl Into<String>) -> Self {
        self.details_url = Some(url.into());
        self
    }

    pub fn with_annotations(mut self, annotations: Vec<Annotation>) -> Self {
        self.annotations = annotations;
        self
    }

    /// The annotations that fit in one update, and the rest.
    pub fn split_annotations(&self) -> (&[Annotation], &[Annotation]) {
        let take = self.annotations.len().min(ANNOTATIONS_PER_UPDATE);
        self.annotations.split_at(take)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRunRef {
    pub id: String,
    pub url: String,
}

/// One file in a proposal (GIT-04). `content` of `None` deletes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub content: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPullRequest {
    pub title: String,
    pub body: String,
    pub head: String,
    pub base: String,
    pub draft: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestRef {
    pub number: u64,
    pub url: String,
    pub head_sha: String,
}

/// What every host must do for Liyasa to drive it. A host that cannot do one
/// of these answers [`GitError::Unsupported`] rather than pretending.
pub trait GitProvider: Send + Sync {
    fn kind(&self) -> Provider;

    /// The commit a branch points at.
    fn branch_head<'a>(
        &'a self,
        repo: &'a RepoRef,
        branch: &'a str,
    ) -> BoxFut<'a, Result<String, GitError>>;

    /// Creates `branch` at `from_sha`. An existing branch of that name is an
    /// error, not a silent move: a proposal never rewrites someone's work.
    fn create_branch<'a>(
        &'a self,
        repo: &'a RepoRef,
        branch: &'a str,
        from_sha: &'a str,
    ) -> BoxFut<'a, Result<(), GitError>>;

    /// Commits `changes` onto `branch`, attributed to `author` (GIT-04).
    fn commit<'a>(
        &'a self,
        repo: &'a RepoRef,
        branch: &'a str,
        message: &'a str,
        author: &'a Author,
        changes: &'a [FileChange],
    ) -> BoxFut<'a, Result<String, GitError>>;

    fn open_pull_request<'a>(
        &'a self,
        repo: &'a RepoRef,
        request: &'a NewPullRequest,
    ) -> BoxFut<'a, Result<PullRequestRef, GitError>>;

    /// A comment on the pull request's conversation (GIT-30).
    fn comment<'a>(
        &'a self,
        repo: &'a RepoRef,
        number: u64,
        body: &'a str,
    ) -> BoxFut<'a, Result<(), GitError>>;

    /// Reports a build's result against a commit (GIT-01, GIT-51).
    fn report_check<'a>(
        &'a self,
        repo: &'a RepoRef,
        run: &'a CheckRun,
    ) -> BoxFut<'a, Result<CheckRunRef, GitError>>;
}

/// A JSON request with the bearer token and the provider's own headers
/// applied. Shared by every implementation so a header is never set in one
/// place and forgotten in another.
pub(crate) fn json_request(
    method: Method,
    url: Url,
    token: &str,
    extra: &[(&str, &str)],
    body: Option<&serde_json::Value>,
) -> HttpRequest {
    let mut headers = vec![
        ("authorization".to_owned(), format!("Bearer {token}")),
        ("accept".to_owned(), "application/json".to_owned()),
        (
            "user-agent".to_owned(),
            concat!("liyasa/", env!("CARGO_PKG_VERSION")).to_owned(),
        ),
    ];
    headers.extend(
        extra
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned())),
    );
    if body.is_some() {
        headers.push(("content-type".to_owned(), "application/json".to_owned()));
    }
    HttpRequest {
        method,
        url,
        headers,
        body: body.map(|value| value.to_string().into_bytes().into()),
    }
}

/// Turns a response into JSON, or into the error a caller can act on. A
/// provider's own message is carried through: "Resource not accessible by
/// integration" tells an operator which permission to grant, and inventing our
/// own wording would lose that.
pub(crate) fn json_response(
    provider: &'static str,
    response: HttpResponse,
) -> Result<serde_json::Value, GitError> {
    if !(200..300).contains(&response.status) {
        let text = String::from_utf8_lossy(&response.body);
        let message = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|value| {
                value
                    .get("message")
                    .or_else(|| value.get("error"))
                    .and_then(|m| m.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| text.chars().take(200).collect());
        return Err(GitError::Status {
            provider,
            status: response.status,
            message,
        });
    }
    if response.body.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_slice(&response.body).map_err(|_| GitError::Malformed {
        provider,
        field: "a JSON body",
    })
}

pub(crate) async fn send(
    http: &dyn HttpClient,
    provider: &'static str,
    request: HttpRequest,
    policy: &HttpPolicy,
) -> Result<serde_json::Value, GitError> {
    let response = http.fetch(request, policy).await?;
    json_response(provider, response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enterprise_server_puts_the_api_under_api_v3_on_the_same_host() {
        let endpoint =
            Endpoint::github_enterprise("https://git.example.com").expect("a valid base");
        assert_eq!(
            endpoint.api_base.as_str(),
            "https://git.example.com/api/v3/"
        );
        assert_eq!(endpoint.web_base.as_str(), "https://git.example.com/");
        assert_eq!(
            endpoint
                .join("repos/o/r/check-runs")
                .expect("a joined path")
                .as_str(),
            "https://git.example.com/api/v3/repos/o/r/check-runs"
        );
    }

    #[test]
    fn a_trailing_slash_on_the_configured_base_does_not_double() {
        let with = Endpoint::github_enterprise("https://git.example.com/").expect("a valid base");
        let without = Endpoint::github_enterprise("https://git.example.com").expect("a valid base");
        assert_eq!(with, without);
    }

    #[test]
    fn a_path_may_not_leave_the_configured_origin() {
        let endpoint = Endpoint::github_com();
        for hostile in ["https://evil.example.com/repos", "//evil.example.com/repos"] {
            assert!(
                matches!(endpoint.join(hostile), Err(GitError::BadUrl(_))),
                "{hostile} must not resolve"
            );
        }
    }

    #[test]
    fn provider_traffic_is_never_allowed_to_redirect_or_reach_a_private_address() {
        let policy = policy(HostSet::default());
        assert_eq!(policy.purpose, Purpose::GitProvider);
        assert_eq!(policy.max_redirects, 0);
        assert!(!policy.allow_private);
    }

    #[test]
    fn a_token_is_never_printed_by_debug() {
        let token = StaticToken::new("ghs_averysecrettokenvalue");
        let printed = format!("{token:?}");
        assert!(!printed.contains("ghs_"), "{printed}");
    }

    #[tokio::test]
    async fn a_static_token_is_what_the_source_hands_back() {
        let source = StaticToken::new("ghs_1");
        let token = source.token().await.expect("a token");
        assert_eq!(&*token, "ghs_1");
    }

    #[test]
    fn a_providers_own_message_survives_into_the_error() {
        let response = HttpResponse {
            status: 403,
            headers: Vec::new(),
            body: br#"{"message":"Resource not accessible by integration"}"#
                .to_vec()
                .into(),
            final_url: "https://api.github.com/".parse().expect("a url"),
        };
        let error = json_response("github", response).expect_err("a refusal");
        assert_eq!(
            error.to_string(),
            "github answered 403: Resource not accessible by integration"
        );
        assert!(!error.is_transient(), "a permission error is not retried");
    }

    #[test]
    fn a_server_error_and_a_rate_limit_are_worth_retrying_and_a_404_is_not() {
        let at = |status| GitError::Status {
            provider: "github",
            status,
            message: String::new(),
        };
        assert!(at(503).is_transient());
        assert!(at(429).is_transient());
        assert!(!at(404).is_transient());
        assert!(GitError::Net(NetError::Timeout).is_transient());
    }

    #[test]
    fn an_empty_body_is_success_rather_than_malformed() {
        let response = HttpResponse {
            status: 204,
            headers: Vec::new(),
            body: Vec::new().into(),
            final_url: "https://api.github.com/".parse().expect("a url"),
        };
        assert_eq!(
            json_response("github", response).expect("a 204"),
            serde_json::Value::Null
        );
    }

    #[test]
    fn a_request_carries_the_token_and_says_it_is_json_only_when_it_has_a_body() {
        let url: Url = "https://api.github.com/repos/o/r".parse().expect("a url");
        let without = json_request(Method::GET, url.clone(), "t", &[], None);
        assert!(
            !without
                .headers
                .iter()
                .any(|(name, _)| name == "content-type")
        );
        let with = json_request(
            Method::POST,
            url,
            "t",
            &[("x-github-api-version", "2022-11-28")],
            Some(&serde_json::json!({ "a": 1 })),
        );
        assert!(
            with.headers
                .contains(&("authorization".to_owned(), "Bearer t".to_owned()))
        );
        assert!(
            with.headers
                .contains(&("x-github-api-version".to_owned(), "2022-11-28".to_owned()))
        );
        assert_eq!(with.body.as_deref(), Some(&b"{\"a\":1}"[..]));
    }

    #[test]
    fn a_long_annotation_list_is_split_at_the_providers_cap() {
        let annotation = Annotation {
            path: "a.md".to_owned(),
            start_line: 1,
            end_line: 1,
            level: "warning".to_owned(),
            message: "m".to_owned(),
            title: None,
        };
        let run = CheckRun::completed("liyasa", "abc", CheckConclusion::Failure)
            .with_annotations(vec![annotation; ANNOTATIONS_PER_UPDATE + 3]);
        let (first, rest) = run.split_annotations();
        assert_eq!(first.len(), ANNOTATIONS_PER_UPDATE);
        assert_eq!(rest.len(), 3);
    }

    #[test]
    fn only_success_and_neutral_let_a_required_check_merge() {
        assert!(CheckConclusion::Success.passes());
        assert!(CheckConclusion::Neutral.passes());
        for blocked in [
            CheckConclusion::Failure,
            CheckConclusion::Cancelled,
            CheckConclusion::TimedOut,
            CheckConclusion::ActionRequired,
        ] {
            assert!(!blocked.passes(), "{}", blocked.as_str());
        }
    }
}
