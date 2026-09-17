//! How a repository, a branch and a subdirectory are named (GIT-10).

use std::fmt;

use serde::{Deserialize, Serialize};

/// `owner/name` on every provider this crate speaks to. GitLab nests groups,
/// so `owner` is everything before the last slash.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RepoRef {
    pub owner: String,
    pub name: String,
}

impl RepoRef {
    pub fn new(owner: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            name: name.into(),
        }
    }

    /// Parses `owner/name` or `group/subgroup/name`. Rejects an empty half and
    /// anything with no slash, so a mistyped config never silently becomes a
    /// repository nobody owns.
    pub fn parse(full_name: &str) -> Option<Self> {
        let trimmed = full_name.trim().trim_matches('/');
        let (owner, name) = trimmed.rsplit_once('/')?;
        if owner.is_empty() || name.is_empty() {
            return None;
        }
        Some(Self::new(owner, name))
    }

    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

impl fmt::Display for RepoRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// `refs/heads/topic` down to `topic`. A ref that is not a branch — a tag, a
/// pull-request head — returns `None`, because a build is only ever triggered
/// by a branch and a tag pushed to the deploy branch's repository must not
/// look like one.
pub fn branch_of_ref(git_ref: &str) -> Option<&str> {
    let name = git_ref.strip_prefix("refs/heads/")?;
    (!name.is_empty()).then_some(name)
}

/// A subdirectory a project lives in (`root: "docs"`), normalised to no
/// leading or trailing slash. An empty or `.` root means the repository root.
pub fn normalize_root(root: &str) -> String {
    let trimmed = root.trim().trim_matches('/');
    match trimmed {
        "" | "." => String::new(),
        other => other.to_owned(),
    }
}

/// Whether `path` is inside `root` (GIT-10). An empty root contains
/// everything, which is what a repository with no `root` key means.
pub fn is_under_root(root: &str, path: &str) -> bool {
    let root = normalize_root(root);
    if root.is_empty() {
        return true;
    }
    let path = path.trim_start_matches('/');
    path.strip_prefix(&root)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// `path` relative to `root`, or `None` when it is outside.
pub fn strip_root<'a>(root: &str, path: &'a str) -> Option<&'a str> {
    let root = normalize_root(root);
    let path = path.trim_start_matches('/');
    if root.is_empty() {
        return Some(path);
    }
    path.strip_prefix(&root)?
        .strip_prefix('/')
        .filter(|rest| !rest.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_name_splits_on_the_last_slash_so_a_gitlab_subgroup_survives() {
        assert_eq!(
            RepoRef::parse("kasecrab/liyasa"),
            Some(RepoRef::new("kasecrab", "liyasa"))
        );
        assert_eq!(
            RepoRef::parse("team/platform/docs"),
            Some(RepoRef::new("team/platform", "docs"))
        );
        assert_eq!(RepoRef::parse("liyasa"), None);
        assert_eq!(RepoRef::parse("kasecrab/"), None);
        assert_eq!(RepoRef::parse("/liyasa"), None);
        assert_eq!(RepoRef::parse(""), None);
    }

    #[test]
    fn a_full_name_round_trips() {
        let repo = RepoRef::parse("team/platform/docs").expect("a nested name");
        assert_eq!(repo.full_name(), "team/platform/docs");
        assert_eq!(repo.to_string(), "team/platform/docs");
        assert_eq!(RepoRef::parse(&repo.full_name()), Some(repo));
    }

    #[test]
    fn only_a_branch_ref_names_a_branch() {
        assert_eq!(branch_of_ref("refs/heads/main"), Some("main"));
        assert_eq!(branch_of_ref("refs/heads/feat/a-b"), Some("feat/a-b"));
        assert_eq!(branch_of_ref("refs/tags/v1.0.0"), None);
        assert_eq!(branch_of_ref("refs/pull/7/head"), None);
        assert_eq!(branch_of_ref("refs/heads/"), None);
        assert_eq!(branch_of_ref("main"), None);
    }

    #[test]
    fn a_root_is_normalised_to_a_bare_path() {
        assert_eq!(normalize_root("/docs/"), "docs");
        assert_eq!(normalize_root("docs"), "docs");
        assert_eq!(normalize_root("  docs/site  "), "docs/site");
        assert_eq!(normalize_root("."), "");
        assert_eq!(normalize_root(""), "");
    }

    #[test]
    fn a_sibling_directory_is_not_inside_the_root() {
        assert!(is_under_root("docs", "docs/index.md"));
        assert!(is_under_root("docs", "/docs/guides/a.md"));
        // The prefix matches as a string and the path is not inside it.
        assert!(!is_under_root("docs", "docs-site/index.md"));
        assert!(!is_under_root("docs", "docs"));
        assert!(!is_under_root("docs", "src/main.rs"));
        assert!(is_under_root("", "anything/at/all.md"));
    }

    #[test]
    fn stripping_the_root_gives_the_project_relative_path() {
        assert_eq!(strip_root("docs", "docs/guides/a.md"), Some("guides/a.md"));
        assert_eq!(strip_root("", "guides/a.md"), Some("guides/a.md"));
        assert_eq!(strip_root("docs", "src/main.rs"), None);
        assert_eq!(strip_root("docs", "docs/"), None);
    }
}
