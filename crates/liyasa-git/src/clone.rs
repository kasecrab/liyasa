//! The clone policy of GIT-11, as a value (`plan/rfcs/1601-clone-policy-without-gix.md`).
//!
//! Every rule GIT-11 states is a decision *about* a clone rather than the
//! clone itself: which depth, which filter, which paths, which cap, refused or
//! allowed. Keeping them here means "a 3 GB repository is refused with a clear
//! message" is a unit test rather than a 3 GB repository.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// `contextRepos[].depth`.
pub const DEFAULT_DEPTH: u32 = 1;
/// `contextRepos[].maxBytes`, two gigabytes.
pub const DEFAULT_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// `contextRepos[].refresh`.
pub const DEFAULT_REFRESH: Duration = Duration::from_secs(24 * 60 * 60);
/// GIT-11: ten context repositories per project by default.
pub const MAX_CONTEXT_REPOS: usize = 10;

/// One entry of `contextRepos[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextRepo {
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    /// The only paths checked out. An empty list is not "everything": a
    /// context repository with no paths has nothing Liyasa is allowed to read,
    /// which is a configuration mistake rather than a licence.
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
    /// Seconds between re-fetches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh: Option<u64>,
}

impl ContextRepo {
    pub fn new(repo: impl Into<String>) -> Self {
        Self {
            repo: repo.into(),
            r#ref: None,
            paths: Vec::new(),
            depth: None,
            max_bytes: None,
            refresh: None,
        }
    }

    pub fn with_paths(mut self, paths: &[&str]) -> Self {
        self.paths = paths.iter().map(|p| (*p).to_owned()).collect();
        self
    }

    pub fn with_depth(mut self, depth: u32) -> Self {
        self.depth = Some(depth);
        self
    }

    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = Some(max_bytes);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CloneRefusal {
    #[error(
        "`{repo}` is {size} bytes, over the {cap}-byte cap for a context repository; \
         raise `contextRepos[].maxBytes` for this entry or narrow `contextRepos[].paths`"
    )]
    TooLarge { repo: String, size: u64, cap: u64 },
    #[error(
        "`{0}` lists no paths, so there is nothing to check out; \
         set `contextRepos[].paths` to the directories Liyasa may read"
    )]
    NoPaths(String),
    #[error("`{path}` in `{repo}` leaves the repository")]
    PathEscapes { repo: String, path: String },
    #[error("{count} context repositories are configured; the limit is {MAX_CONTEXT_REPOS}")]
    TooMany { count: usize },
}

/// What to do, decided before anything is fetched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneSpec {
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    pub depth: u32,
    /// Blob filtering, `blob:none`: the history arrives without file
    /// contents, and a blob is fetched when a path in the allow list is
    /// actually read. Spelled as git's own argument by [`CloneSpec::arguments`].
    pub blob_filter: bool,
    /// Sparse checkout, limited to `contextRepos[].paths`.
    pub sparse_paths: Vec<String>,
    pub max_bytes: u64,
    pub refresh: Duration,
}

impl CloneSpec {
    /// Derives the spec, refusing anything GIT-11 says to refuse.
    ///
    /// `size` is what the host reports for the repository, when it reports
    /// one. An unknown size is not a refusal: the cap is enforced again while
    /// fetching, and refusing every repository whose host is quiet about size
    /// would make the feature unusable on a plain SSH remote.
    pub fn plan(repo: &ContextRepo, size: Option<u64>) -> Result<Self, CloneRefusal> {
        if repo.paths.is_empty() {
            return Err(CloneRefusal::NoPaths(repo.repo.clone()));
        }
        for path in &repo.paths {
            if escapes(path) {
                return Err(CloneRefusal::PathEscapes {
                    repo: repo.repo.clone(),
                    path: path.clone(),
                });
            }
        }
        let cap = repo.max_bytes.unwrap_or(DEFAULT_MAX_BYTES);
        if let Some(size) = size
            && size > cap
        {
            return Err(CloneRefusal::TooLarge {
                repo: repo.repo.clone(),
                size,
                cap,
            });
        }
        let mut sparse_paths: Vec<String> = repo.paths.iter().map(|p| normalize(p)).collect();
        sparse_paths.sort();
        sparse_paths.dedup();
        Ok(Self {
            repo: repo.repo.clone(),
            r#ref: repo.r#ref.clone(),
            depth: repo.depth.unwrap_or(DEFAULT_DEPTH),
            blob_filter: true,
            sparse_paths,
            max_bytes: cap,
            refresh: repo
                .refresh
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_REFRESH),
        })
    }

    /// Whether `path` is inside the sparse checkout. A blob outside it is
    /// never fetched, however it is asked for.
    pub fn allows(&self, path: &str) -> bool {
        let path = normalize(path);
        self.sparse_paths.iter().any(|allowed| {
            path == *allowed || path.starts_with(&format!("{allowed}/")) || allowed.is_empty()
        })
    }

    /// Whether a clone taken at `fetched_at` is due for a re-fetch.
    pub fn due(&self, fetched_at_ms: i64, now_ms: i64) -> bool {
        now_ms.saturating_sub(fetched_at_ms) >= self.refresh.as_millis() as i64
    }

    /// The argument list a git implementation is asked for, in the order the
    /// porcelain takes them. Not executed here; it is what a `Cloner` reads,
    /// and what an operator sees when a clone is explained to them.
    pub fn arguments(&self) -> Vec<String> {
        let mut out = vec![
            format!("--depth={}", self.depth),
            "--filter=blob:none".to_owned(),
            "--sparse".to_owned(),
            "--no-tags".to_owned(),
        ];
        if let Some(reference) = &self.r#ref {
            out.push(format!("--branch={reference}"));
        }
        out
    }
}

