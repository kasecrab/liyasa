//! Multi-repository sites (GIT-10, `plan/rfcs/1607-mounts-has-no-config-key.md`).
//!
//! A site can be one repository, or a primary repository with others mounted
//! at path prefixes. Two questions follow from that and nothing else does:
//! which repository owns a given route, and which mounts a push to a given
//! repository has to rebuild.
//!
//! Longest matching prefix wins, so a mount at `/api/reference` takes its
//! routes back from a mount at `/api`. That is the rule every router in the
//! world uses, and getting it wrong would route a page to the repository that
//! happens to sort first.

use serde::{Deserialize, Serialize};

use crate::repo::{RepoRef, is_under_root, normalize_root};

/// One repository at one path prefix.
// TODO(rfc-1607): built from values until `mounts` is a key in the schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mount {
    /// `/api`, or the empty string for the primary. No trailing slash.
    pub prefix: String,
    pub repo: RepoRef,
    /// The subdirectory within that repository, normalised.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub root: String,
    /// The branch this mount tracks. `None` means the repository's own deploy
    /// branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
}

impl Mount {
    pub fn primary(repo: RepoRef) -> Self {
        Self {
            prefix: String::new(),
            repo,
            root: String::new(),
            r#ref: None,
        }
    }

    pub fn at(prefix: &str, repo: RepoRef) -> Self {
        Self {
            prefix: normalize_prefix(prefix),
            repo,
            root: String::new(),
            r#ref: None,
        }
    }

    pub fn with_root(mut self, root: &str) -> Self {
        self.root = normalize_root(root);
        self
    }

    pub fn with_ref(mut self, reference: impl Into<String>) -> Self {
        self.r#ref = Some(reference.into());
        self
    }

    /// Whether `route` falls under this mount's prefix.
    pub fn covers(&self, route: &str) -> bool {
        let route = normalize_prefix(route);
        if self.prefix.is_empty() {
            return true;
        }
        route == self.prefix || route.starts_with(&format!("{}/", self.prefix))
    }

    /// `route` with this mount's prefix removed, which is what the mounted
    /// repository's own build calls it.
    pub fn strip(&self, route: &str) -> Option<String> {
        if !self.covers(route) {
            return None;
        }
        let route = normalize_prefix(route);
        let inner = route
            .strip_prefix(&self.prefix)
            .unwrap_or(route.as_str())
            .to_owned();
        Some(match inner.is_empty() {
            true => "/".to_owned(),
            false => inner,
        })
    }

    /// Whether a push that touched these paths changes anything this mount
    /// builds. An empty list means the provider did not say.
    pub fn touched_by(&self, changed: &[String]) -> bool {
        if self.root.is_empty() || changed.is_empty() {
            return true;
        }
        changed.iter().any(|path| is_under_root(&self.root, path))
    }
}

/// `/api`, `/`, or the empty string. One leading slash, no trailing one.
pub fn normalize_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim().trim_matches('/');
    match trimmed.is_empty() {
        true => String::new(),
        false => format!("/{trimmed}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MountError {
    #[error("`{0}` is already mounted")]
    PrefixTaken(String),
    #[error("a mount at the site root would replace the primary repository")]
    WouldReplacePrimary,
}

/// A site: one primary repository and any number of mounted ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Site {
    primary: Mount,
    mounts: Vec<Mount>,
}

impl Site {
    pub fn new(primary: Mount) -> Self {
        Self {
            primary: Mount {
                prefix: String::new(),
                ..primary
            },
            mounts: Vec::new(),
        }
    }

    pub fn with_mount(mut self, mount: Mount) -> Result<Self, MountError> {
        if mount.prefix.is_empty() {
            return Err(MountError::WouldReplacePrimary);
        }
        if self.mounts.iter().any(|other| other.prefix == mount.prefix) {
            return Err(MountError::PrefixTaken(mount.prefix));
        }
        self.mounts.push(mount);
        Ok(self)
    }

    pub fn primary(&self) -> &Mount {
        &self.primary
    }

    pub fn mounts(&self) -> &[Mount] {
        &self.mounts
    }

    /// Every mount, primary last, so a caller that iterates sees the specific
    /// ones first.
    pub fn all(&self) -> impl Iterator<Item = &Mount> {
        self.mounts.iter().chain(std::iter::once(&self.primary))
    }

    /// Which repository owns `route`: the mount with the longest matching
    /// prefix, or the primary.
    pub fn owner_of(&self, route: &str) -> &Mount {
        self.mounts
            .iter()
            .filter(|mount| mount.covers(route))
            .max_by_key(|mount| mount.prefix.len())
            .unwrap_or(&self.primary)
    }

    /// The mounts a push to `repo` has to rebuild. A repository mounted twice
    /// — the same API repo at `/api` and at `/v2/api` — returns both.
    ///
    /// A `Vec` rather than an iterator: the list is at most one per mount, and
    /// an iterator would tie its lifetime to the caller's `repo` for no gain.
    pub fn for_repo(&self, repo: &RepoRef) -> Vec<&Mount> {
        self.all().filter(|mount| &mount.repo == repo).collect()
    }

