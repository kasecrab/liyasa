//! How long a bundle is kept (GIT-23).
//!
//! Two rules, and the order between them is the whole point: a build an
//! environment points at is never removed, however old it is, and everything
//! else past the policy's count goes. A retention sweep that got the order
//! wrong would delete the bundle production is serving.

use std::collections::BTreeSet;
use std::sync::Arc;

use liyasa_core::ids::{BuildId, ProjectId};
use liyasa_core::net::BoxFut;
use liyasa_core::store::{Page, StoreError};
use liyasa_store::SqliteStore;

/// The default of GIT-23: the last fifty production builds.
pub const DEFAULT_PRODUCTION: usize = 50;
/// A preview's bundle is cheap to rebuild and numerous, so fewer are kept.
pub const DEFAULT_PREVIEW: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub production: usize,
    pub preview: usize,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            production: DEFAULT_PRODUCTION,
            preview: DEFAULT_PREVIEW,
        }
    }
}

impl Policy {
    pub fn keep_for(&self, env: &str) -> usize {
        match env {
            super::environment::PRODUCTION => self.production,
            super::environment::PREVIEW => self.preview,
            // A named environment is somewhere people look at, so it is kept
            // like production rather than like a preview.
            _ => self.production,
        }
    }
}

/// Where a bundle's bytes live. Removing a build record without removing its
/// bytes would leak the disk the retention policy exists to bound.
pub trait BundleStore: Send + Sync + std::fmt::Debug {
    fn remove<'a>(&'a self, dist: &'a str) -> BoxFut<'a, Result<(), StoreError>>;
}

/// A local directory: `dist` is a path.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalBundles;

impl BundleStore for LocalBundles {
    fn remove<'a>(&'a self, dist: &'a str) -> BoxFut<'a, Result<(), StoreError>> {
        Box::pin(async move {
            match tokio::fs::remove_dir_all(dist).await {
                Ok(()) => Ok(()),
                // Already gone is the desired state, not a failure.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(StoreError::Sql(error.to_string())),
            }
        })
    }
}

/// One build, reduced to what the policy looks at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub id: BuildId,
    pub dist: String,
    pub created_at: i64,
}

/// The builds that may be removed: everything past `keep`, newest first, that
/// no environment points at.
///
/// `referenced` is checked before the count, not after, so a build an
/// environment still serves never falls off the end of the list.
pub fn expired(
    candidates: &[Candidate],
    referenced: &BTreeSet<BuildId>,
    keep: usize,
) -> Vec<Candidate> {
    let mut newest_first = candidates.to_vec();
    newest_first.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.to_string().cmp(&a.id.to_string()))
    });
    let mut kept = 0usize;
    let mut out = Vec::new();
    for candidate in newest_first {
        if referenced.contains(&candidate.id) {
            continue;
        }
        if kept < keep {
            kept += 1;
            continue;
        }
        out.push(candidate);
    }
    out
}

#[derive(Debug, Clone)]
pub struct Retention {
    store: Arc<SqliteStore>,
    bundles: Arc<dyn BundleStore>,
    policy: Policy,
}

impl Retention {
    pub fn new(store: Arc<SqliteStore>) -> Self {
        Self {
            store,
            bundles: Arc::new(LocalBundles),
            policy: Policy::default(),
        }
    }

    pub fn with_bundles(mut self, bundles: Arc<dyn BundleStore>) -> Self {
        self.bundles = bundles;
        self
    }

    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    pub fn policy(&self) -> Policy {
        self.policy
    }

    /// Every build any environment of this project points at.
    pub async fn referenced(&self, project: &ProjectId) -> Result<BTreeSet<BuildId>, StoreError> {
        Ok(self
            .store
            .deployments_typed()
            .list(Some(project), None)
            .await?
            .into_iter()
            .map(|record| record.build)
            .collect())
    }

    /// Applies the policy to one environment and returns what it removed.
    pub async fn sweep(&self, project: &ProjectId, env: &str) -> Result<Vec<BuildId>, StoreError> {
        let builds = self
            .store
            .builds_typed()
            .list(
                Some(project),
                Some(env),
                None,
                &Page {
                    cursor: None,
                    limit: 500,
                },
            )
            .await?;
        let candidates: Vec<Candidate> = builds
            .iter()
            .map(|build| Candidate {
                id: build.id,
                dist: build.dist.clone(),
                created_at: build.created_at,
            })
            .collect();
        let referenced = self.referenced(project).await?;
        let mut removed = Vec::new();
        for candidate in expired(&candidates, &referenced, self.policy.keep_for(env)) {
            self.bundles.remove(&candidate.dist).await?;
            self.store.builds_typed().delete(&candidate.id).await?;
            removed.push(candidate.id);
        }
        Ok(removed)
    }

