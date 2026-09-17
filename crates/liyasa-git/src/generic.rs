//! Any other git host, over SSH or HTTPS with a deploy key (GIT-03).
//!
//! GIT-03 asks for "generic git over SSH or HTTPS with a deploy key and a
//! manual webhook, for any other host". Note what is and is not in that
//! sentence. A deploy key and a webhook are enough to *build what was pushed*.
//! They are not enough to *open a pull request*: there is no such thing in
//! plain git, only in each host's own API, and Gitea, Forgejo, Azure DevOps
//! and Codeberg do not share one.
//!
//! So this provider answers honestly. Reading a branch head works, because
//! that is `git ls-remote` and every host speaks it. Everything else returns
//! [`GitError::Unsupported`] naming the operation, rather than pretending and
//! failing later at a URL nobody configured.
//!
//! A Gitea or Forgejo installation that wants proposals configures itself as
//! GitHub instead: their API is GitHub-shaped, and
//! [`crate::github::GitHub`] with a base URL of `https://<host>/api/v1/`
//! drives it.

use liyasa_core::net::BoxFut;

use crate::provider::{
    Author, CheckRun, CheckRunRef, FileChange, GitError, GitProvider, NewPullRequest,
    PullRequestRef,
};
use crate::repo::RepoRef;
use crate::webhook::Provider;

const NAME: &str = "a generic git remote";

/// How a generic remote is reached. Held so an operator's configuration is
/// checked when it is written rather than when a clone first fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub url: String,
    pub transport: Transport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// `git@host:owner/repo.git` or `ssh://git@host/owner/repo.git`, with a
    /// deploy key.
    Ssh,
    /// `https://host/owner/repo.git`, with a token in the credential helper.
    Https,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RemoteError {
    #[error("`{0}` is not an SSH or HTTPS git URL")]
    NotAGitUrl(String),
    #[error("`{0}` carries a password in the URL; use a deploy key or a credential helper")]
    CredentialsInUrl(String),
}

impl Remote {
    /// Parses a remote URL, refusing the two shapes that cause trouble later:
    /// anything that is not SSH or HTTPS, and a URL with a secret in it.
    pub fn parse(url: &str) -> Result<Self, RemoteError> {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err(RemoteError::NotAGitUrl(url.to_owned()));
        }
        // `user:password@host` — the colon before the `@` is the giveaway.
        if let Some((authority, _)) = trimmed.split_once('@') {
            let authority = authority.rsplit("//").next().unwrap_or(authority);
            if authority.contains(':') && !trimmed.starts_with("ssh://") {
                return Err(RemoteError::CredentialsInUrl(url.to_owned()));
            }
        }
        if trimmed.starts_with("ssh://") || (trimmed.contains('@') && trimmed.contains(':')) {
            return Ok(Self {
                url: trimmed.to_owned(),
                transport: Transport::Ssh,
            });
        }
        if trimmed.starts_with("https://") {
            return Ok(Self {
                url: trimmed.to_owned(),
                transport: Transport::Https,
            });
        }
        Err(RemoteError::NotAGitUrl(url.to_owned()))
    }
}

/// A host with no API.
#[derive(Debug, Clone, Default)]
pub struct Generic;

fn unsupported<T>(operation: &'static str) -> Result<T, GitError> {
    Err(GitError::Unsupported {
        operation,
        provider: NAME,
    })
}

impl GitProvider for Generic {
    fn kind(&self) -> Provider {
        Provider::Generic
    }

