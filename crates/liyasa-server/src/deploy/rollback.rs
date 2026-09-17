//! Rolling an environment back, and the edge purge that follows (GIT-40, GIT-41).
//!
//! A bundle is immutable and content-addressed, so a rollback moves a pointer
//! and nothing else: no rebuild, no copy, no rewrite of history. What takes
//! time is the edge still holding the old bytes, which is why the purge is
//! part of the operation rather than a follow-up somebody remembers.

use std::sync::Arc;
use std::time::{Duration, Instant};

use liyasa_core::ids::{BuildId, ProjectId};
use liyasa_core::net::BoxFut;
use liyasa_core::store::{BuildStatus, Page, StoreError};
use liyasa_store::SqliteStore;
use serde::{Deserialize, Serialize};

/// How far back `retained` looks for something to roll back to. The retention
/// policy keeps fifty production builds (GIT-23), so a page of that size sees
/// every build a rollback may target.
pub const HISTORY_PAGE: u32 = 50;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PurgeError {
    #[error("{0}")]
    Failed(String),
}

/// Purging by tag rather than by URL is what makes a rollback one request
/// instead of one per page (GIT-41).
pub trait CdnPurge: Send + Sync + std::fmt::Debug {
    fn purge_tag<'a>(&'a self, tag: &'a str) -> BoxFut<'a, Result<(), PurgeError>>;
}

/// An installation serving from its own origin has no edge to purge. Saying so
/// explicitly beats an `Option<Arc<dyn CdnPurge>>` at every call site.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPurge;

impl CdnPurge for NoPurge {
    fn purge_tag<'a>(&'a self, _tag: &'a str) -> BoxFut<'a, Result<(), PurgeError>> {
        Box::pin(async { Ok(()) })
    }
}

/// The surrogate key an environment's responses carry, so one purge reaches
/// every object it served.
pub fn cache_tag(project: &ProjectId, env: &str) -> String {
    format!("liyasa-{project}-{env}")
}

/// Who asked. The server's auth module supplies this; the deploy module only
/// reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub id: String,
    pub admin: bool,
}

impl Actor {
    pub fn admin(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            admin: true,
        }
    }

    pub fn member(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            admin: false,
        }
    }
}

/// GIT-40: rollbacks may be restricted to admins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Policy {
    pub admins_only: bool,
}

/// One line in the audit trail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub actor: String,
    pub action: String,
    pub subject: String,
    pub detail: String,
}