    /// Removes one build's bundle, refusing while an environment points at it
    /// (GIT-23).
    pub async fn remove(&self, project: &ProjectId, build: &BuildId) -> Result<bool, StoreError> {
        if self.referenced(project).await?.contains(build) {
            return Ok(false);
        }
        if let Some(record) = self.store.builds_typed().get(build).await? {
            self.bundles.remove(&record.dist).await?;
        }
        self.store.builds_typed().delete(build).await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use liyasa_core::ids::Fingerprint;

    use super::*;

    fn build(seed: &str, created_at: i64) -> Candidate {
        Candidate {
            id: BuildId(Fingerprint::of(seed.as_bytes())),
            dist: format!("/var/lib/liyasa/{seed}"),
            created_at,
        }
    }

    #[test]
    fn everything_past_the_count_is_removed_newest_first() {
        let candidates: Vec<Candidate> = (0..5)
            .map(|n| build(&format!("b{n}"), i64::from(n)))
            .collect();
        let expired = expired(&candidates, &BTreeSet::new(), 2);
        assert_eq!(expired.len(), 3);
        let kept: Vec<&Candidate> = candidates
            .iter()
            .filter(|c| !expired.iter().any(|e| e.id == c.id))
            .collect();
        assert_eq!(
            kept.iter().map(|c| c.created_at).collect::<Vec<_>>(),
            vec![3, 4],
            "the two newest are what remain"
        );
    }

    #[test]
    fn a_build_an_environment_points_at_is_never_removed_however_old() {
        let candidates: Vec<Candidate> = (0..5)
            .map(|n| build(&format!("b{n}"), i64::from(n)))
            .collect();
        let oldest = candidates[0].id;
        let referenced = BTreeSet::from([oldest]);
        let expired = expired(&candidates, &referenced, 2);
        assert!(
            !expired.iter().any(|c| c.id == oldest),
            "production is still serving it"
        );
        assert_eq!(expired.len(), 2, "b1 and b2 go instead");
    }

    #[test]
    fn a_referenced_build_does_not_use_up_one_of_the_kept_slots() {
        let candidates: Vec<Candidate> = (0..4)
            .map(|n| build(&format!("b{n}"), i64::from(n)))
            .collect();
        // The newest is deployed; keeping two should still keep two others.
        let referenced = BTreeSet::from([candidates[3].id]);
        let expired = expired(&candidates, &referenced, 2);
        let removed: Vec<i64> = expired.iter().map(|c| c.created_at).collect();
        assert_eq!(removed, vec![0], "b1 and b2 are the two kept, b0 goes");
    }

    #[test]
    fn a_policy_that_keeps_nothing_still_keeps_what_is_deployed() {
        let candidates = vec![build("a", 1), build("b", 2)];
        let referenced = BTreeSet::from([candidates[1].id]);
        let expired = expired(&candidates, &referenced, 0);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].id, candidates[0].id);
    }

    #[test]
    fn fewer_builds_than_the_policy_keeps_removes_nothing() {
        let candidates = vec![build("a", 1), build("b", 2)];
        assert!(expired(&candidates, &BTreeSet::new(), DEFAULT_PRODUCTION).is_empty());
    }

    #[test]
    fn a_preview_is_kept_less_generously_than_a_named_environment() {
        let policy = Policy::default();
        assert_eq!(policy.keep_for("production"), DEFAULT_PRODUCTION);
        assert_eq!(policy.keep_for("preview"), DEFAULT_PREVIEW);
        assert_eq!(
            policy.keep_for("staging"),
            DEFAULT_PRODUCTION,
            "a named environment is somewhere people look at"
        );
    }

    #[tokio::test]
    async fn removing_a_bundle_that_is_already_gone_is_success() {
        let missing = std::env::temp_dir().join("liyasa-retention-absent-dir");
        let _ = std::fs::remove_dir_all(&missing);
        assert!(
            LocalBundles
                .remove(&missing.to_string_lossy())
                .await
                .is_ok()
        );
    }
}