    /// The mounts a push to `repo` that touched `changed` has to rebuild.
    pub fn touched(&self, repo: &RepoRef, changed: &[String]) -> Vec<&Mount> {
        self.for_repo(repo)
            .into_iter()
            .filter(|mount| mount.touched_by(changed))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docs() -> RepoRef {
        RepoRef::new("kasecrab", "docs")
    }

    fn api() -> RepoRef {
        RepoRef::new("kasecrab", "api")
    }

    fn site() -> Site {
        Site::new(Mount::primary(docs()))
            .with_mount(Mount::at("/api", api()).with_root("reference"))
            .expect("a free prefix")
    }

    #[test]
    fn a_prefix_is_normalised_to_one_leading_slash() {
        assert_eq!(normalize_prefix("api"), "/api");
        assert_eq!(normalize_prefix("/api/"), "/api");
        assert_eq!(normalize_prefix("  /a/b/  "), "/a/b");
        assert_eq!(normalize_prefix("/"), "");
        assert_eq!(normalize_prefix(""), "");
    }

    #[test]
    fn a_route_goes_to_the_repository_that_claims_its_prefix() {
        let site = site();
        assert_eq!(site.owner_of("/api/users").repo, api());
        assert_eq!(site.owner_of("/api").repo, api());
        assert_eq!(site.owner_of("/guides/install").repo, docs());
        assert_eq!(site.owner_of("/").repo, docs());
    }

    #[test]
    fn a_prefix_that_only_looks_like_one_belongs_to_the_primary() {
        let site = site();
        assert_eq!(
            site.owner_of("/apiary/x").repo,
            docs(),
            "`/apiary` is not under `/api`"
        );
    }

    #[test]
    fn the_longest_matching_prefix_wins() {
        let site = site()
            .with_mount(Mount::at(
                "/api/reference",
                RepoRef::new("kasecrab", "spec"),
            ))
            .expect("a free prefix");
        assert_eq!(site.owner_of("/api/reference/users").repo.name, "spec");
        assert_eq!(site.owner_of("/api/guides").repo, api());
    }

    #[test]
    fn a_mount_cannot_take_a_prefix_twice_or_replace_the_primary() {
        let taken = site()
            .with_mount(Mount::at("/api", RepoRef::new("someone", "else")))
            .expect_err("a refusal");
        assert_eq!(taken, MountError::PrefixTaken("/api".to_owned()));

        let root = site()
            .with_mount(Mount::at("/", RepoRef::new("someone", "else")))
            .expect_err("a refusal");
        assert_eq!(root, MountError::WouldReplacePrimary);
    }

    #[test]
    fn a_primary_is_always_at_the_root_whatever_it_was_built_with() {
        let site = Site::new(Mount::at("/somewhere", docs()));
        assert_eq!(site.primary().prefix, "");
        assert_eq!(site.owner_of("/anything").repo, docs());
    }

    #[test]
    fn stripping_a_prefix_gives_the_route_the_mounted_build_knows() {
        let site = site();
        let mount = site.owner_of("/api/users");
        assert_eq!(mount.strip("/api/users"), Some("/users".to_owned()));
        assert_eq!(mount.strip("/api"), Some("/".to_owned()));
        assert_eq!(mount.strip("/guides"), None);
        assert_eq!(
            site.primary().strip("/guides/install"),
            Some("/guides/install".to_owned())
        );
    }

    #[test]
    fn a_push_rebuilds_only_the_mounts_that_repository_feeds() {
        let site = site();
        let for_api = site.for_repo(&api());
        assert_eq!(for_api.len(), 1);
        assert_eq!(for_api[0].prefix, "/api");

        let for_docs = site.for_repo(&docs());
        assert_eq!(for_docs.len(), 1);
        assert_eq!(for_docs[0].prefix, "");

        let stranger = RepoRef::new("someone", "else");
        assert!(site.for_repo(&stranger).is_empty());
    }

    #[test]
    fn one_repository_mounted_twice_rebuilds_both_places() {
        let site = site()
            .with_mount(Mount::at("/v2/api", api()).with_root("reference"))
            .expect("a free prefix");
        let prefixes: Vec<&str> = site
            .for_repo(&api())
            .into_iter()
            .map(|mount| mount.prefix.as_str())
            .collect();
        assert_eq!(prefixes.len(), 2);
        assert!(prefixes.contains(&"/api"));
        assert!(prefixes.contains(&"/v2/api"));
    }

    #[test]
    fn a_push_outside_a_mounts_root_rebuilds_nothing() {
        let site = site();
        let changed = ["src/main.rs".to_owned()];
        assert!(site.touched(&api(), &changed).is_empty());

        let inside = ["reference/users.md".to_owned()];
        assert_eq!(site.touched(&api(), &inside).len(), 1);
    }

    #[test]
    fn a_provider_that_sent_no_path_list_rebuilds_everything_that_repository_feeds() {
        let site = site();
        assert_eq!(
            site.touched(&api(), &[]).len(),
            1,
            "silence is not evidence that nothing changed"
        );
    }

    #[test]
    fn a_site_round_trips_as_json() {
        let site = site();
        let text = serde_json::to_string(&site).expect("a site serializes");
        let back: Site = serde_json::from_str(&text).expect("it deserializes");
        assert_eq!(back, site);
    }
}