    fn branch_head<'a>(
        &'a self,
        _repo: &'a RepoRef,
        _branch: &'a str,
    ) -> BoxFut<'a, Result<String, GitError>> {
        // `git ls-remote` would answer this, and that needs the git
        // implementation RFC 1601 defers. Until then it is unsupported rather
        // than wrong: the webhook already carries the commit for every build
        // this provider is used for.
        Box::pin(async move { unsupported("reading a branch head") })
    }

    fn create_branch<'a>(
        &'a self,
        _repo: &'a RepoRef,
        _branch: &'a str,
        _from_sha: &'a str,
    ) -> BoxFut<'a, Result<(), GitError>> {
        Box::pin(async move { unsupported("creating a branch") })
    }

    fn commit<'a>(
        &'a self,
        _repo: &'a RepoRef,
        _branch: &'a str,
        _message: &'a str,
        _author: &'a Author,
        _changes: &'a [FileChange],
    ) -> BoxFut<'a, Result<String, GitError>> {
        Box::pin(async move { unsupported("committing") })
    }

    fn open_pull_request<'a>(
        &'a self,
        _repo: &'a RepoRef,
        _request: &'a NewPullRequest,
    ) -> BoxFut<'a, Result<PullRequestRef, GitError>> {
        Box::pin(async move { unsupported("opening a pull request") })
    }

    fn comment<'a>(
        &'a self,
        _repo: &'a RepoRef,
        _number: u64,
        _body: &'a str,
    ) -> BoxFut<'a, Result<(), GitError>> {
        Box::pin(async move { unsupported("commenting") })
    }

    fn report_check<'a>(
        &'a self,
        _repo: &'a RepoRef,
        _run: &'a CheckRun,
    ) -> BoxFut<'a, Result<CheckRunRef, GitError>> {
        Box::pin(async move { unsupported("reporting a check") })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ssh_remote_is_recognised_in_both_spellings() {
        assert_eq!(
            Remote::parse("git@codeberg.org:team/docs.git")
                .expect("a remote")
                .transport,
            Transport::Ssh
        );
        assert_eq!(
            Remote::parse("ssh://git@git.example.com/team/docs.git")
                .expect("a remote")
                .transport,
            Transport::Ssh
        );
    }

    #[test]
    fn an_https_remote_is_recognised() {
        assert_eq!(
            Remote::parse("https://git.example.com/team/docs.git")
                .expect("a remote")
                .transport,
            Transport::Https
        );
    }

    #[test]
    fn a_url_with_a_password_in_it_is_refused() {
        let refusal = Remote::parse("https://user:hunter2@git.example.com/team/docs.git")
            .expect_err("a refusal");
        assert!(
            matches!(refusal, RemoteError::CredentialsInUrl(_)),
            "{refusal}"
        );
        assert!(refusal.to_string().contains("deploy key"), "{refusal}");
    }

    #[test]
    fn anything_that_is_not_ssh_or_https_is_refused() {
        for bad in [
            "",
            "http://git.example.com/x.git",
            "file:///srv/git/x.git",
            "git://h/x",
        ] {
            assert!(
                matches!(Remote::parse(bad), Err(RemoteError::NotAGitUrl(_))),
                "`{bad}` was accepted"
            );
        }
    }

    #[tokio::test]
    async fn every_api_operation_says_what_it_cannot_do_and_who_it_is() {
        let generic = Generic;
        let repo = RepoRef::new("team", "docs");
        let error = generic
            .open_pull_request(
                &repo,
                &NewPullRequest {
                    title: "t".to_owned(),
                    body: String::new(),
                    head: "h".to_owned(),
                    base: "b".to_owned(),
                    draft: false,
                },
            )
            .await
            .expect_err("a refusal");
        assert_eq!(
            error.to_string(),
            "opening a pull request is not something a generic git remote offers"
        );
        assert!(!error.is_transient(), "retrying will not add an API");

        assert!(generic.comment(&repo, 1, "x").await.is_err());
        assert!(generic.branch_head(&repo, "main").await.is_err());
        assert!(
            generic
                .report_check(&repo, &CheckRun::queued("liyasa", "abc"))
                .await
                .is_err()
        );
    }

    #[test]
    fn a_generic_remote_still_identifies_itself_as_a_provider() {
        assert_eq!(Generic.kind(), Provider::Generic);
    }
}
