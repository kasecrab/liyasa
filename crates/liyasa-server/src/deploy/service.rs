//! What the deploy routes are given (GIT-20, GIT-21, GIT-22).
//!
//! `AppState` belongs to WP-14 and is shared by every handler, so this module
//! holds its own state and carries `AppState` inside it rather than growing
//! it. The deploy router is merged onto the main one; nothing in `routes/`
//! changes.

use std::collections::BTreeMap;
use std::sync::Arc;

use liyasa_core::ids::ProjectId;
use liyasa_git::repo::{RepoRef, normalize_root};
use liyasa_git::webhook::{Provider, Verifier};

use super::environment::{Environment, PREVIEW};
use super::queue::DeployQueue;
use super::retention::Retention;
use super::rollback::Rollback;
use crate::routes::AppState;

/// Which project a repository's pushes belong to (GIT-10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub repo: RepoRef,
    pub project: ProjectId,
    /// The branch whose pushes deploy to production.
    pub deploy_branch: String,
    /// The subdirectory the project lives in, normalised (`root: "docs"`).
    pub root: String,
    /// Branch patterns trusted beyond the deploy branch.
    /// TODO(rfc-1604): from `verify.sources.trustedBranches` once it exists.
    pub trusted_branches: Vec<String>,
    /// Where previews for this project are served.
    pub preview_domain: String,
    pub environments: Vec<Environment>,
}

impl Binding {
    pub fn new(repo: RepoRef, project: ProjectId, deploy_branch: impl Into<String>) -> Self {
        let deploy_branch = deploy_branch.into();
        Self {
            environments: vec![
                Environment::production(deploy_branch.clone()),
                Environment::preview(),
            ],
            repo,
            project,
            deploy_branch,
            root: String::new(),
            trusted_branches: Vec::new(),
            preview_domain: "preview.localhost".to_owned(),
        }
    }

    pub fn with_root(mut self, root: &str) -> Self {
        self.root = normalize_root(root);
        self
    }

    pub fn with_trusted_branches(mut self, patterns: Vec<String>) -> Self {
        self.trusted_branches = patterns;
        self
    }

    pub fn with_preview_domain(mut self, domain: impl Into<String>) -> Self {
        self.preview_domain = domain.into();
        self
    }

    pub fn with_environment(mut self, environment: Environment) -> Self {
        self.environments.retain(|e| e.name != environment.name);
        self.environments.push(environment);
        self
    }

    /// The environment a push to `branch` deploys to, or the preview
    /// environment when no environment claims it.
    pub fn environment_for(&self, branch: &str) -> &Environment {
        super::environment::for_branch(&self.environments, branch).unwrap_or_else(|| {
            super::environment::by_name(&self.environments, PREVIEW)
                .unwrap_or(&self.environments[0])
        })
    }

    /// Whether a push that touched only these paths changes anything this
    /// project builds (GIT-10). An empty list means the provider did not say,
    /// which is not the same as "nothing changed".
    pub fn touches(&self, changed: &[String]) -> bool {
        if self.root.is_empty() || changed.is_empty() {
            return true;
        }
        changed
            .iter()
            .any(|path| liyasa_git::repo::is_under_root(&self.root, path))
    }
}

/// The inbound webhook secrets, one verifier per provider so each keeps its
/// own replay memory.
#[derive(Debug, Default)]
pub struct Hooks {
    verifiers: BTreeMap<Provider, Verifier>,
}

impl Hooks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_secret(mut self, provider: Provider, secret: impl Into<String>) -> Self {
        self.verifiers.insert(provider, Verifier::new(secret));
        self
    }

    pub fn verifier(&self, provider: Provider) -> Option<&Verifier> {
        self.verifiers.get(&provider)
    }

    pub fn is_empty(&self) -> bool {
        self.verifiers.is_empty()
    }
}

/// Everything the deploy routes need.
#[derive(Debug)]
pub struct DeployState {
    pub app: Arc<AppState>,
    pub queue: DeployQueue,
    pub rollback: Rollback,
    pub retention: Retention,
    pub hooks: Hooks,
    bindings: Vec<Binding>,
}