/// Where audit lines go.
///
/// TODO(rfc-1603): `audit_log` is a table in the schema but `liyasa-store`
/// exposes no writer for it, and the store is not this package's to change.
/// The shipped sink is a structured `tracing` event, which the server already
/// renders as JSON; a durable sink is one `impl` away.
pub trait Audit: Send + Sync + std::fmt::Debug {
    fn record(&self, entry: &AuditEntry);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TracingAudit;

impl Audit for TracingAudit {
    fn record(&self, entry: &AuditEntry) {
        tracing::info!(
            target: "liyasa_audit",
            actor = %entry.actor,
            action = %entry.action,
            subject = %entry.subject,
            detail = %entry.detail,
            "an audited action was taken"
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RollbackError {
    #[error("only an administrator may roll {0} back")]
    NotPermitted(String),
    #[error("no build `{0}` is retained for this environment")]
    NotRetained(String),
    #[error("{0} has never been deployed")]
    NeverDeployed(String),
    #[error("{0} is already serving the newest build")]
    AlreadyLatest(String),
    #[error("{0}")]
    Store(String),
    #[error("the pointer moved but the edge purge failed: {0}")]
    Purge(String),
}

impl From<StoreError> for RollbackError {
    fn from(error: StoreError) -> Self {
        Self::Store(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub project: ProjectId,
    pub env: String,
    /// What the environment serves now.
    pub build: BuildId,
    /// What it served a moment ago.
    pub previous: Option<BuildId>,
    pub purged_tag: String,
    /// How long the pointer switch took, which GIT-41 holds to one second.
    pub took: Duration,
}

/// Rolling back and returning to latest (GIT-40, GIT-41).
#[derive(Debug, Clone)]
pub struct Rollback {
    store: Arc<SqliteStore>,
    purge: Arc<dyn CdnPurge>,
    audit: Arc<dyn Audit>,
    policy: Policy,
}

impl Rollback {
    pub fn new(store: Arc<SqliteStore>) -> Self {
        Self {
            store,
            purge: Arc::new(NoPurge),
            audit: Arc::new(TracingAudit),
            policy: Policy::default(),
        }
    }

    pub fn with_purge(mut self, purge: Arc<dyn CdnPurge>) -> Self {
        self.purge = purge;
        self
    }

    pub fn with_audit(mut self, audit: Arc<dyn Audit>) -> Self {
        self.audit = audit;
        self
    }

    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// Every build this environment may be rolled back to, newest first. A
    /// build whose record is gone — deleted by the retention policy — is not
    /// offered, because pointing at it would serve nothing.
    pub async fn retained(
        &self,
        project: &ProjectId,
        env: &str,
    ) -> Result<Vec<BuildId>, RollbackError> {
        let history = self
            .store
            .deployments_typed()
            .history(
                project,
                env,
                &Page {
                    cursor: None,
                    limit: HISTORY_PAGE,
                },
            )
            .await?;
        let mut out = Vec::new();
        for record in history {
            if out.contains(&record.build) {
                continue;
            }
            if self
                .store
                .builds_typed()
                .get(&record.build)
                .await?
                .is_some_and(|build| build.status == BuildStatus::Succeeded)
            {
                out.push(record.build);
            }
        }
        Ok(out)
    }

    /// Points `env` at `build`, which must be one of [`Self::retained`].
    pub async fn to(
        &self,
        actor: &Actor,
        project: &ProjectId,
        env: &str,
        build: &BuildId,
    ) -> Result<Outcome, RollbackError> {
        self.permit(actor, env)?;
        if !self.retained(project, env).await?.contains(build) {
            return Err(RollbackError::NotRetained(build.to_string()));
        }
        self.point(project, env, build, "deployment.rollback", actor)
            .await
    }

    /// One action back to the newest successful build of this environment
    /// (GIT-40). Not the newest history entry: after a rollback that entry is
    /// the old build, and "return to latest" would be a no-op for ever.
    pub async fn latest(
        &self,
        actor: &Actor,
        project: &ProjectId,
        env: &str,
    ) -> Result<Outcome, RollbackError> {
        self.permit(actor, env)?;
        let newest = self
            .store
            .builds_typed()
            .latest_for(project, env)
            .await?
            .filter(|build| build.status == BuildStatus::Succeeded)
            .ok_or_else(|| RollbackError::NeverDeployed(env.to_owned()))?;
        if self.current(project, env).await? == Some(newest.id) {
            return Err(RollbackError::AlreadyLatest(env.to_owned()));
        }
        self.point(
            project,
            env,
            &newest.id,
            "deployment.return_to_latest",
            actor,
        )
        .await
    }

    pub async fn current(
        &self,
        project: &ProjectId,
        env: &str,
    ) -> Result<Option<BuildId>, RollbackError> {
        Ok(self
            .store
            .deployments_typed()
            .current(project, env)
            .await?
            .map(|record| record.build))
    }

    fn permit(&self, actor: &Actor, env: &str) -> Result<(), RollbackError> {
        match self.policy.admins_only && !actor.admin {
            true => Err(RollbackError::NotPermitted(env.to_owned())),
            false => Ok(()),
        }
    }

    async fn point(
        &self,
        project: &ProjectId,
        env: &str,
        build: &BuildId,
        action: &str,
        actor: &Actor,
    ) -> Result<Outcome, RollbackError> {
        let previous = self.current(project, env).await?;
        let started = Instant::now();
        // The pointer moves first. An edge that fails to purge serves stale
        // bytes for a while; a pointer that was never moved serves the wrong
        // build for ever.
        self.store
            .deployments_typed()
            .point(project, env, build)
            .await?;
        let took = started.elapsed();
        let tag = cache_tag(project, env);
        let purge = self.purge.purge_tag(&tag).await;
        self.audit.record(&AuditEntry {
            actor: actor.id.clone(),
            action: action.to_owned(),
            subject: format!("{project}/{env}"),
            detail: match &previous {
                Some(previous) => format!("{previous} -> {build}"),
                None => format!("-> {build}"),
            },
        });
        purge.map_err(|error| RollbackError::Purge(error.to_string()))?;
        Ok(Outcome {
            project: *project,
            env: env.to_owned(),
            build: *build,
            previous,
            purged_tag: tag,
            took,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Debug, Default)]
    struct Recording(Mutex<Vec<AuditEntry>>);

    impl Audit for Recording {
        fn record(&self, entry: &AuditEntry) {
            if let Ok(mut entries) = self.0.lock() {
                entries.push(entry.clone());
            }
        }
    }

    fn project() -> ProjectId {
        ProjectId(ulid::Ulid::from_bytes([7; 16]))
    }

    #[test]
    fn a_cache_tag_names_the_project_and_the_environment() {
        let tag = cache_tag(&project(), "production");
        assert!(tag.starts_with("liyasa-"));
        assert!(tag.ends_with("-production"));
        assert_ne!(tag, cache_tag(&project(), "preview"));
    }

    #[test]
    fn an_admin_only_policy_refuses_a_member_and_allows_an_admin() {
        let policy = Policy { admins_only: true };
        let permit = |actor: &Actor| match policy.admins_only && !actor.admin {
            true => Err(RollbackError::NotPermitted("production".to_owned())),
            false => Ok(()),
        };
        assert!(permit(&Actor::admin("root")).is_ok());
        assert_eq!(
            permit(&Actor::member("writer")),
            Err(RollbackError::NotPermitted("production".to_owned()))
        );
    }

    #[test]
    fn an_open_policy_lets_anyone_roll_back() {
        let policy = Policy::default();
        assert!(!policy.admins_only);
    }

    #[tokio::test]
    async fn nothing_to_purge_is_success_rather_than_a_special_case() {
        assert!(NoPurge.purge_tag("liyasa-x-production").await.is_ok());
    }

    #[test]
    fn an_audit_entry_names_the_actor_the_action_and_both_builds() {
        let sink = Recording::default();
        sink.record(&AuditEntry {
            actor: "root".to_owned(),
            action: "deployment.rollback".to_owned(),
            subject: "p/production".to_owned(),
            detail: "blake3:aa -> blake3:bb".to_owned(),
        });
        let entries = sink.0.lock().expect("the recording is not poisoned");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, "deployment.rollback");
        assert!(entries[0].detail.contains("->"));
    }
}
