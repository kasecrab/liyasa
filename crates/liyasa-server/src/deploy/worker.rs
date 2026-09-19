//! What runs a queued build (GIT-20, GIT-31; RFC 1404, RFC 1608).
//!
//! The deploy queue has enqueued `deploy.build` rows since WP-16 landed and
//! nothing claimed them. This is the handler WP-14's worker registers, and it
//! is deliberately only a handler: the claim-heartbeat-complete loop and the
//! lease live once, in `routes::work`, because the lease is the part that must
//! not be reimplemented per package.
//!
//! The order of the steps is the requirement, not an implementation detail. A
//! build that succeeds is made live *before* its embedding is queued (GIT-20,
//! AST-01), so a slow model provider can never hold a deploy open; and the
//! build's environment is decided *before* the build starts (GIT-31), because
//! an untrusted build that reads a token has already leaked it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use liyasa_build::engine::{self, Options, Report};
use liyasa_core::diagnostics::Severity;
use liyasa_core::ids::{BuildId, ProjectId};
use liyasa_core::store::BuildStatus;
use liyasa_store::records::{BuildRecord, JobRecord};
use serde_json::{Value, json};

use super::queue::{BuildOutcome, Class};
use super::untrusted::Sandbox;
use crate::routes::AppState;

/// What a handler did. Mirrors `routes::work::Outcome` (RFC 1404) without
/// depending on it, so this module compiles and is testable before that
/// worker merges; the registration converts between them in one line.
#[derive(Debug, Clone, PartialEq)]
pub enum Done {
    Ok(Value),
    Failed(String),
    /// A missing configuration rather than a fault: no store, no workspace.
    /// Completed rather than retried, so a build does not burn its attempts
    /// reaching `dead` to tell an operator nothing.
    Skipped(String),
}

/// Everything the handler needs off the job row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub project: ProjectId,
    pub env: String,
    pub branch: String,
    pub commit: String,
    pub untrusted: bool,
    pub workspace: Option<PathBuf>,
    pub base_commit: Option<String>,
    pub cache_from: Option<String>,
    pub pull_request: Option<u64>,
}