impl DeployState {
    /// Returns `None` when the server was started without a store: every
    /// deploy route needs one, and a half-built state that fails per request
    /// would be worse than not being routed at all.
    pub fn new(app: Arc<AppState>) -> Option<Self> {
        let store = app.store.clone()?;
        Some(Self {
            queue: DeployQueue::new(store.clone()),
            rollback: Rollback::new(store.clone()),
            retention: Retention::new(store),
            hooks: Hooks::new(),
            bindings: Vec::new(),
            app,
        })
    }

    pub fn with_hooks(mut self, hooks: Hooks) -> Self {
        self.hooks = hooks;
        self
    }

    pub fn with_binding(mut self, binding: Binding) -> Self {
        self.bindings.retain(|b| b.repo != binding.repo);
        self.bindings.push(binding);
        self
    }

    pub fn with_queue(mut self, queue: DeployQueue) -> Self {
        self.queue = queue;
        self
    }

    pub fn with_rollback(mut self, rollback: Rollback) -> Self {
        self.rollback = rollback;
        self
    }

    pub fn with_retention(mut self, retention: Retention) -> Self {
        self.retention = retention;
        self
    }

    pub fn binding_for(&self, repo: &RepoRef) -> Option<&Binding> {
        self.bindings.iter().find(|binding| &binding.repo == repo)
    }

    pub fn binding_of(&self, project: &ProjectId) -> Option<&Binding> {
        self.bindings
            .iter()
            .find(|binding| &binding.project == project)
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectId {
        ProjectId(ulid::Ulid::from_bytes([9; 16]))
    }

    fn binding() -> Binding {
        Binding::new(RepoRef::new("kasecrab", "liyasa"), project(), "main")
    }

    #[test]
    fn a_push_to_the_deploy_branch_goes_to_production() {
        let binding = binding();
        assert_eq!(binding.environment_for("main").name, "production");
    }

    #[test]
    fn a_branch_nobody_claims_goes_to_the_preview_environment() {
        let binding = binding();
        assert_eq!(binding.environment_for("feat/x").name, PREVIEW);
    }

    #[test]
    fn a_named_environment_claims_its_own_branch() {
        let binding = binding()
            .with_environment(Environment::named("staging", "release").expect("a valid name"));
        assert_eq!(binding.environment_for("release").name, "staging");
        assert_eq!(binding.environment_for("main").name, "production");
    }

    #[test]
    fn a_monorepo_root_filters_the_pushes_that_build() {
        let binding = binding().with_root("docs");
        assert!(binding.touches(&["docs/index.md".to_owned()]));
        assert!(!binding.touches(&["src/main.rs".to_owned()]));
        assert!(
            binding.touches(&[]),
            "a provider that sent no path list is not evidence that nothing changed"
        );
    }

    #[test]
    fn a_project_at_the_repository_root_is_touched_by_everything() {
        assert!(binding().touches(&["src/main.rs".to_owned()]));
    }

    #[test]
    fn a_second_binding_for_the_same_repository_replaces_the_first() {
        let other = ProjectId(ulid::Ulid::from_bytes([10; 16]));
        let state_bindings = {
            let mut bindings: Vec<Binding> = Vec::new();
            for binding in [
                binding(),
                Binding::new(RepoRef::new("kasecrab", "liyasa"), other, "trunk"),
            ] {
                bindings.retain(|b| b.repo != binding.repo);
                bindings.push(binding);
            }
            bindings
        };
        assert_eq!(state_bindings.len(), 1);
        assert_eq!(state_bindings[0].project, other);
    }

    #[test]
    fn a_verifier_is_kept_per_provider() {
        let hooks = Hooks::new()
            .with_secret(Provider::GitHub, "a-github-secret-value")
            .with_secret(Provider::GitLab, "a-gitlab-token-value");
        assert!(hooks.verifier(Provider::GitHub).is_some());
        assert!(hooks.verifier(Provider::GitLab).is_some());
        assert!(hooks.verifier(Provider::Bitbucket).is_none());
        assert!(!hooks.is_empty());
    }
}
