//! Deploy and preview builds on the build-worker queue (§6.13, GIT-20, GIT-24).
//!
//! Three rules from §6.13 and one from GIT-24 decide what runs next:
//! production outranks preview outranks scheduled verification outranks
//! re-index; within a class projects take turns, so one tenant's ten thousand
//! pages cannot starve anyone; a project runs `concurrencyPerProject` builds
//! at a time; and a newer push to a branch supersedes the build queued for the
//! older commit.
//!
//! The ordering is a pure function of the rows, not a query, for two reasons.
//! A dashboard has to show the same queue position the worker will honour, and
//! deriving both from one function is the only way that stays true. And the
//! rules are the part with requirements attached, so they are the part that
//! has to be testable without a database.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use liyasa_core::ids::{JobId, ProjectId};
use liyasa_core::store::{JobQuery, JobState, Page, StoreError};
use liyasa_store::SqliteStore;
use liyasa_store::jobs::Enqueue;
use liyasa_store::records::JobRecord;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The job name every deploy and preview build is queued under.
pub const JOB_NAME: &str = "deploy.build";

/// `server.builds.queue`.
pub const DEFAULT_QUEUE_CAP: u64 = 100;
/// `server.builds.concurrencyPerProject`.
pub const DEFAULT_CONCURRENCY_PER_PROJECT: u32 = 1;

/// How long a build may hold its lease before another worker may take it.
pub const BUILD_LEASE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// A queued build is retried, but not many times: a build that fails twice for
/// the same commit fails for a reason retrying will not fix.
pub const MAX_ATTEMPTS: u32 = 2;

/// The priority classes of §6.13, highest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Class {
    Reindex,
    Verification,
    Preview,
    Production,
}

impl Class {
    /// The `priority` column. Spaced by ten so a future class fits between two
    /// without renumbering rows that already exist.
    pub fn priority(self) -> i32 {
        match self {
            Self::Reindex => 10,
            Self::Verification => 20,
            Self::Preview => 30,
            Self::Production => 40,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reindex => "reindex",
            Self::Verification => "verification",
            Self::Preview => "preview",
            Self::Production => "production",
        }
    }

    pub fn from_priority(priority: i32) -> Self {
        match priority {
            p if p >= Self::Production.priority() => Self::Production,
            p if p >= Self::Preview.priority() => Self::Preview,
            p if p >= Self::Verification.priority() => Self::Verification,
            _ => Self::Reindex,
        }
    }