/// Checks the whole set: the per-project limit, then each entry.
pub fn plan_all(repos: &[ContextRepo]) -> Result<Vec<CloneSpec>, CloneRefusal> {
    if repos.len() > MAX_CONTEXT_REPOS {
        return Err(CloneRefusal::TooMany { count: repos.len() });
    }
    repos
        .iter()
        .map(|repo| CloneSpec::plan(repo, None))
        .collect()
}

fn normalize(path: &str) -> String {
    path.trim().trim_matches('/').to_owned()
}

/// A path that climbs out of the repository, however it is spelled.
///
/// A leading slash is not one of them: git's own sparse-checkout syntax uses
/// it to anchor at the repository root, so `/docs` is `docs` and `/etc/passwd`
/// is the repository's own `etc/passwd`. Only `..` leaves.
fn escapes(path: &str) -> bool {
    path.split('/').any(|segment| segment == "..")
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CloneError {
    #[error("{0}")]
    Refused(#[from] CloneRefusal),
    #[error("{0}")]
    Failed(String),
}

/// Performs a clone.
///
/// TODO(rfc-1601): `gix` is not in the tree; when it is, it arrives as one
/// implementation of this and nothing that consumes [`CloneSpec`] changes.
/// Clones live on the build-worker volume and never in the serving process
/// (GIT-11), which is why this is a trait the worker holds rather than
/// something the server can call.
pub trait Cloner: Send + Sync {
    fn fetch<'a>(
        &'a self,
        spec: &'a CloneSpec,
        into: &'a std::path::Path,
    ) -> liyasa_core::net::BoxFut<'a, Result<(), CloneError>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> ContextRepo {
        ContextRepo::new("kasecrab/api").with_paths(&["src/handlers", "openapi.yaml"])
    }

    #[test]
    fn the_defaults_are_the_ones_the_requirement_states() {
        let spec = CloneSpec::plan(&repo(), None).expect("a plan");
        assert_eq!(spec.depth, 1);
        assert!(spec.blob_filter);
        assert_eq!(spec.max_bytes, 2 * 1024 * 1024 * 1024);
        assert_eq!(spec.refresh, Duration::from_secs(24 * 60 * 60));
    }

    #[test]
    fn a_repository_over_its_cap_is_refused_with_a_message_that_says_what_to_do() {
        let refusal =
            CloneSpec::plan(&repo().with_max_bytes(1_000), Some(3_000)).expect_err("a refusal");
        let message = refusal.to_string();
        assert!(message.contains("3000 bytes"), "{message}");
        assert!(message.contains("over the 1000-byte cap"), "{message}");
        assert!(message.contains("maxBytes"), "{message}");
        assert!(message.contains("paths"), "{message}");
    }

    #[test]
    fn a_repository_at_exactly_its_cap_is_allowed() {
        assert!(CloneSpec::plan(&repo().with_max_bytes(1_000), Some(1_000)).is_ok());
    }

    #[test]
    fn a_host_that_does_not_report_a_size_is_not_a_refusal() {
        assert!(CloneSpec::plan(&repo().with_max_bytes(1), None).is_ok());
    }

    #[test]
    fn a_context_repository_with_no_paths_is_a_configuration_mistake() {
        let refusal =
            CloneSpec::plan(&ContextRepo::new("kasecrab/api"), None).expect_err("a refusal");
        assert_eq!(refusal, CloneRefusal::NoPaths("kasecrab/api".to_owned()));
        assert!(refusal.to_string().contains("paths"), "{refusal}");
    }

    #[test]
    fn a_path_that_climbs_out_of_the_repository_is_refused() {
        for hostile in ["../secrets", "src/../../etc", "docs/.."] {
            let refusal = CloneSpec::plan(&ContextRepo::new("r").with_paths(&[hostile]), None)
                .expect_err("a refusal");
            assert!(
                matches!(refusal, CloneRefusal::PathEscapes { .. }),
                "{hostile} was allowed"
            );
        }
    }

    #[test]
    fn a_leading_slash_anchors_at_the_root_rather_than_escaping() {
        // git's own sparse-checkout syntax; `/etc/passwd` is the repository's
        // own file of that name, not the machine's.
        let spec = CloneSpec::plan(&ContextRepo::new("r").with_paths(&["/etc/passwd"]), None)
            .expect("a plan");
        assert_eq!(spec.sparse_paths, ["etc/passwd"]);
    }

    #[test]
    fn the_sparse_checkout_is_exactly_the_configured_paths() {
        let spec = CloneSpec::plan(&repo(), None).expect("a plan");
        assert_eq!(spec.sparse_paths, ["openapi.yaml", "src/handlers"]);
        assert!(spec.allows("src/handlers/users.rs"));
        assert!(spec.allows("openapi.yaml"));
        assert!(!spec.allows("src/secrets.rs"));
        assert!(
            !spec.allows("src/handlers-other/x.rs"),
            "a prefix match is not a path match"
        );
    }

    #[test]
    fn a_duplicate_or_slashed_path_is_normalised_once() {
        let spec = CloneSpec::plan(
            &ContextRepo::new("r").with_paths(&["/docs/", "docs", "docs"]),
            None,
        )
        .expect("a plan");
        assert_eq!(spec.sparse_paths, ["docs"]);
    }

    #[test]
    fn a_clone_is_re_fetched_on_its_own_schedule() {
        let spec = CloneSpec::plan(&repo(), None).expect("a plan");
        let day = 24 * 60 * 60 * 1000;
        assert!(!spec.due(0, day - 1));
        assert!(spec.due(0, day));

        let hourly = ContextRepo {
            refresh: Some(3_600),
            ..repo()
        };
        let spec = CloneSpec::plan(&hourly, None).expect("a plan");
        assert!(spec.due(0, 3_600_000));
        assert!(!spec.due(0, 3_599_999));
    }

    #[test]
    fn the_arguments_are_shallow_blob_filtered_and_sparse() {
        let spec = CloneSpec::plan(
            &ContextRepo {
                r#ref: Some("main".to_owned()),
                ..repo()
            },
            None,
        )
        .expect("a plan");
        let arguments = spec.arguments();
        assert!(arguments.contains(&"--depth=1".to_owned()), "{arguments:?}");
        assert!(
            arguments.contains(&"--filter=blob:none".to_owned()),
            "{arguments:?}"
        );
        assert!(arguments.contains(&"--sparse".to_owned()), "{arguments:?}");
        assert!(
            arguments.contains(&"--branch=main".to_owned()),
            "{arguments:?}"
        );
    }

    #[test]
    fn a_configured_depth_overrides_the_default() {
        let spec = CloneSpec::plan(&repo().with_depth(50), None).expect("a plan");
        assert_eq!(spec.depth, 50);
        assert!(spec.arguments().contains(&"--depth=50".to_owned()));
    }

    #[test]
    fn more_than_ten_context_repositories_is_refused_as_a_set() {
        let repos: Vec<ContextRepo> = (0..=MAX_CONTEXT_REPOS)
            .map(|n| ContextRepo::new(format!("o/r{n}")).with_paths(&["docs"]))
            .collect();
        assert_eq!(
            plan_all(&repos),
            Err(CloneRefusal::TooMany {
                count: MAX_CONTEXT_REPOS + 1
            })
        );
        assert!(plan_all(&repos[..MAX_CONTEXT_REPOS]).is_ok());
    }

    #[test]
    fn a_context_repository_round_trips_as_json() {
        let repo = repo().with_depth(3).with_max_bytes(1_024);
        let text = serde_json::to_string(&repo).expect("it serializes");
        assert!(text.contains("maxBytes"), "{text}");
        let back: ContextRepo = serde_json::from_str(&text).expect("it deserializes");
        assert_eq!(back, repo);
    }
}