impl Plan {
    /// Reads a `deploy.build` payload. A row this cannot read is a programming
    /// error on the enqueue side rather than a build failure, so the caller
    /// reports it as such.
    pub fn of(job: &JobRecord) -> Option<Self> {
        let payload = &job.payload;
        let text = |key: &str| payload.get(key).and_then(Value::as_str);
        Some(Self {
            project: job
                .project
                .or_else(|| text("project").and_then(ProjectId::parse))?,
            env: text("env")?.to_owned(),
            branch: text("branch").unwrap_or_default().to_owned(),
            commit: text("commit").unwrap_or_default().to_owned(),
            untrusted: payload
                .get("untrusted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            workspace: text("workspace").map(PathBuf::from),
            base_commit: text("baseCommit").map(str::to_owned),
            cache_from: text("cacheFrom").map(str::to_owned),
            pull_request: payload.get("pullRequest").and_then(Value::as_u64),
        })
    }

    /// Whether this build may be made live when it succeeds. A preview is
    /// served from its own host and never points production at anything.
    pub fn deploys(&self) -> bool {
        self.pull_request.is_none() && self.env != super::environment::PREVIEW
    }
}

/// The build's environment, decided before the build runs (GIT-31).
///
/// An untrusted build gets the allow-listed map and nothing else, so
/// `{{ env("GITHUB_TOKEN") }}` renders undefined by construction rather than
/// by a filter someone has to remember to apply.
pub fn environment_for(plan: &Plan, ambient: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    match plan.untrusted {
        true => super::untrusted::keep_only(ambient.iter()),
        false => ambient.clone(),
    }
}

/// The options one build runs under.
pub fn options_for(plan: &Plan, environment: BTreeMap<String, String>) -> Options {
    Options {
        env: Some(plan.env.clone()),
        // A deploy build is never a draft build and never lenient: what is
        // published is what `liyasa build` would have published.
        drafts: false,
        // `cache_from` warms the artifact cache, so a clean build is the case
        // where there is nothing to warm from (GIT-20).
        clean: plan.cache_from.is_none(),
        environment: Some(environment),
        ..Options::default()
    }
}

/// Counts what a build reported, for the history join of RFC 1605.
pub fn tally(report: &Report) -> (u32, u32) {
    let count = |want: Severity| {
        report
            .diagnostics
            .iter()
            .filter(|d| d.severity == want)
            .count() as u32
    };
    (count(Severity::Error), count(Severity::Warning))
}

/// Builds `plan` and returns what it produced.
///
/// Split from [`run`] so a test can drive a real build without a store.
pub fn build(plan: &Plan, root: &Path, ambient: &BTreeMap<String, String>) -> Report {
    let environment = environment_for(plan, ambient);
    let options = options_for(plan, environment);
    // TODO(rfc-1601): `NoGit` until a git implementation lands. The build
    // clock then falls back to its own last rule, which is what W0707 already
    // describes for a project with no repository.
    engine::build(
        &liyasa_core::vfs::StdVfs::new(root),
        &liyasa_build::git::NoGit,
        root,
        &options,
    )
}

/// The handler WP-14's worker registers for `deploy.build` (RFC 1404).
pub async fn run_build(state: &Arc<AppState>, job: &JobRecord) -> Done {
    let Some(plan) = Plan::of(job) else {
        return Done::Failed("this build row is not a deploy.build payload".to_owned());
    };
    let Some(store) = state.store.clone() else {
        return Done::Skipped("this instance has no store to record a build in".to_owned());
    };
    let Some(root) = plan.workspace.clone() else {
        // RFC 1608: no workspace is a configuration this installation has not
        // made, not a build that failed.
        return Done::Skipped(format!(
            "no workspace is configured for `{}`, so there is nothing to build",
            plan.project
        ));
    };
    if !root.is_dir() {
        return Done::Failed(format!("`{}` is not a directory", root.display()));
    }

    let ambient: BTreeMap<String, String> = std::env::vars().collect();
    let plan_for_build = plan.clone();
    let root_for_build = root.clone();
    // The build is CPU-bound and synchronous; running it on the async worker's
    // thread would stall every other job on this replica for its duration.
    let report = match tokio::task::spawn_blocking(move || {
        build(&plan_for_build, &root_for_build, &ambient)
    })
    .await
    {
        Ok(report) => report,
        Err(error) => return Done::Failed(format!("the build panicked: {error}")),
    };

    let (errors, warnings) = tally(&report);
    let Some(build_id) = report.build_id else {
        return Done::Failed(format!(
            "the build produced no bundle: {errors} error(s), {warnings} warning(s)"
        ));
    };
    let dist = root.join("dist").to_string_lossy().into_owned();
    let failed = report.failed(false);
    let status = match failed {
        true => BuildStatus::Failed,
        false => BuildStatus::Succeeded,
    };

    let now = liyasa_store::now_ms();
    if let Err(error) = store
        .builds_typed()
        .put(&BuildRecord {
            id: build_id,
            project: plan.project,
            env: plan.env.clone(),
            status,
            dist,
            created_at: now,
            updated_at: now,
            version: 1,
        })
        .await
    {
        return Done::Failed(format!("the build record could not be written: {error}"));
    }

    let outcome = BuildOutcome::new(build_id.to_string()).with_diagnostics(errors, warnings);
    let body = json!({
        "buildId": build_id.to_string(),
        "project": plan.project.to_string(),
        "env": plan.env,
        "commit": plan.commit,
        "branch": plan.branch,
        "untrusted": plan.untrusted,
        "pages": report.pages,
        "errors": errors,
        "warnings": warnings,
        "cacheHits": report.cache_hits,
    });

    if failed {
        state.notify_webhook("deployment.failed", &body);
        return Done::Failed(format!(
            "the build reported {errors} error(s) and {warnings} warning(s)"
        ));
    }

    // The pointer moves first and the embedding is queued after, never the
    // other way round (GIT-20, AST-01).
    if plan.deploys()
        && let Err(error) = store
            .deployments_typed()
            .point(&plan.project, &plan.env, &build_id)
            .await
    {
        return Done::Failed(format!("the deployment pointer could not be moved: {error}"));
    }

    let embedding = super::queue::DeployQueue::new(store)
        .queue_embedding(&plan.project, &build_id.to_string())
        .await
        .ok()
        .flatten()
        .map(|id| id.to_string());

    state.notify_webhook("deployment.succeeded", &body);
    let mut result = serde_json::to_value(&outcome).unwrap_or(Value::Null);
    result["embeddingJobId"] = json!(embedding);
    result["deployed"] = json!(plan.deploys());
    Done::Ok(result)
}

/// The class a queued build was filed under, for a log line that says whether
/// a slow build was holding a production deploy or a preview.
pub fn class_of(job: &JobRecord) -> Class {
    Class::from_priority(job.priority)
}

/// A build id from a bundle's own bytes, for a caller that needs one before a
/// build has run.
pub fn placeholder_build_id(seed: &str) -> BuildId {
    BuildId(liyasa_core::ids::Fingerprint::of(seed.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::queue::{BuildRequest, Trigger};

    fn project() -> ProjectId {
        ProjectId(ulid::Ulid::from_bytes([4; 16]))
    }

    fn job_from(request: &BuildRequest) -> JobRecord {
        let enqueue = request.to_enqueue();
        JobRecord {
            id: liyasa_core::ids::JobId(ulid::Ulid::from_bytes([1; 16])),
            name: enqueue.name,
            key: enqueue.key,
            priority: enqueue.priority,
            state: liyasa_core::store::JobState::Leased,
            project: enqueue.project,
            payload: enqueue.payload,
            attempts: 1,
            max_attempts: 2,
            run_at: 0,
            lease_ms: 60_000,
            lease_until: None,
            worker: Some("w".to_owned()),
            result: None,
            error: None,
            created_at: 0,
            updated_at: 0,
            version: 1,
        }
    }

    fn request() -> BuildRequest {
        BuildRequest::new(
            project(),
            "production",
            Class::Production,
            "kasecrab/liyasa",
            "main",
            "abc123",
        )
        .with_trigger(Trigger::Push)
        .in_workspace(Some("/srv/site".to_owned()))
    }

    #[test]
    fn a_plan_reads_back_everything_the_enqueuer_put_on_the_row() {
        let plan = Plan::of(&job_from(&request())).expect("a readable payload");
        assert_eq!(plan.project, project());
        assert_eq!(plan.env, "production");
        assert_eq!(plan.branch, "main");
        assert_eq!(plan.commit, "abc123");
        assert_eq!(plan.workspace, Some(PathBuf::from("/srv/site")));
        assert!(!plan.untrusted);
        assert!(plan.deploys());
    }

    #[test]
    fn an_incremental_build_is_not_a_clean_one() {
        let request = request().incremental_from(Some("old".to_owned()), Some("blake3:aa".to_owned()));
        let plan = Plan::of(&job_from(&request)).expect("a readable payload");
        assert_eq!(plan.cache_from.as_deref(), Some("blake3:aa"));
        let options = options_for(&plan, BTreeMap::new());
        assert!(
            !options.clean,
            "a build with a cache to warm from must not start by emptying it"
        );

        let cold = Plan::of(&job_from(&request())).expect("a readable payload");
        assert!(options_for(&cold, BTreeMap::new()).clean);
    }

    #[test]
    fn a_preview_build_never_moves_a_pointer() {
        let request = request().for_pull_request(7);
        let plan = Plan::of(&job_from(&request)).expect("a readable payload");
        assert!(
            !plan.deploys(),
            "a pull-request build is served from its own host"
        );

        let preview = BuildRequest::new(
            project(),
            "preview",
            Class::Preview,
            "kasecrab/liyasa",
            "topic",
            "def",
        );
        let plan = Plan::of(&job_from(&preview)).expect("a readable payload");
        assert!(!plan.deploys(), "and neither does the preview environment");
    }

    #[test]
    fn an_untrusted_build_gets_the_allow_list_and_a_trusted_one_gets_the_lot() {
        let ambient: BTreeMap<String, String> = [
            ("PATH", "/usr/bin"),
            ("LIYASA_CACHE", "/var/cache"),
            ("GITHUB_TOKEN", "ghs_secret"),
            ("AWS_SECRET_ACCESS_KEY", "aws_secret"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();

        let trusted = Plan::of(&job_from(&request())).expect("a readable payload");
        assert_eq!(environment_for(&trusted, &ambient).len(), 4);

        let untrusted = Plan::of(&job_from(&request().untrusted())).expect("a readable payload");
        let seen = environment_for(&untrusted, &ambient);
        assert_eq!(seen.len(), 2, "PATH and LIYASA_CACHE only: {seen:?}");
        for leaked in ["GITHUB_TOKEN", "AWS_SECRET_ACCESS_KEY"] {
            assert!(!seen.contains_key(leaked), "{leaked} reached the build");
        }
    }

    #[test]
    fn the_untrusted_environment_is_what_the_build_is_actually_given() {
        // The point of GIT-31 is not that a map was computed correctly, it is
        // that the map reaches `env()`. `Options::env_value` is what the
        // template function reads.
        let ambient: BTreeMap<String, String> = [("GITHUB_TOKEN", "ghs_secret")]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        let plan = Plan::of(&job_from(&request().untrusted())).expect("a readable payload");
        let options = options_for(&plan, environment_for(&plan, &ambient));
        assert_eq!(
            options.env_value("GITHUB_TOKEN"),
            None,
            "`{{{{ env(\"GITHUB_TOKEN\") }}}}` must render undefined in an untrusted build"
        );

        let trusted = Plan::of(&job_from(&request())).expect("a readable payload");
        let options = options_for(&trusted, environment_for(&trusted, &ambient));
        assert_eq!(options.env_value("GITHUB_TOKEN").as_deref(), Some("ghs_secret"));
    }

    #[test]
    fn a_row_that_is_not_a_build_payload_is_refused_rather_than_guessed_at() {
        let mut job = job_from(&request());
        job.payload = json!({ "nothing": "useful" });
        job.project = None;
        assert_eq!(Plan::of(&job), None);
    }

    #[test]
    fn a_job_row_still_names_the_class_it_was_queued_under() {
        assert_eq!(class_of(&job_from(&request())), Class::Production);
        let preview = request().for_pull_request(3);
        let mut job = job_from(&preview);
        job.priority = Class::Preview.priority();
        assert_eq!(class_of(&job), Class::Preview);
    }

    #[test]
    fn a_placeholder_build_id_is_stable_for_its_seed() {
        assert_eq!(placeholder_build_id("a"), placeholder_build_id("a"));
        assert_ne!(placeholder_build_id("a"), placeholder_build_id("b"));
    }
}