    /// The class a build for `environment_kind` belongs to.
    pub fn for_environment(kind: super::environment::EnvironmentKind) -> Self {
        match kind {
            super::environment::EnvironmentKind::Production => Self::Production,
            // A named environment is a deploy, not a preview: `staging` is
            // somewhere people look at, and queueing it behind every open pull
            // request would make it useless.
            super::environment::EnvironmentKind::Named => Self::Production,
            super::environment::EnvironmentKind::Preview => Self::Preview,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Trigger {
    Push,
    PullRequest,
    /// The dashboard or the REST API (GIT-21).
    Manual,
    Schedule,
}

impl Trigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Push => "push",
            Self::PullRequest => "pullRequest",
            Self::Manual => "manual",
            Self::Schedule => "schedule",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildRequest {
    pub project: ProjectId,
    pub env: String,
    pub class: Class,
    pub repo: String,
    pub branch: String,
    pub commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub trigger: Trigger,
    /// A fork's pull request, or a branch outside the trusted set (GIT-31).
    #[serde(default)]
    pub untrusted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<u64>,
}

impl BuildRequest {
    pub fn new(
        project: ProjectId,
        env: impl Into<String>,
        class: Class,
        repo: impl Into<String>,
        branch: impl Into<String>,
        commit: impl Into<String>,
    ) -> Self {
        Self {
            project,
            env: env.into(),
            class,
            repo: repo.into(),
            branch: branch.into(),
            commit: commit.into(),
            message: None,
            trigger: Trigger::Push,
            untrusted: false,
            pull_request: None,
        }
    }

    pub fn with_trigger(mut self, trigger: Trigger) -> Self {
        self.trigger = trigger;
        self
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn untrusted(mut self) -> Self {
        self.untrusted = true;
        self
    }

    pub fn for_pull_request(mut self, number: u64) -> Self {
        self.pull_request = Some(number);
        self
    }

    /// One live build per project and branch. A second push to the same branch
    /// supersedes the first (§6.13); a push to another branch queues behind it
    /// rather than replacing it.
    ///
    /// A pull request is keyed by its number, not its head branch: a pull
    /// request retargeted onto a new branch is still the same preview, and two
    /// pull requests from branches of the same name in different forks are not.
    pub fn key(&self) -> String {
        match self.pull_request {
            Some(number) => format!("{}:pr-{number}", self.project),
            None => format!("{}:{}", self.project, self.branch),
        }
    }

    pub fn payload(&self) -> Value {
        json!({
            "project": self.project.to_string(),
            "env": self.env,
            "class": self.class.as_str(),
            "repo": self.repo,
            "branch": self.branch,
            "commit": self.commit,
            "message": self.message,
            "trigger": self.trigger.as_str(),
            "untrusted": self.untrusted,
            "pullRequest": self.pull_request,
        })
    }

    pub fn to_enqueue(&self) -> Enqueue {
        Enqueue {
            priority: self.class.priority(),
            project: Some(self.project),
            payload: self.payload(),
            max_attempts: MAX_ATTEMPTS,
            lease: BUILD_LEASE,
            ..Enqueue::new(JOB_NAME, self.key())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub queue: u64,
    pub concurrency_per_project: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            queue: DEFAULT_QUEUE_CAP,
            concurrency_per_project: DEFAULT_CONCURRENCY_PER_PROJECT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accepted {
    Queued(JobId),
    /// The build queued for an older commit on the same branch was cancelled.
    Superseded {
        cancelled: JobId,
        queued: JobId,
    },
}

impl Accepted {
    pub fn id(&self) -> JobId {
        match self {
            Self::Queued(id) | Self::Superseded { queued: id, .. } => *id,
        }
    }

    pub fn superseded(&self) -> Option<JobId> {
        match self {
            Self::Superseded { cancelled, .. } => Some(*cancelled),
            Self::Queued(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QueueError {
    /// `server.builds.queue` is reached. The caller reports `E0809` and fires
    /// `build.queue_full` (§6.13).
    #[error("the build queue holds {depth} jobs, at its cap of {cap}")]
    Full { depth: u64, cap: u64 },
    #[error("{0}")]
    Store(String),
}

impl From<StoreError> for QueueError {
    fn from(error: StoreError) -> Self {
        Self::Store(error.to_string())
    }
}

/// What a worker records when a build finishes (GIT-21).
///
/// `BuildRecord` holds the build's id, environment, status and bundle path and
/// nothing else — no commit, no logs, no diagnostics, no verification report.
/// Those live here, in the job's result, and the history endpoint joins the
/// two on `build_id`. The alternative was five columns on a table this package
/// does not own (`plan/rfcs/1605-deployment-history-joins-the-job.md`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildOutcome {
    /// The build this job produced, as a `BuildId` string.
    pub build_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logs_url: Option<String>,
    #[serde(default)]
    pub errors: u32,
    #[serde(default)]
    pub warnings: u32,
    /// The verification report's summary line, or `None` when `verify` did not
    /// run for this build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<String>,
}

impl BuildOutcome {
    pub fn new(build_id: impl Into<String>) -> Self {
        Self {
            build_id: build_id.into(),
            ..Self::default()
        }
    }

    pub fn with_logs(mut self, url: impl Into<String>) -> Self {
        self.logs_url = Some(url.into());
        self
    }

    pub fn with_diagnostics(mut self, errors: u32, warnings: u32) -> Self {
        self.errors = errors;
        self.warnings = warnings;
        self
    }

    pub fn with_verification(mut self, summary: impl Into<String>) -> Self {
        self.verification = Some(summary.into());
        self
    }
}

/// A queued build, reduced to what the ordering rules look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pending {
    pub id: JobId,
    pub project: ProjectId,
    pub priority: i32,
    pub created_at: i64,
}

impl Pending {
    pub fn of(record: &JobRecord) -> Option<Self> {
        Some(Self {
            id: record.id,
            project: record.project?,
            priority: record.priority,
            created_at: record.created_at,
        })
    }
}

/// The order queued builds run in: priority class first, then projects taking
/// turns within the class, oldest build of each project first.
///
/// Round-robin is what keeps §6.13's fairness rule true. Sorting by age alone
/// would let a project that pushed ten commits at once occupy the whole class
/// before anyone else's first build started.
pub fn order(queued: &[Pending]) -> Vec<JobId> {
    let mut classes: BTreeMap<i32, BTreeMap<ProjectId, Vec<Pending>>> = BTreeMap::new();
    for job in queued {
        classes
            .entry(job.priority)
            .or_default()
            .entry(job.project)
            .or_default()
            .push(*job);
    }
    let mut out = Vec::with_capacity(queued.len());
    // Highest priority first; `BTreeMap` iterates ascending, so reverse.
    for (_, mut projects) in classes.into_iter().rev() {
        for jobs in projects.values_mut() {
            jobs.sort_by_key(|job| (job.created_at, job.id.to_string()));
        }
        // The project whose oldest build is oldest goes first in each round,
        // so a project that has been waiting does not lose its turn to one
        // that happens to sort earlier by id.
        let mut order: Vec<ProjectId> = projects.keys().copied().collect();
        order.sort_by_key(|project| {
            projects
                .get(project)
                .and_then(|jobs| jobs.first())
                .map(|job| (job.created_at, job.id.to_string()))
        });
        let deepest = projects.values().map(Vec::len).max().unwrap_or(0);
        for round in 0..deepest {
            for project in &order {
                if let Some(job) = projects.get(project).and_then(|jobs| jobs.get(round)) {
                    out.push(job.id);
                }
            }
        }
    }
    out
}

/// The build a worker should lease next: the first in [`order`] whose project
/// is under its concurrency limit.
pub fn next(queued: &[Pending], running: &[ProjectId], limits: &Limits) -> Option<JobId> {
    let mut busy: HashMap<ProjectId, u32> = HashMap::new();
    for project in running {
        *busy.entry(*project).or_default() += 1;
    }
    let by_id: HashMap<JobId, &Pending> = queued.iter().map(|job| (job.id, job)).collect();
    order(queued).into_iter().find(|id| {
        by_id.get(id).is_some_and(|job| {
            busy.get(&job.project).copied().unwrap_or(0) < limits.concurrency_per_project
        })
    })
}

/// One-based position in the queue, for the dashboard (§6.13).
pub fn position(queued: &[Pending], id: JobId) -> Option<u32> {
    order(queued)
        .into_iter()
        .position(|candidate| candidate == id)
        .and_then(|index| u32::try_from(index + 1).ok())
}

/// When a pending build is expected to start, given how long a build has been
/// taking and how many workers there are. An estimate, and reported as one:
/// with no measured duration yet there is nothing to estimate from.
pub fn estimated_start_ms(
    position: u32,
    average_build_ms: Option<i64>,
    workers: u32,
) -> Option<i64> {
    let average = average_build_ms?;
    let workers = i64::from(workers.max(1));
    let ahead = i64::from(position.saturating_sub(1));
    Some(ahead.saturating_mul(average) / workers)
}

/// The queue of §6.13 over the job store.
#[derive(Debug, Clone)]
pub struct DeployQueue {
    store: Arc<SqliteStore>,
    limits: Limits,
}

impl DeployQueue {
    pub fn new(store: Arc<SqliteStore>) -> Self {
        Self {
            store,
            limits: Limits::default(),
        }
    }

    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Every live build job, queued or leased.
    pub async fn live(&self) -> Result<Vec<JobRecord>, QueueError> {
        let mut out = Vec::new();
        for state in [JobState::Queued, JobState::Leased] {
            out.extend(
                self.store
                    .jobs_typed()
                    .list(
                        &JobQuery {
                            name: Some(JOB_NAME.to_owned()),
                            state: Some(state),
                            project: None,
                        },
                        Page {
                            cursor: None,
                            limit: 500,
                        },
                    )
                    .await?,
            );
        }
        Ok(out)
    }

    pub async fn depth(&self) -> Result<u64, QueueError> {
        Ok(self.store.jobs_typed().depth(Some(JOB_NAME)).await?)
    }

    /// Queues a build, superseding the one already queued for the same key.
    ///
    /// The cap is checked before the supersede, not after: a queue at its cap
    /// with a build already queued for this branch would otherwise accept work
    /// it has no room for, and a supersede that fails halfway would have
    /// cancelled a build and queued nothing in its place.
    pub async fn submit(&self, request: &BuildRequest) -> Result<Accepted, QueueError> {
        let key = request.key();
        let live = self.live().await?;
        let existing = live.iter().find(|job| job.key == key);
        if existing.is_none() {
            let depth = live.len() as u64;
            if depth >= self.limits.queue {
                return Err(QueueError::Full {
                    depth,
                    cap: self.limits.queue,
                });
            }
        }
        let cancelled = match existing {
            Some(job) => {
                self.store.jobs_typed().cancel(&job.id).await?;
                Some(job.id)
            }
            None => None,
        };
        let queued = self
            .store
            .jobs_typed()
            .enqueue(&request.to_enqueue())
            .await?;
        Ok(match cancelled {
            Some(cancelled) => Accepted::Superseded {
                cancelled,
                queued: queued.id(),
            },
            None => Accepted::Queued(queued.id()),
        })
    }

    /// The order the queued builds will run in, and what is already running.
    pub async fn pending(&self) -> Result<(Vec<Pending>, Vec<ProjectId>), QueueError> {
        let live = self.live().await?;
        let mut queued = Vec::new();
        let mut running = Vec::new();
        for job in &live {
            match job.state {
                JobState::Queued => queued.extend(Pending::of(job)),
                JobState::Leased => running.extend(job.project),
                _ => {}
            }
        }
        Ok((queued, running))
    }

    /// The build a worker should take next.
    pub async fn next(&self) -> Result<Option<JobId>, QueueError> {
        let (queued, running) = self.pending().await?;
        Ok(next(&queued, &running, &self.limits))
    }

    /// One-based queue position for a pending build (§6.13).
    pub async fn position(&self, id: JobId) -> Result<Option<u32>, QueueError> {
        let (queued, _) = self.pending().await?;
        Ok(position(&queued, id))
    }

    /// Records what a build produced (GIT-21).
    pub async fn complete(&self, id: &JobId, outcome: &BuildOutcome) -> Result<(), QueueError> {
        let result = serde_json::to_value(outcome).unwrap_or(Value::Null);
        self.store.jobs_typed().complete(id, result).await?;
        Ok(())
    }

    /// Records that a build failed. The job is retried until its attempts run
    /// out, which is what `Jobs::fail` decides.
    pub async fn fail(&self, id: &JobId, error: &str) -> Result<JobState, QueueError> {
        Ok(self.store.jobs_typed().fail(id, error).await?)
    }

    /// Finished build jobs for a project, newest first, keyed by the build
    /// each produced. A job with no recorded outcome is skipped: there is
    /// nothing to join it to.
    pub async fn outcomes(
        &self,
        project: &ProjectId,
        limit: u32,
    ) -> Result<Vec<(JobRecord, BuildOutcome)>, QueueError> {
        let mut out = Vec::new();
        for state in [JobState::Done, JobState::Failed, JobState::Dead] {
            let rows = self
                .store
                .jobs_typed()
                .list(
                    &JobQuery {
                        name: Some(JOB_NAME.to_owned()),
                        state: Some(state),
                        project: Some(*project),
                    },
                    Page {
                        cursor: None,
                        limit,
                    },
                )
                .await?;
            for job in rows {
                let Some(result) = job.result.clone() else {
                    continue;
                };
                let Ok(outcome) = serde_json::from_value::<BuildOutcome>(result) else {
                    continue;
                };
                if outcome.build_id.is_empty() {
                    continue;
                }
                out.push((job, outcome));
            }
        }
        out.sort_by_key(|(job, _)| std::cmp::Reverse(job.updated_at));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(byte: u8) -> ProjectId {
        ProjectId(ulid::Ulid::from_bytes([byte; 16]))
    }

    fn job(seq: u8, project: ProjectId, class: Class, created_at: i64) -> Pending {
        Pending {
            id: JobId(ulid::Ulid::from_bytes([seq; 16])),
            project,
            priority: class.priority(),
            created_at,
        }
    }

    #[test]
    fn production_runs_before_preview_however_long_the_preview_has_waited() {
        let a = project(1);
        let preview = job(1, a, Class::Preview, 0);
        let production = job(2, a, Class::Production, 1_000);
        assert_eq!(
            order(&[preview, production]),
            vec![production.id, preview.id]
        );
    }

    #[test]
    fn every_class_is_ordered_as_the_specification_lists_them() {
        let a = project(1);
        let jobs = [
            job(1, a, Class::Reindex, 0),
            job(2, a, Class::Verification, 0),
            job(3, a, Class::Preview, 0),
            job(4, a, Class::Production, 0),
        ];
        let ordered = order(&jobs);
        assert_eq!(
            ordered,
            vec![jobs[3].id, jobs[2].id, jobs[1].id, jobs[0].id],
            "production, preview, verification, re-index"
        );
    }

    #[test]
    fn two_projects_pushing_ten_commits_at_once_take_turns() {
        let (a, b) = (project(1), project(2));
        let mut jobs = Vec::new();
        // Project A's ten arrive first, which is the shape that starves B if
        // the queue is ordered by age alone.
        for n in 0..10u8 {
            jobs.push(job(n, a, Class::Production, i64::from(n)));
        }
        for n in 0..10u8 {
            jobs.push(job(100 + n, b, Class::Production, 100 + i64::from(n)));
        }
        let ordered = order(&jobs);
        assert_eq!(ordered.len(), 20);
        let owner = |id: JobId| {
            jobs.iter()
                .find(|job| job.id == id)
                .map(|job| job.project)
                .expect("a queued job")
        };
        assert_eq!(owner(ordered[0]), a, "A waited longest, so A goes first");
        assert_eq!(owner(ordered[1]), b, "then B, rather than nine more of A");
        assert_eq!(owner(ordered[2]), a);
        assert_eq!(owner(ordered[3]), b);
        let a_positions = ordered.iter().filter(|id| owner(**id) == a).count();
        assert_eq!(a_positions, 10);
    }

    #[test]
    fn a_projects_own_builds_stay_in_the_order_they_arrived() {
        let a = project(1);
        let second = job(2, a, Class::Production, 200);
        let first = job(1, a, Class::Production, 100);
        assert_eq!(order(&[second, first]), vec![first.id, second.id]);
    }

    #[test]
    fn per_project_concurrency_of_one_holds_the_second_build_back() {
        let (a, b) = (project(1), project(2));
        let first = job(1, a, Class::Production, 0);
        let second = job(2, a, Class::Production, 1);
        let other = job(3, b, Class::Production, 2);
        let limits = Limits::default();

        assert_eq!(next(&[first, second, other], &[], &limits), Some(first.id));
        assert_eq!(
            next(&[second, other], &[a], &limits),
            Some(other.id),
            "A is building, so B's goes next rather than A's second"
        );
        assert_eq!(
            next(&[second], &[a], &limits),
            None,
            "nothing else is runnable while A is at its limit"
        );
    }

    #[test]
    fn a_higher_concurrency_lets_a_project_run_two_at_once() {
        let a = project(1);
        let second = job(2, a, Class::Production, 1);
        let limits = Limits {
            concurrency_per_project: 2,
            ..Limits::default()
        };
        assert_eq!(next(&[second], &[a], &limits), Some(second.id));
        assert_eq!(next(&[second], &[a, a], &limits), None);
    }

    #[test]
    fn a_queue_position_is_one_based_and_matches_the_run_order() {
        let (a, b) = (project(1), project(2));
        let first = job(1, a, Class::Production, 0);
        let second = job(2, b, Class::Production, 1);
        let third = job(3, a, Class::Production, 2);
        let queued = [first, second, third];
        assert_eq!(position(&queued, first.id), Some(1));
        assert_eq!(position(&queued, second.id), Some(2));
        assert_eq!(position(&queued, third.id), Some(3));
        assert_eq!(position(&queued, job(9, a, Class::Preview, 0).id), None);
    }

    #[test]
    fn an_estimated_start_needs_a_measured_duration_to_exist() {
        assert_eq!(estimated_start_ms(1, None, 1), None);
        assert_eq!(estimated_start_ms(1, Some(10_000), 1), Some(0));
        assert_eq!(estimated_start_ms(3, Some(10_000), 1), Some(20_000));
        assert_eq!(
            estimated_start_ms(5, Some(10_000), 2),
            Some(20_000),
            "two workers halve the wait"
        );
        assert_eq!(estimated_start_ms(5, Some(10_000), 0), Some(40_000));
    }

    #[test]
    fn a_class_round_trips_through_its_priority() {
        for class in [
            Class::Reindex,
            Class::Verification,
            Class::Preview,
            Class::Production,
        ] {
            assert_eq!(Class::from_priority(class.priority()), class);
        }
    }

    #[test]
    fn a_named_environment_is_queued_as_a_deploy_rather_than_a_preview() {
        use super::super::environment::EnvironmentKind;
        assert_eq!(
            Class::for_environment(EnvironmentKind::Named),
            Class::Production
        );
        assert_eq!(
            Class::for_environment(EnvironmentKind::Preview),
            Class::Preview
        );
    }

    #[test]
    fn a_branch_keys_a_build_and_a_pull_request_keys_its_number() {
        let p = project(1);
        let push = BuildRequest::new(p, "production", Class::Production, "o/r", "main", "abc");
        assert_eq!(push.key(), format!("{p}:main"));

        let preview = BuildRequest::new(p, "preview", Class::Preview, "o/r", "patch-1", "def")
            .for_pull_request(7);
        assert_eq!(preview.key(), format!("{p}:pr-7"));
        let retargeted = BuildRequest::new(p, "preview", Class::Preview, "o/r", "renamed", "ghi")
            .for_pull_request(7);
        assert_eq!(
            retargeted.key(),
            preview.key(),
            "a retargeted pull request is still the same preview"
        );
    }

    #[test]
    fn a_request_carries_everything_the_worker_needs_into_its_payload() {
        let p = project(3);
        let request = BuildRequest::new(
            p,
            "production",
            Class::Production,
            "kasecrab/liyasa",
            "main",
            "abc123",
        )
        .with_message("docs: fix the install guide")
        .with_trigger(Trigger::Manual);
        let enqueue = request.to_enqueue();
        assert_eq!(enqueue.name, JOB_NAME);
        assert_eq!(enqueue.priority, Class::Production.priority());
        assert_eq!(enqueue.project, Some(p));
        assert_eq!(enqueue.max_attempts, MAX_ATTEMPTS);
        let payload = enqueue.payload;
        assert_eq!(payload["commit"], "abc123");
        assert_eq!(payload["trigger"], "manual");
        assert_eq!(payload["message"], "docs: fix the install guide");
        assert_eq!(payload["untrusted"], false);
    }

    #[test]
    fn an_untrusted_build_says_so_in_its_payload() {
        let request =
            BuildRequest::new(project(4), "preview", Class::Preview, "o/r", "patch", "abc")
                .for_pull_request(3)
                .untrusted();
        assert_eq!(request.payload()["untrusted"], true);
        assert_eq!(request.payload()["pullRequest"], 3);
    }

    #[test]
    fn an_outcome_carries_what_the_build_record_cannot() {
        let outcome = BuildOutcome::new("blake3:aa")
            .with_logs("https://liyasa.example/builds/1/logs")
            .with_diagnostics(2, 7)
            .with_verification("41 of 42 checks passed");
        let text = serde_json::to_string(&outcome).expect("an outcome serializes");
        assert!(text.contains("buildId"), "{text}");
        assert!(text.contains("logsUrl"), "{text}");
        let back: BuildOutcome = serde_json::from_str(&text).expect("it deserializes");
        assert_eq!(back, outcome);
        assert_eq!(back.errors, 2);
        assert_eq!(back.warnings, 7);
    }

    #[test]
    fn an_outcome_with_nothing_recorded_still_names_its_build() {
        let outcome = BuildOutcome::new("blake3:bb");
        let text = serde_json::to_string(&outcome).expect("an outcome serializes");
        assert!(!text.contains("logsUrl"), "{text}");
        assert!(!text.contains("verification"), "{text}");
        assert_eq!(outcome.build_id, "blake3:bb");
    }

    #[test]
    fn a_request_round_trips_as_json() {
        let request = BuildRequest::new(
            project(5),
            "staging",
            Class::Production,
            "o/r",
            "release",
            "abc",
        )
        .with_trigger(Trigger::Schedule);
        let text = serde_json::to_string(&request).expect("a request serializes");
        let back: BuildRequest = serde_json::from_str(&text).expect("it deserializes");
        assert_eq!(back, request);
    }
}
